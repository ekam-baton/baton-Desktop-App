use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, ChatOptions, Provider, ProviderError};
use crate::config::Config;

pub struct GeminiProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiProvider {
    pub fn new(config: &Config, client: Client) -> Result<Self, ProviderError> {
        let api_key = config
            .gemini_api_key
            .clone()
            .ok_or_else(|| ProviderError::NotConfigured("Gemini (GEMINI_API_KEY)".into()))?;
        Ok(Self { client, api_key, model: config.gemini_model.clone() })
    }

    fn messages_to_contents(&self, messages: &[ChatMessage]) -> Vec<Value> {
        messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| {
                let role = if m.role == "user" { "user" } else { "model" };
                json!({ "role": role, "parts": [{ "text": m.content }] })
            })
            .collect()
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &'static str { "gemini" }

    async fn chat(&self, messages: &[ChatMessage], options: &ChatOptions) -> Result<String, ProviderError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let contents = self.messages_to_contents(messages);
        let system = messages.iter().find(|m| m.role == "system").map(|m| m.content.clone())
            .or_else(|| options.system_prompt.clone());

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": options.temperature.unwrap_or(0.7),
                "maxOutputTokens": options.max_tokens.unwrap_or(2048),
            }
        });
        if let Some(sys) = system {
            body["systemInstruction"] = json!({ "parts": [{ "text": sys }] });
        }

        let resp = self.client.post(&url).json(&body).send().await
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let json: Value = resp.json().await.map_err(|e| ProviderError::Parse(e.to_string()))?;
        Ok(json["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or("").to_owned())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
        tx: Sender<Result<String, ProviderError>>,
    ) -> Result<(), ProviderError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}&alt=sse",
            self.model, self.api_key
        );

        let contents = self.messages_to_contents(messages);
        let system = messages.iter().find(|m| m.role == "system").map(|m| m.content.clone())
            .or_else(|| options.system_prompt.clone());

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": options.temperature.unwrap_or(0.7),
                "maxOutputTokens": options.max_tokens.unwrap_or(2048),
            }
        });
        if let Some(sys) = system {
            body["systemInstruction"] = json!({ "parts": [{ "text": sys }] });
        }

        let resp = self.client.post(&url).json(&body).send().await
            .map_err(|e| ProviderError::Http(e.to_string()))?;

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
                        if let Some(text) = json["candidates"][0]["content"]["parts"][0]["text"].as_str() {
                            if !text.is_empty() {
                                let _ = tx.send(Ok(text.to_owned())).await;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
