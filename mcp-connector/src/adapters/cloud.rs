/// Cloud adapters — HuggingFace and PrivateCloud.
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use super::{AgentCredentials, AgentTransport};

pub struct HuggingFaceAdapter;

#[async_trait]
impl AgentTransport for HuggingFaceAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        // Validate model ID: only allow safe alphanumeric/hyphen/slash characters
        // Prevents path traversal like "../../v1/admin" injected as the model name.
        if tool_name.is_empty()
            || tool_name.len() > 200
            || !tool_name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
            || tool_name.contains("..")
        {
            return Err("Invalid model ID format".into());
        }

        info!("HuggingFace Inference: calling model '{}'", tool_name);
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

        // SSRF prevention: only allow https:// endpoints.
        // Blocks: http://, file://, ftp://, and internal addresses.
        let parsed_url = reqwest::Url::parse(endpoint)
            .map_err(|_| "Invalid endpoint URL".to_string())?;
        if parsed_url.scheme() != "https" {
            return Err("PrivateCloud endpoint must use https://".into());
        }
        // Block access to private/loopback addresses
        let host = parsed_url.host_str().unwrap_or("");
        if host == "localhost"
            || host.starts_with("127.")
            || host.starts_with("10.")
            || host.starts_with("192.168.")
            || host.starts_with("169.254.")
            || host == "[::1]"
        {
            return Err("PrivateCloud endpoint must not target internal addresses".into());
        }

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
