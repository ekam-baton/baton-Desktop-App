use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use axum::http::HeaderMap;
use ed25519_dalek::{SigningKey, VerifyingKey};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
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
    /// Device-generated Ed25519 public key in hex
    pub public_key: String,
    /// Human-readable device name (optional)
    pub device_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PairResponse {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

// ─── Pairing logic ────────────────────────────────────────────────────────────

/// Register a new device: store its public key, generate client_id + JWT pair.
pub async fn pair_device(
    req: PairRequest,
    state: &Arc<AppState>,
) -> anyhow::Result<PairResponse> {
    // Validate that the supplied public key is a valid Ed25519 key
    let pk_bytes = hex::decode(&req.public_key)
        .map_err(|_| anyhow!("Invalid public key hex encoding"))?;
    if pk_bytes.len() != 32 {
        return Err(anyhow!("Public key must be 32 bytes (Ed25519)"));
    }
    VerifyingKey::from_bytes(pk_bytes.as_slice().try_into().unwrap())
        .map_err(|_| anyhow!("Invalid Ed25519 public key"))?;

    // Derive a deterministic client_id from the public key (first 16 bytes as hex)
    let client_id = hex::encode(&pk_bytes[..16]);

    let device_name = req.device_name.unwrap_or_else(|| "Baton Device".into());

    // Persist to DB
    sqlx::query(
        r#"
        INSERT INTO paired_devices (client_id, public_key, device_name, created_at)
        VALUES ($1, $2, $3, CURRENT_TIMESTAMP)
        ON CONFLICT (client_id) DO UPDATE SET
            public_key = $2,
            device_name = $3
        "#
    )
    .bind(&client_id)
    .bind(&req.public_key)
    .bind(&device_name)
    .execute(&state.db)
    .await
    .map_err(|e| anyhow!("DB error: {}", e))?;

    // Cache in memory
    state
        .paired_devices
        .insert(client_id.clone(), req.public_key.clone());

    tracing::info!(
        "Paired new device: client_id={} name={}",
        client_id,
        device_name
    );

    // Generate tokens
    let (access_token, refresh_token) =
        generate_token_pair(&client_id, &state.config.jwt_secret, state.config.jwt_expiry_hours)?;

    Ok(PairResponse {
        client_id,
        access_token,
        refresh_token,
        expires_in: state.config.jwt_expiry_hours * 3600,
    })
}

/// Generate (access_token, refresh_token) pair.
fn generate_token_pair(
    client_id: &str,
    secret: &str,
    expiry_hours: u64,
) -> anyhow::Result<(String, String)> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
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

    let key = EncodingKey::from_secret(secret.as_bytes());
    let access = encode(&Header::default(), &access_claims, &key)?;
    let refresh = encode(&Header::default(), &refresh_claims, &key)?;

    Ok((access, refresh))
}

// ─── Token validation ─────────────────────────────────────────────────────────

pub fn validate_access_token(
    headers: &HeaderMap,
    secret: &str,
) -> Result<Claims, AuthError> {
    let token = extract_bearer(headers).ok_or(AuthError::Missing)?;

    let mut validation = Validation::default();
    validation.validate_exp = true;

    let data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
        _ => AuthError::Invalid(e.to_string()),
    })?;

    if data.claims.kind != "access" {
        return Err(AuthError::Invalid("Not an access token".into()));
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
        let (status, message) = match &self {
            AuthError::Missing => (axum::http::StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::Expired => (axum::http::StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::Invalid(_) => (axum::http::StatusCode::UNAUTHORIZED, self.to_string()),
        };
        (status, message).into_response()
    }
}
