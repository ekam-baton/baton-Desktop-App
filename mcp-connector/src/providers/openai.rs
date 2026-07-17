/// OpenAI-compatible provider — works with OpenAI, Groq, Together AI, Perplexity, etc.
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, ChatOptions, Provider, ProviderError};
use crate::config::Config;

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(config: &Config, client: Client) -> Result<Self, ProviderError> {
        let api_key = config
            .openai_api_key
            .clone()
            .ok_or_else(|| ProviderError::NotConfigured("OpenAI (OPENAI_API_KEY)".into()))?;
        Ok(Self {
            client,
            api_key,
            base_url: config.openai_base_url.clone(),
            model: config.openai_model.clone(),
        })
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn name(&self) -> &'static str { "openai" }

    async fn chat(&self, messages: &[ChatMessage], options: &ChatOptions) -> Result<String, ProviderError> {
        let msgs: Vec<Value> = messages.iter().map(|m| json!({ "role": m.role, "content": m.content })).collect();
        let body = json!({
            "model": self.model,
            "messages": msgs,
            "temperature": options.temperature.unwrap_or(0.7),
            "max_tokens": options.max_tokens.unwrap_or(2048),
        });

        let resp = self.client.post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await.map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let json: Value = resp.json().await.map_err(|e| ProviderError::Parse(e.to_string()))?;
        Ok(json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_owned())
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
            "temperature": options.temperature.unwrap_or(0.7),
            "max_tokens": options.max_tokens.unwrap_or(2048),
            "stream": true,
        });

        let resp = self.client.post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
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
                    if data.trim() == "[DONE]" { return Ok(()); }
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                            if !delta.is_empty() {
                                let _ = tx.send(Ok(delta.to_owned())).await;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        let body = json!({
            "model": "text-embedding-3-small", // Standard fast embedding model
            "input": texts,
        });

        let resp = self.client.post(format!("{}/embeddings", self.base_url.trim_end_matches("/chat/completions")))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await.map_err(|e| ProviderError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: msg });
        }

        let mut json: Value = resp.json().await.map_err(|e| ProviderError::Parse(e.to_string()))?;
        
        let data = json["data"].as_array_mut().ok_or_else(|| ProviderError::Parse("Missing data array".into()))?;
        
        // Ensure they are sorted by index
        data.sort_by_key(|d| d["index"].as_u64().unwrap_or(0));
        
        let mut embeddings = Vec::with_capacity(data.len());
        for item in data {
            let vec: Vec<f32> = item["embedding"]
                .as_array()
                .ok_or_else(|| ProviderError::Parse("Missing embedding".into()))?
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            embeddings.push(vec);
        }

        Ok(embeddings)
    }
}
