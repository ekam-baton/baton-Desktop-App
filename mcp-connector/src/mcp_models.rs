use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request.
/// `id` is kept as a `Value` to correctly handle both string and numeric IDs per spec.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Whitelist of methods that clients are allowed to invoke via MCP.
pub const ALLOWED_METHODS: &[&str] = &[
    "initialize",
    "notifications/initialized",
    "tools/list",
    "tools/call",
];

impl McpRequest {
    pub fn new(id: Value, method: String, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method,
            params,
        }
    }

    /// Returns a String representation of the ID for matching purposes.
    pub fn id_str(&self) -> String {
        match &self.id {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => self.id.to_string(),
        }
    }

    /// Returns true if the method is on the server's allowlist.
    pub fn is_method_allowed(&self) -> bool {
        ALLOWED_METHODS.contains(&self.method.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl McpResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: String, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError { code, message, data }),
        }
    }

    /// Produce a generic "method not found" error (JSON-RPC code -32601).
    pub fn method_not_found(id: Value) -> Self {
        Self::error(id, -32601, "Method not found".to_string(), None)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpToolCallParams {
    pub name: String,
    pub arguments: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct McpInitializeParams {
    pub protocol_version: Option<String>,
    pub capabilities: Value,
    pub client_info: Value,
}
