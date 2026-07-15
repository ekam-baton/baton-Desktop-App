use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use axum::http::HeaderMap;
use ed25519_dalek::VerifyingKey;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ─── JWT Claims ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject = device client_id
    pub sub: String,
    /// Issued At (Unix timestamp)
    pub iat: usize,
    /// Expiry (Unix timestamp)
    pub exp: usize,
    /// Token type: "access" or "refresh"
    pub kind: String,
}

// ─── Pairing request / response ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PairRequest {
    /// Device-generated Ed25519 public key in hex (exactly 64 hex chars = 32 bytes)
    pub public_key: String,
    /// Human-readable device name (optional, max 128 chars enforced server-side)
    pub device_name: Option<String>,
    /// Device's X25519 public key in hex (exactly 64 hex chars = 32 bytes)
    pub x25519_public_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PairResponse {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    // Identity & Ownership
    pub agent_owner_id: String,
    pub agent_owner_name: String,
    pub x25519_public_key: String,
}

// ─── Pairing logic ────────────────────────────────────────────────────────────

/// Register a new device: store its public key, generate client_id + JWT pair.
pub async fn pair_device(
    req: PairRequest,
    state: &Arc<AppState>,
) -> anyhow::Result<PairResponse> {
    // Validate public key: must be exactly 64 hex chars (32 bytes)
    if req.public_key.len() != 64 || !req.public_key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!("Invalid public key format"));
    }
    let pk_bytes = hex::decode(&req.public_key)
        .map_err(|_| anyhow!("Invalid public key hex encoding"))?;
    let arr: &[u8; 32] = pk_bytes.as_slice().try_into()
        .map_err(|_| anyhow!("Invalid public key length"))?;
    VerifyingKey::from_bytes(arr)
        .map_err(|_| anyhow!("Invalid Ed25519 public key"))?;

    // Derive a collision-resistant client_id: hex encoding of the full 32-byte key.
    // Never truncate — truncation creates collision attack surface.
    let client_id = hex::encode(&pk_bytes);

    // Cap device_name to prevent oversized DB writes and XSS amplification.
    let raw_name = req.device_name.unwrap_or_else(|| "Baton Device".into());
    let device_name: String = raw_name.chars().take(128).collect();

    let x25519_pub = req.x25519_public_key.unwrap_or_else(|| "".to_string());

    // Persist to DB
    sqlx::query(
        r#"
        INSERT INTO paired_devices (client_id, public_key, device_name, created_at, x25519_public_key)
        VALUES ($1, $2, $3, CURRENT_TIMESTAMP, $4)
        ON CONFLICT (client_id) DO UPDATE SET
            public_key = $2,
            device_name = $3,
            x25519_public_key = $4
        "#
    )
    .bind(&client_id)
    .bind(&req.public_key)
    .bind(&device_name)
    .bind(&x25519_pub)
    .execute(&state.db)
    .await
    .map_err(|e| anyhow!("DB error during pairing: {}", e))?;

    // Cache in memory
    state
        .paired_devices
        .insert(client_id.clone(), req.public_key.clone());

    // NOTE: Never log the public_key, access_token, or refresh_token.
    tracing::info!(
        "Paired new device: client_id={} name={}",
        client_id,
        device_name
    );

    // Generate tokens — access and refresh use DIFFERENT secrets (MED-03).
    let (access_token, refresh_token) = generate_token_pair(
        &client_id,
        &state.config.jwt_secret,
        &state.config.jwt_refresh_secret,
        state.config.jwt_expiry_hours,
    )?;

    let expires_in_sec = state.config.jwt_expiry_hours * 3600;

    Ok(PairResponse {
        client_id,
        access_token,
        refresh_token,
        expires_in: expires_in_sec,
        agent_owner_id: state.config.agent_owner_id.clone(),
        agent_owner_name: state.config.agent_owner_name.clone(),
        x25519_public_key: hex::encode(&state.config.x25519_public_key),
    })
}

/// Generate (access_token, refresh_token) using SEPARATE signing keys (MED-03).
/// This prevents refresh tokens from being used as access tokens and vice-versa
/// even if the `kind` field check were accidentally omitted.
fn generate_token_pair(
    client_id: &str,
    access_secret: &str,
    refresh_secret: &str,
    expiry_hours: u64,
) -> anyhow::Result<(String, String)> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow!("System clock error: {}", e))?
        .as_secs() as usize;

    let access_claims = Claims {
        sub: client_id.to_owned(),
        iat: now,
        exp: now + (expiry_hours * 3600) as usize,
        kind: "access".into(),
    };

    let refresh_claims = Claims {
        sub: client_id.to_owned(),
        iat: now,
        exp: now + (30 * 24 * 3600), // 30 days
        kind: "refresh".into(),
    };

    // Access token signed with access_secret.
    let access_key = EncodingKey::from_secret(access_secret.as_bytes());
    let access = encode(&Header::default(), &access_claims, &access_key)?;

    // Refresh token signed with a DIFFERENT secret — cryptographically separated.
    let refresh_key = EncodingKey::from_secret(refresh_secret.as_bytes());
    let refresh = encode(&Header::default(), &refresh_claims, &refresh_key)?;

    Ok((access, refresh))
}

// ─── Token validation ─────────────────────────────────────────────────────────

pub fn validate_access_token(
    headers: &HeaderMap,
    secret: &str,
) -> Result<Claims, AuthError> {
    let token = extract_bearer(headers).ok_or(AuthError::Missing)?;

    // MED-05: Reject tokens with absurdly large headers (DoS guard).
    if token.len() > 4096 {
        return Err(AuthError::Invalid("Token too large".into()));
    }

    let mut validation = Validation::default();
    validation.validate_exp = true;
    // Only accept HS256. Never allow 'none' algorithm.
    validation.algorithms = vec![jsonwebtoken::Algorithm::HS256];

    let data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
        // Do NOT leak internal error details to callers.
        _ => AuthError::Invalid("Invalid token".into()),
    })?;

    if data.claims.kind != "access" {
        return Err(AuthError::Invalid("Not an access token".into()));
    }

    Ok(data.claims)
}

/// Validate a refresh token using the dedicated refresh secret.
pub fn validate_refresh_token(
    token: &str,
    refresh_secret: &str,
) -> Result<Claims, AuthError> {
    if token.len() > 4096 {
        return Err(AuthError::Invalid("Token too large".into()));
    }

    let mut validation = Validation::default();
    validation.validate_exp = true;
    validation.algorithms = vec![jsonwebtoken::Algorithm::HS256];

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(refresh_secret.as_bytes()),
        &validation,
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
        _ => AuthError::Invalid("Invalid token".into()),
    })?;

    if data.claims.kind != "refresh" {
        return Err(AuthError::Invalid("Not a refresh token".into()));
    }

    Ok(data.claims)
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_owned())
}

// ─── Error type ───────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Missing Authorization header")]
    Missing,
    #[error("Token has expired")]
    Expired,
    #[error("Invalid token: {0}")]
    Invalid(String),
}

impl axum::response::IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        // Always return 401 with a generic message — never leak which check failed.
        (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::http::HeaderMap::new(),
            "Unauthorized",
        )
            .into_response()
    }
}
