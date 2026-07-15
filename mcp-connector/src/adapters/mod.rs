/// Legacy adapter system — routes to local harnesses and agent frameworks.
/// These wrap the original `AgentTransport` / `execute_tool` pattern.
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

pub mod local;
pub mod frameworks;
pub mod cloud;
pub mod stdio;

/// Legacy tool-call trait (mirrors the original mcp-connector design).
#[async_trait]
pub trait AgentTransport: Send + Sync {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String>;
}

/// Credentials for private cloud adapters.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AgentCredentials {
    pub token: String,
    pub token_type: String,
}

/// Resolve a `target_environment` string to an `AgentTransport` impl.
pub fn get_adapter(
    target_environment: &str,
    credentials: Option<AgentCredentials>,
) -> Result<Box<dyn AgentTransport>, String> {
    match target_environment {
        "OpenClaw"    => Ok(Box::new(local::OpenClawAdapter)),
        "Hermes"      => Ok(Box::new(local::HermesAdapter)),
        "NemoClaw"    => Ok(Box::new(local::NemoClawAdapter)),
        "AutoGPT"     => Ok(Box::new(frameworks::AutoGptAdapter)),
        "BabyAGI"     => Ok(Box::new(frameworks::BabyAgiAdapter)),
        "HuggingFace" => Ok(Box::new(cloud::HuggingFaceAdapter)),
        "PrivateCloud" => Ok(Box::new(cloud::PrivateCloudAdapter { credentials })),
        other => Err(format!("Unknown target environment: {}", other)),
    }
}
