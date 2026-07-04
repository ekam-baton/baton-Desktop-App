/// Local harness adapters — OpenClaw, Hermes, NemoClaw.
/// These connect to locally-running agent harnesses via IPC.
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use super::AgentTransport;

pub struct OpenClawAdapter;

#[async_trait]
impl AgentTransport for OpenClawAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        info!("OpenClaw: executing tool '{}'", tool_name);
        // TODO: Connect via named pipe / IPC to the local OpenClaw harness.
        // For now, returns a structured stub response.
        Ok(json!({
            "status": "success",
            "harness": "OpenClaw",
            "tool": tool_name,
            "arguments": arguments,
            "result": "Tool dispatched to local OpenClaw harness."
        }))
    }
}

pub struct HermesAdapter;

#[async_trait]
impl AgentTransport for HermesAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        info!("Hermes: executing workflow '{}'", tool_name);
        Ok(json!({
            "status": "success",
            "harness": "Hermes",
            "tool": tool_name,
            "arguments": arguments,
            "result": "Workflow dispatched to local Hermes harness."
        }))
    }
}

pub struct NemoClawAdapter;

#[async_trait]
impl AgentTransport for NemoClawAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        info!("NemoClaw: executing tool '{}'", tool_name);
        Ok(json!({
            "status": "success",
            "harness": "NemoClaw",
            "tool": tool_name,
            "arguments": arguments,
            "result": "Tool dispatched to local NemoClaw harness."
        }))
    }
}
