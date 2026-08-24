use std::sync::Arc;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{error, info};
use axum::response::sse::{Event, Sse};
use futures::stream::Stream;
use std::convert::Infallible;

use crate::auth::validate_access_token;
use crate::adapters::{get_adapter, AgentCredentials};
use crate::providers::{get_provider, ChatMessage, ChatOptions};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct McpRequest {
    pub target_environment: String,
    pub tool_name: String,
    pub arguments: Value,
    pub credentials: Option<AgentCredentials>,
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub provider: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub options: Option<ChatOptions>,
}

/// Executes an MCP tool on the target environment (local harness, agent framework, private cloud).
pub async fn execute_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    // Auth validation
    let claims = match validate_access_token(&headers, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    if payload.tool_name.is_empty() || payload.tool_name.len() > 100 {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid tool_name"}))).into_response();
    }

    info!(
        "Client {} executing MCP tool '{}' on '{}'",
        claims.sub, payload.tool_name, payload.target_environment
    );

    // ── DLP Pre-Flight: scan tool arguments before executing (Pillar 5) ───
    let args_str = payload.arguments.to_string();
    let dlp_violations = state.dlp_engine.scan(&args_str);
    if !dlp_violations.is_empty() {
        let labels: Vec<&str> = dlp_violations.iter().map(|v| v.label).collect();
        tracing::warn!(
            "🛡️ DLP BLOCK: tool '{}' arguments contain sensitive data patterns: {:?}. Request blocked.",
            payload.tool_name, labels
        );
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "DLP Policy Violation",
                "detail": format!("Request blocked: potential sensitive data detected ({}).", labels.join(", ")),
                "patterns": labels,
            }))
        ).into_response();
    }

    let transport = match get_adapter(&payload.target_environment, payload.credentials) {
        Ok(t) => t,
        Err(e) => {
            error!("Unknown target environment: {}", payload.target_environment);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": e})),
            ).into_response();
        }
    };

    let result = match transport.execute_tool(&payload.tool_name, &payload.arguments).await {
        Ok(res) => {
            if res.get("status").and_then(|s| s.as_str()) == Some("not_available") {
                return (StatusCode::NOT_IMPLEMENTED, Json(res)).into_response();
            }
            res
        },
        Err(e) => {
            state.provider_errors.with_label_values(&[&payload.target_environment, "execute_error"]).inc();
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response();
        }
    };

    state.requests_total.with_label_values(&[&payload.target_environment, "success"]).inc();

    (StatusCode::OK, Json(json!({
        "status": "completed",
        "result": result,
    }))).into_response()
}

/// Streams a chat completion from an LLM provider (optional utility).
pub async fn chat_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_access_token(&headers, &state.config.jwt_secret) {
        return e.into_response();
    }

    let provider_name = payload.provider.unwrap_or_else(|| state.config.default_provider.clone());
    let provider = match get_provider(&provider_name, &state.config, &state.http) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))).into_response();
        }
    };

    info!("Streaming chat via provider: {}", provider.name());

    // ── DLP Pre-Flight: scan all message content before calling LLM (Pillar 5) ──
    let combined_prompt: String = payload.messages.iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let dlp_violations = state.dlp_engine.scan(&combined_prompt);
    if !dlp_violations.is_empty() {
        let labels: Vec<&str> = dlp_violations.iter().map(|v| v.label).collect();
        tracing::warn!(
            "🛡️ DLP BLOCK: chat prompt to '{}' contains sensitive data patterns: {:?}. Request blocked.",
            provider_name, labels
        );
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "DLP Policy Violation",
                "detail": format!("Request blocked: potential sensitive data detected ({}).", labels.join(", ")),
                "patterns": labels,
            }))
        ).into_response();
    }

    let (tx, mut rx) = mpsc::channel(100);
    
    let options = payload.options.unwrap_or_default();
    let messages = payload.messages;

    let provider_label = provider.name();
    let timer = state.streaming_duration.with_label_values(&[provider_label]).start_timer();
    state.active_connections.inc();
    state.requests_total.with_label_values(&[provider_label, "success"]).inc();

    let permit = match state.llm_concurrency_limiter.clone().acquire_owned().await {
        Ok(p) => p,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to acquire concurrency permit"}))).into_response();
        }
    };

    tokio::spawn(async move {
        let _permit = permit;
        if let Err(e) = provider.chat_stream(&messages, &options, tx.clone()).await {
            error!("Stream error: {}", e);
            let _ = tx.send(Err(e)).await;
        }
    });

    let state_clone = state.clone();
    let stream = async_stream::stream! {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(text) => yield Ok::<Event, Infallible>(Event::default().data(text)),
                Err(e) => yield Ok::<Event, Infallible>(Event::default().event("error").data(e.to_string())),
            }
        }
        timer.observe_duration();
        state_clone.active_connections.dec();
    };

    Sse::new(stream).into_response()
}
