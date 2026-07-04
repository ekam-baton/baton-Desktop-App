/// Cloud adapters — HuggingFace and PrivateCloud.
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use super::{AgentCredentials, AgentTransport};

pub struct HuggingFaceAdapter;

#[async_trait]
impl AgentTransport for HuggingFaceAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        info!("HuggingFace Inference: calling model '{}'", tool_name);
        // tool_name is treated as the model ID on HuggingFace Hub.
        // TODO: Use reqwest to call https://api-inference.huggingface.co/models/{tool_name}
        let api_key = std::env::var("HUGGINGFACE_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            return Err("HUGGINGFACE_API_KEY is not configured".into());
        }

        let client = reqwest::Client::new();
        let inputs = arguments.get("inputs").cloned().unwrap_or(arguments.clone());

        let resp = client
            .post(format!(
                "https://api-inference.huggingface.co/models/{}",
                tool_name
            ))
            .bearer_auth(&api_key)
            .json(&json!({ "inputs": inputs }))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("HuggingFace API error: {}", resp.status()));
        }

        let result: Value = resp.json().await.map_err(|e| e.to_string())?;
        Ok(json!({
            "status": "success",
            "provider": "HuggingFace",
            "model": tool_name,
            "result": result
        }))
    }
}

pub struct PrivateCloudAdapter {
    pub credentials: Option<AgentCredentials>,
}

#[async_trait]
impl AgentTransport for PrivateCloudAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        let creds = self.credentials.as_ref().ok_or("PrivateCloud requires credentials")?;

        info!("PrivateCloud: calling tool '{}' with {}", tool_name, creds.token_type);

        let endpoint = arguments
            .get("endpoint")
            .and_then(|v| v.as_str())
            .ok_or("PrivateCloud requires 'endpoint' in arguments")?;

        let client = reqwest::Client::new();
        let resp = client
            .post(endpoint)
            .header(
                "Authorization",
                format!("{} {}", creds.token_type, creds.token),
            )
            .json(arguments)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("PrivateCloud endpoint returned {}", resp.status()));
        }

        let result: Value = resp.json().await.map_err(|e| e.to_string())?;
        Ok(json!({
            "status": "success",
            "provider": "PrivateCloud",
            "tool": tool_name,
            "result": result
        }))
    }
}
