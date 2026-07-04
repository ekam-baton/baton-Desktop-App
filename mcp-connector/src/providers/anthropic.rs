use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, ChatOptions, Provider, ProviderError};
use crate::config::Config;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(config: &Config, client: Client) -> Result<Self, ProviderError> {
        let api_key = config
            .anthropic_api_key
            .clone()
            .ok_or_else(|| ProviderError::NotConfigured("Anthropic (ANTHROPIC_API_KEY)".into()))?;
        Ok(Self { client, api_key, model: config.anthropic_model.clone() })
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &'static str { "anthropic" }

    async fn chat(&self, messages: &[ChatMessage], options: &ChatOptions) -> Result<String, ProviderError> {
        let (system, msgs) = split_system(messages, options);
        let mut body = json!({
            "model": self.model,
            "max_tokens": options.max_tokens.unwrap_or(2048),
            "messages": msgs,
        });
        if let Some(s) = system { body["system"] = json!(s); }

        let resp = self.client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send().await.map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let json: Value = resp.json().await.map_err(|e| ProviderError::Parse(e.to_string()))?;
        Ok(json["content"][0]["text"].as_str().unwrap_or("").to_owned())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
        tx: Sender<Result<String, ProviderError>>,
    ) -> Result<(), ProviderError> {
        let (system, msgs) = split_system(messages, options);
        let mut body = json!({
            "model": self.model,
            "max_tokens": options.max_tokens.unwrap_or(2048),
            "messages": msgs,
            "stream": true,
        });
        if let Some(s) = system { body["system"] = json!(s); }

        let resp = self.client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send().await.map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| ProviderError::Http(e.to_string()))?;
            for line in String::from_utf8_lossy(&bytes).lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        // Anthropic SSE: content_block_delta event
                        if json["type"].as_str() == Some("content_block_delta") {
                            if let Some(text) = json["delta"]["text"].as_str() {
                                if !text.is_empty() {
                                    let _ = tx.send(Ok(text.to_owned())).await;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn split_system(
    messages: &[ChatMessage],
    options: &ChatOptions,
) -> (Option<String>, Vec<Value>) {
    let system = options.system_prompt.clone().or_else(|| {
        messages.iter().find(|m| m.role == "system").map(|m| m.content.clone())
    });
    let msgs: Vec<Value> = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();
    (system, msgs)
}
