/// Ollama local provider — talks to a running Ollama instance.
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, ChatOptions, Provider, ProviderError};
use crate::config::Config;

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(config: &Config, client: Client) -> Self {
        Self {
            client,
            base_url: config.ollama_base_url.clone(),
            model: config.ollama_model.clone(),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &'static str { "ollama" }

    async fn chat(&self, messages: &[ChatMessage], options: &ChatOptions) -> Result<String, ProviderError> {
        let msgs: Vec<Value> = messages.iter().map(|m| json!({ "role": m.role, "content": m.content })).collect();
        let body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": false,
            "options": {
                "temperature": options.temperature.unwrap_or(0.7),
                "num_predict": options.max_tokens.unwrap_or(2048),
            }
        });

        let resp = self.client.post(format!("{}/api/chat", self.base_url))
            .json(&body).send().await.map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let json: Value = resp.json().await.map_err(|e| ProviderError::Parse(e.to_string()))?;
        Ok(json["message"]["content"].as_str().unwrap_or("").to_owned())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
        tx: Sender<Result<String, ProviderError>>,
    ) -> Result<(), ProviderError> {
        let msgs: Vec<Value> = messages.iter().map(|m| json!({ "role": m.role, "content": m.content })).collect();
        let body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": true,
            "options": {
                "temperature": options.temperature.unwrap_or(0.7),
                "num_predict": options.max_tokens.unwrap_or(2048),
            }
        });

        let resp = self.client.post(format!("{}/api/chat", self.base_url))
            .json(&body).send().await.map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        // Ollama streams newline-delimited JSON (not SSE)
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| ProviderError::Http(e.to_string()))?;
            for line in String::from_utf8_lossy(&bytes).lines() {
                if line.is_empty() { continue; }
                if let Ok(json) = serde_json::from_str::<Value>(line) {
                    if let Some(content) = json["message"]["content"].as_str() {
                        if !content.is_empty() {
                            let _ = tx.send(Ok(content.to_owned())).await;
                        }
                    }
                    if json["done"].as_bool().unwrap_or(false) {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}
