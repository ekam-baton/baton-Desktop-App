/// Agent framework adapters — AutoGPT and BabyAGI.
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;

use super::AgentTransport;

pub struct AutoGptAdapter;

#[async_trait]
impl AgentTransport for AutoGptAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        info!("AutoGPT: dispatching task '{}'", tool_name);
        // TODO: Connect to a local AutoGPT instance via its REST API.
        Ok(json!({
            "status": "success",
            "framework": "AutoGPT",
            "task": tool_name,
            "arguments": arguments,
            "result": "Task submitted to AutoGPT framework."
        }))
    }
}

pub struct BabyAgiAdapter;

#[async_trait]
impl AgentTransport for BabyAgiAdapter {
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        info!("BabyAGI: dispatching objective '{}'", tool_name);
        Ok(json!({
            "status": "success",
            "framework": "BabyAGI",
            "objective": tool_name,
            "arguments": arguments,
            "result": "Objective submitted to BabyAGI."
        }))
    }
}
