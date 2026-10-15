use std::sync::Arc;
use axum::{extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use serde_json::json;

use crate::auth::validate_client_authorization;
use crate::state::AppState;

pub async fn tools_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(e) = validate_client_authorization(&headers, &state, None).await {
        return e.into_response();
    }

    let tools = json!([
        {
            "name": "chat",
            "description": "Send a chat message to the agent or relay",
            "parameters": {
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            }
        },
        {
            "name": "read_file",
            "description": "Read local file context",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }
        }
    ]);

    (StatusCode::OK, Json(json!({ "tools": tools }))).into_response()
}
