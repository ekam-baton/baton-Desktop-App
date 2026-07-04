use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;
use tracing::debug;

use super::{ChatMessage, ChatOptions, Provider, ProviderError};
use crate::config::Config;

pub struct NvidiaProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl NvidiaProvider {
    pub fn new(config: &Config, client: Client) -> Result<Self, ProviderError> {
        let api_key = config
            .nvidia_api_key
            .clone()
            .ok_or_else(|| ProviderError::NotConfigured("Nvidia NIM (NVIDIA_API_KEY)".into()))?;
        Ok(Self {
            client,
            api_key,
            base_url: config.nvidia_base_url.clone(),
            model: config.nvidia_model.clone(),
        })
    }

    fn build_body(&self, messages: &[ChatMessage], options: &ChatOptions, stream: bool) -> Value {
        let msgs: Vec<Value> = messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();

        json!({
            "model": self.model,
            "messages": msgs,
            "temperature": options.temperature.unwrap_or(0.7),
            "max_tokens": options.max_tokens.unwrap_or(2048),
            "stream": stream,
        })
    }
}

#[async_trait]
impl Provider for NvidiaProvider {
    fn name(&self) -> &'static str { "nvidianim" }

    async fn chat(&self, messages: &[ChatMessage], options: &ChatOptions) -> Result<String, ProviderError> {
        let body = self.build_body(messages, options, false);
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let json: Value = resp.json().await.map_err(|e| ProviderError::Parse(e.to_string()))?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        Ok(content)
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
        tx: Sender<Result<String, ProviderError>>,
    ) -> Result<(), ProviderError> {
        let body = self.build_body(messages, options, true);
        let url = format!("{}/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| ProviderError::Http(e.to_string()))?;
            let text = String::from_utf8_lossy(&bytes);

            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        return Ok(());
                    }
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                            if !delta.is_empty() {
                                debug!("Nvidia delta: {:?}", delta);
                                let _ = tx.send(Ok(delta.to_owned())).await;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
