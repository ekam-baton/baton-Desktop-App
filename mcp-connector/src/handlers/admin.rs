use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json},
};
use serde::Serialize;
use std::sync::Arc;
use crate::state::AppState;
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[derive(Serialize)]
pub struct PendingRequest {
    pub client_id: String,
    pub device_name: Option<String>,
}

/// Constant-time byte comparison to prevent timing-oracle attacks on the password.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // XOR every byte; only zero if all are equal. Never short-circuits.
    let diff = a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    diff == 0
}

fn check_auth(headers: &HeaderMap, password: &str) -> Result<(), StatusCode> {
    let auth_header = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let base64_creds = auth_header
        .strip_prefix("Basic ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let decoded = STANDARD
        .decode(base64_creds)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let creds = String::from_utf8(decoded)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let colon = creds.find(':').ok_or(StatusCode::UNAUTHORIZED)?;
    let (username, submitted_password) = creds.split_at(colon);
    let submitted_password = &submitted_password[1..]; // skip the ':'

    // Only accept the username "admin" and the configured password, using constant-time comparison.
    let username_ok = constant_time_eq(username.as_bytes(), b"admin");
    let password_ok = constant_time_eq(submitted_password.as_bytes(), password.as_bytes());

    if username_ok && password_ok {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn dashboard_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.admin_password) {
        return (
            e,
            [(axum::http::header::WWW_AUTHENTICATE, "Basic realm=\"Baton Admin\"")],
            Html("Unauthorized".to_string()),
        ).into_response();
    }

    let html = include_str!("../dashboard.html");
    Html(html).into_response()
}

pub async fn pending_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PendingRequest>>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT client_id, device_name FROM authorized_users WHERE status = 'pending' ORDER BY created_at DESC LIMIT 200"
    )
    .fetch_all(&state.db).await
    .map_err(|e| {
        tracing::error!("DB error in pending_handler: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let pending: Vec<PendingRequest> = rows.iter().map(|row| PendingRequest {
        client_id: row.get("client_id"),
        device_name: row.get("device_name"),
    }).collect();

    Ok(Json(pending))
}

pub async fn approve_handler(
    headers: HeaderMap,
    Path(client_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    // Validate client_id format to prevent injection or path traversal.
    if !client_id.chars().all(|c| c.is_ascii_hexdigit()) || client_id.len() > 64 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let rows_affected = sqlx::query(
        "UPDATE authorized_users SET status = 'approved', role = 'guest' WHERE client_id = $1 AND status = 'pending'"
    )
    .bind(&client_id)
    .execute(&state.db).await
    .map_err(|e| {
        tracing::error!("DB error in approve_handler: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .rows_affected();

    if rows_affected == 0 {
        // Either doesn't exist or already approved/denied — not an error.
        tracing::warn!("approve_handler: no pending row found for client_id={}", client_id);
    }

    Ok(StatusCode::OK)
}

pub async fn deny_handler(
    headers: HeaderMap,
    Path(client_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    // Validate client_id format.
    if !client_id.chars().all(|c| c.is_ascii_hexdigit()) || client_id.len() > 64 {
        return Err(StatusCode::BAD_REQUEST);
    }

    sqlx::query(
        "UPDATE authorized_users SET status = 'denied' WHERE client_id = $1"
    )
    .bind(&client_id)
    .execute(&state.db).await
    .map_err(|e| {
        tracing::error!("DB error in deny_handler: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}
