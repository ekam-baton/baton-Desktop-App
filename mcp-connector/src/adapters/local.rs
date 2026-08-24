/// Local harness adapters — OpenClaw, Hermes, NemoClaw.
/// These connect to locally-running agent harnesses via IPC.
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use super::AgentTransport;

pub struct OpenClawAdapter;

#[async_trait]
impl AgentTransport for OpenClawAdapter {
    async fn execute_tool(&self, _tool_name: &str, _arguments: &Value) -> Result<Value, String> {
        Ok(json!({
            "error": "This adapter is not yet implemented. It will be available in a future release.",
            "status": "not_available"
        }))
    }
}

pub struct HermesAdapter;

#[async_trait]
impl AgentTransport for HermesAdapter {
    async fn execute_tool(&self, _tool_name: &str, _arguments: &Value) -> Result<Value, String> {
        Ok(json!({
            "error": "This adapter is not yet implemented. It will be available in a future release.",
            "status": "not_available"
        }))
    }
}

pub struct NemoClawAdapter;

#[async_trait]
impl AgentTransport for NemoClawAdapter {
    async fn execute_tool(&self, _tool_name: &str, _arguments: &Value) -> Result<Value, String> {
        Ok(json!({
            "error": "This adapter is not yet implemented. It will be available in a future release.",
            "status": "not_available"
        }))
    }
}
