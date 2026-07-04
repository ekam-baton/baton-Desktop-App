use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;

use crate::state::AppState;

pub async fn health_handler() -> impl IntoResponse {
    Json(json!({ "status": "ok", "service": "mcp-connector" }))
}

pub async fn ready_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Check DB connection
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    
    if db_ok {
        Json(json!({ "status": "ready", "db": "connected" }))
    } else {
        Json(json!({ "status": "error", "db": "disconnected" }))
    }
}
