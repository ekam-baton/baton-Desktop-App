use std::sync::Arc;
use axum::{
    extract::{State, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use axum::response::sse::{Event, Sse};
use std::convert::Infallible;

use crate::auth::validate_access_token;
use crate::mcp_models::{McpRequest, McpResponse};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct McpQuery {
    pub target: Option<String>,
}

/// Validate the target agent name: only lowercase alphanumeric and hyphens, max 64 chars.
fn validate_target(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub async fn initialize_handler(
    headers: HeaderMap,
    Query(query): Query<McpQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_access_token(&headers, &state.config.jwt_secret) {
        return e.into_response();
    }

    if payload.jsonrpc != "2.0" {
        return (
            StatusCode::BAD_REQUEST,
            Json(McpResponse::error(payload.id, -32600, "Invalid JSON-RPC version".into(), None))
        ).into_response();
    }

    let raw_target = query.target.unwrap_or_else(|| "openclaw".to_string()).to_lowercase();
    if !validate_target(&raw_target) {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32602, "Invalid target name".into(), None))).into_response();
    }

    let client = match state.mcp_clients.get(&raw_target) {
        Some(c) => c.clone(),
        None => return (StatusCode::NOT_FOUND, Json(McpResponse::error(payload.id, -32601, "Agent not found".into(), None))).into_response(),
    };

    match client.send_request(&payload).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => {
            tracing::warn!("initialize agent error (fallback to local init): {}", e);
            // If the agent doesn't support initialize or fails, fallback to connector response
            let capabilities = json!({
                "tools": { "listChanged": false }
            });
            (StatusCode::OK, Json(McpResponse::success(payload.id, json!({
                "protocolVersion": "2024-11-05",
                "capabilities": capabilities,
                "serverInfo": {
                    "name": "baton-connector",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })))).into_response()
        }
    }
}

pub async fn notifications_initialized_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(e) = validate_access_token(&headers, &state.config.jwt_secret) {
        return e.into_response();
    }
    StatusCode::OK.into_response()
}

pub async fn tools_list_handler(
    headers: HeaderMap,
    Query(query): Query<McpQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_access_token(&headers, &state.config.jwt_secret) {
        return e.into_response();
    }

    if payload.jsonrpc != "2.0" {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32600, "Invalid JSON-RPC version".into(), None))).into_response();
    }

    if !payload.is_method_allowed() {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::method_not_found(payload.id))).into_response();
    }

    let raw_target = query.target.unwrap_or_else(|| "openclaw".to_string()).to_lowercase();
    if !validate_target(&raw_target) {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32602, "Invalid target name".into(), None))).into_response();
    }

    let client = match state.mcp_clients.get(&raw_target) {
        Some(c) => c.clone(),
        None => return (StatusCode::NOT_FOUND, Json(McpResponse::error(payload.id, -32601, "Agent not found".into(), None))).into_response(),
    };

    match client.send_request(&payload).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => {
            tracing::error!("tools/list agent error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(McpResponse::error(payload.id, -32000, "Agent error".into(), None))).into_response()
        }
    }
}

pub async fn tools_call_handler(
    headers: HeaderMap,
    Query(query): Query<McpQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_access_token(&headers, &state.config.jwt_secret) {
        return e.into_response();
    }

    if payload.jsonrpc != "2.0" {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32600, "Invalid JSON-RPC version".into(), None))).into_response();
    }

    if !payload.is_method_allowed() {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::method_not_found(payload.id))).into_response();
    }

    let raw_target = query.target.unwrap_or_else(|| "openclaw".to_string()).to_lowercase();
    if !validate_target(&raw_target) {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32602, "Invalid target name".into(), None))).into_response();
    }

    let client = match state.mcp_clients.get(&raw_target) {
        Some(c) => c.clone(),
        None => {
            let err = McpResponse::error(payload.id.clone(), -32601, "Agent not found".into(), None);
            return (StatusCode::NOT_FOUND, Json(err)).into_response();
        }
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<String, String>>(10);

    tokio::spawn(async move {
        match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            client.send_request(&payload)
        ).await {
            Ok(Ok(res)) => {
                match serde_json::to_string(&res) {
                    Ok(s) => { let _ = tx.send(Ok(s)).await; }
                    Err(e) => {
                        tracing::error!("tools/call serialization error: {}", e);
                        let _ = tx.send(Err("Serialization error".to_string())).await;
                    }
                }
            }
            Ok(Err(e)) => {
                // Do NOT leak internal agent error details to the caller.
                tracing::error!("tools/call agent error: {}", e);
                let err_res = McpResponse::error(payload.id.clone(), -32000, "Agent error".into(), None);
                match serde_json::to_string(&err_res) {
                    Ok(s) => { let _ = tx.send(Ok(s)).await; }
                    Err(_) => { let _ = tx.send(Err("Agent error".to_string())).await; }
                }
            }
            Err(_timeout) => {
                tracing::error!("tools/call agent timed out for method={}", payload.method);
                let err_res = McpResponse::error(payload.id.clone(), -32001, "Agent timeout".into(), None);
                match serde_json::to_string(&err_res) {
                    Ok(s) => { let _ = tx.send(Ok(s)).await; }
                    Err(_) => { let _ = tx.send(Err("Timeout".to_string())).await; }
                }
            }
        }
    });

    let stream = async_stream::stream! {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(text) => yield Ok::<Event, Infallible>(Event::default().data(text)),
                Err(e) => yield Ok::<Event, Infallible>(Event::default().event("error").data(e)),
            }
        }
        // Send the [DONE] marker expected by HttpSseMcpTransport.kt
        yield Ok::<Event, Infallible>(Event::default().data("[DONE]"));
    };

    Sse::new(stream).into_response()
}
