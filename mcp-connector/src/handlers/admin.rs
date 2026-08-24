use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json},
};
use serde::Serialize;
use std::sync::Arc;
use crate::state::AppState;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use axum::response::sse::{Event, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio::sync::mpsc;
use crate::providers::{get_provider, ChatMessage, ChatOptions};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ChatRequest {
    pub provider: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub options: Option<ChatOptions>,
}

#[derive(Serialize)]
pub struct PendingRequest {
    pub client_id: String,
    pub device_name: Option<String>,
}

pub fn check_auth(headers: &HeaderMap, password_hash: &str) -> Result<(), StatusCode> {
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

    use argon2::{password_hash::{PasswordHash, PasswordVerifier}, Argon2};
    let parsed_hash = match PasswordHash::new(password_hash) {
        Ok(h) => h,
        Err(_) => return Err(StatusCode::UNAUTHORIZED),
    };
    let password_ok = Argon2::default().verify_password(submitted_password.as_bytes(), &parsed_hash).is_ok();

    // Constant-time check for username
    use sha2::{Sha256, Digest};
    let expected_username = Sha256::digest(b"admin");
    let provided_username = Sha256::digest(username.as_bytes());
    let username_ok = expected_username.iter().zip(provided_username.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;

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

    let pending: Vec<PendingRequest> = rows.iter().map(|row| {
        let enc_name: Option<String> = row.get("device_name");
        let dec_name = enc_name.map(|n| crate::auth::decrypt_db_field(&n, &state.config.jwt_secret).unwrap_or(n));
        PendingRequest {
            client_id: row.get("client_id"),
            device_name: dec_name,
        }
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

pub async fn authorized_handler(headers: axum::http::HeaderMap, axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>) -> Result<axum::response::Json<Vec<PendingRequest>>, axum::http::StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let rows = sqlx::query("SELECT client_id, device_name FROM authorized_users WHERE status = 'approved' ORDER BY created_at DESC LIMIT 200").fetch_all(&state.db).await.map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    let authorized = rows.iter().map(|row| {
        let enc_name: Option<String> = row.get("device_name");
        let dec_name = enc_name.map(|n| crate::auth::decrypt_db_field(&n, &state.config.jwt_secret).unwrap_or(n));
        PendingRequest { client_id: row.get("client_id"), device_name: dec_name }
    }).collect();
    Ok(axum::response::Json(authorized))
}

pub async fn revoke_handler(headers: axum::http::HeaderMap, axum::extract::Path(client_id): axum::extract::Path<String>, axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>) -> Result<axum::http::StatusCode, axum::http::StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    if !client_id.chars().all(|c| c.is_ascii_hexdigit()) || client_id.len() > 64 { return Err(axum::http::StatusCode::BAD_REQUEST); }
    sqlx::query("UPDATE authorized_users SET status = 'revoked' WHERE client_id = $1").bind(&client_id).execute(&state.db).await.map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::http::StatusCode::OK)
}

pub async fn admin_chat_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.admin_password) {
        return (
            e,
            [(axum::http::header::WWW_AUTHENTICATE, "Basic realm=\"Baton Admin\"")],
            Html("Unauthorized".to_string()),
        ).into_response();
    }

    let provider_name = payload.provider.unwrap_or_else(|| state.config.default_provider.clone());
    let provider = match get_provider(&provider_name, &state.config, &state.http) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string()}))).into_response();
        }
    };

    tracing::info!("Admin streaming chat via provider: {}", provider.name());
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
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to acquire concurrency permit"}))).into_response();
        }
    };

    tokio::spawn(async move {
        let _permit = permit;
        if let Err(e) = provider.chat_stream(&messages, &options, tx.clone()).await {
            tracing::error!("Stream error: {}", e);
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

pub async fn safety_number_handler(
    Path(device_id): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    // Validate the format (hex encoded Ed25519 public key)
    if device_id.len() != 64 || !device_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    let device_pub_bytes = hex::decode(&device_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let our_pub_bytes = state.config.ed25519_public_key;

    // Sort the keys lexicographically
    let (first, second) = if device_pub_bytes.as_slice() < our_pub_bytes.as_slice() {
        (device_pub_bytes, our_pub_bytes.to_vec())
    } else {
        (our_pub_bytes.to_vec(), device_pub_bytes)
    };

    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(&first);
    combined.extend_from_slice(&second);

    // Compute SHA-512
    use sha2::{Sha512, Digest};
    let mut hasher = Sha512::new();
    hasher.update(&combined);
    let hash = hasher.finalize();

    // The safety number is 60 digits derived from the hash (12 groups of 5)
    let mut safety_number = String::with_capacity(72);
    for i in 0..12 {
        let offset = i * 5;
        if offset + 4 >= hash.len() { break; }
        
        let mut buf = [0u8; 8];
        buf[3..8].copy_from_slice(&hash[offset..offset+5]);
        let val = u64::from_be_bytes(buf);
        let digits = val % 100000;
        
        if i > 0 {
            safety_number.push(' ');
        }
        safety_number.push_str(&format!("{:05}", digits));
    }

    Ok(Json(serde_json::json!({
        "device_id": device_id,
        "safety_number": safety_number
    })))
}

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub member_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub creator_id: String,
    pub member_ids: Vec<String>,
    pub created_at: i64,
}

pub async fn create_group_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateGroupRequest>,
) -> Result<Json<Group>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    let group_id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp();
    let member_ids_json = serde_json::to_string(&payload.member_ids).unwrap_or_default();
    let creator_id = hex::encode(state.config.ed25519_public_key);
    
    // insert into db
    sqlx::query("INSERT INTO groups (id, name, creator_id, member_ids, created_at) VALUES ($1, $2, $3, $4, $5)")
        .bind(&group_id)
        .bind(&payload.name)
        .bind(&creator_id)
        .bind(&member_ids_json)
        .bind(created_at)
        .execute(&state.db).await
        .map_err(|e| {
            tracing::error!("DB error in create_group_handler: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Call Cloud Router POST http://<relay_url>/group/create
    let http_url = state.config.a2a_router_url.replace("wss://", "https://").replace("ws://", "http://");
    let create_url = format!("{}/group/create", http_url);

    let mut members_with_creator = payload.member_ids.clone();
    if !members_with_creator.contains(&creator_id) {
        members_with_creator.push(creator_id.clone());
    }

    let req_body = serde_json::json!({
        "group_id": group_id,
        "name": payload.name,
        "members": members_with_creator,
    });

    let res = state.http.post(&create_url)
        .json(&req_body)
        .send().await;

    if let Err(e) = res {
        tracing::error!("Failed to create group on cloud router: {}", e);
    }

    Ok(Json(Group {
        id: group_id,
        name: payload.name,
        creator_id,
        member_ids: payload.member_ids,
        created_at,
    }))
}

pub async fn list_groups_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Group>>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    let rows = sqlx::query("SELECT id, name, creator_id, member_ids, created_at FROM groups")
        .fetch_all(&state.db).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    use sqlx::Row;
    let mut groups = Vec::new();
    for row in rows {
        let member_ids_str: String = row.get("member_ids");
        let member_ids: Vec<String> = serde_json::from_str(&member_ids_str).unwrap_or_default();
        groups.push(Group {
            id: row.get("id"),
            name: row.get("name"),
            creator_id: row.get("creator_id"),
            member_ids,
            created_at: row.get("created_at"),
        });
    }

    Ok(Json(groups))
}

pub async fn delete_group_handler(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    sqlx::query("DELETE FROM groups WHERE id = $1")
        .bind(&id)
        .execute(&state.db).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let http_url = state.config.a2a_router_url.replace("wss://", "https://").replace("ws://", "http://");
    let delete_url = format!("{}/group/{}", http_url, id);

    let res = state.http.delete(&delete_url)
        .send().await;

    if let Err(e) = res {
        tracing::error!("Failed to delete group on cloud router: {}", e);
    }

    Ok(StatusCode::OK)
}

// ── Vault Backup Endpoints (Pillar 6) ───────────────────────────────────────

#[derive(Deserialize)]
pub struct VaultRequest {
    pub client_id: String,
    pub vault_data: String,
}

#[derive(Serialize)]
pub struct VaultResponse {
    pub client_id: String,
    pub vault_data: String,
    pub updated_at: i64,
}

pub async fn upload_vault_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<VaultRequest>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    sqlx::query(
        "INSERT INTO vaults (client_id, vault_data, updated_at) VALUES ($1, $2, $3)
         ON CONFLICT(client_id) DO UPDATE SET vault_data = $2, updated_at = $3"
    )
    .bind(&payload.client_id)
    .bind(&payload.vault_data)
    .bind(now)
    .execute(&state.db).await
    .map_err(|e| {
        tracing::error!("DB error in upload_vault_handler: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}

pub async fn download_vault_handler(
    headers: HeaderMap,
    Path(client_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<VaultResponse>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    use sqlx::Row;
    let row = sqlx::query("SELECT vault_data, updated_at FROM vaults WHERE client_id = $1")
        .bind(&client_id)
        .fetch_optional(&state.db).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(row) = row {
        Ok(Json(VaultResponse {
            client_id,
            vault_data: row.get("vault_data"),
            updated_at: row.get("updated_at"),
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
