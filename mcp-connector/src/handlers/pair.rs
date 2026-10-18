use std::sync::Arc;
use axum::{extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use serde_json::json;

use crate::auth::{pair_device, refresh_device_token, PairRequest, RefreshRequest};
use crate::state::AppState;

pub async fn pair_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PairRequest>,
) -> impl IntoResponse {
    match pair_device(payload, &state).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => {
            // Log the full detail internally; never expose internals to caller.
            tracing::error!("Pairing failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Pairing failed" })),
            )
            .into_response()
        }
    }
}

pub async fn refresh_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    body: Option<Json<RefreshRequest>>,
) -> impl IntoResponse {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| body.and_then(|b| b.0.refresh_token));

    let refresh_token = match token {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Missing refresh_token" })),
            )
                .into_response();
        }
    };

    match refresh_device_token(&refresh_token, &state).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => e.into_response(),
    }
}

