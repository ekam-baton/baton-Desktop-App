use std::sync::Arc;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::auth::{pair_device, PairRequest};
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
