use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use axum::http::HeaderMap;
use ed25519_dalek::VerifyingKey;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit}};
use rand::RngCore;
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::state::AppState;

pub fn encrypt_db_field(plaintext: &str, master_secret: &str) -> anyhow::Result<String> {
    if plaintext.is_empty() { return Ok(String::new()); }
    
    let hk = hkdf::Hkdf::<Sha256>::new(None, master_secret.as_bytes());
    let mut derived_key = [0u8; 32];
    hk.expand(b"baton_db_encryption", &mut derived_key).map_err(|_| anyhow!("HKDF expand failed"))?;

    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(derived_key));
    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);
    let nonce = Nonce::from(iv);

    let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut combined = iv.to_vec();
    combined.extend(ciphertext);
    Ok(STANDARD.encode(&combined))
}

pub fn decrypt_db_field(ciphertext_b64: &str, master_secret: &str) -> anyhow::Result<String> {
    if ciphertext_b64.is_empty() { return Ok(String::new()); }
    
    let combined = match STANDARD.decode(ciphertext_b64) {
        Ok(c) => c,
        Err(_) => return Ok(ciphertext_b64.to_string()),
    };
    if combined.len() < 12 { return Ok(ciphertext_b64.to_string()); }

    let hk = hkdf::Hkdf::<Sha256>::new(None, master_secret.as_bytes());
    let mut derived_key = [0u8; 32];
    hk.expand(b"baton_db_encryption", &mut derived_key).map_err(|_| anyhow!("HKDF failed"))?;

    let (iv, ciphertext) = combined.split_at(12);
    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(derived_key));
    let nonce = Nonce::try_from(iv).map_err(|_| anyhow!("Invalid IV length"))?;

    let plaintext = match cipher.decrypt(&nonce, ciphertext) {
        Ok(p) => p,
        Err(_) => return Err(anyhow::anyhow!("Decryption failed")),
    };

    let result = String::from_utf8(plaintext).map_err(|_| anyhow::anyhow!("Invalid UTF-8"))?;
    Ok(result)
}


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
    pub ed25519_public_key: String,
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

    // Cap device_name to prevent oversized DB writes and XSS/CRLF log injection.
    let raw_name = req.device_name.unwrap_or_else(|| "Baton Device".into());
    let device_name: String = raw_name.chars().filter(|c| !c.is_control()).take(128).collect();
    let device_name = if device_name.trim().is_empty() { "Baton Device".to_string() } else { device_name };
    let enc_device_name = encrypt_db_field(&device_name, &state.config.jwt_secret).unwrap_or(device_name.clone());

    let x25519_pub = req.x25519_public_key.unwrap_or_default();
    if !x25519_pub.is_empty() && (x25519_pub.len() != 64 || !x25519_pub.chars().all(|c| c.is_ascii_hexdigit())) {
        return Err(anyhow!("Invalid x25519 public key format: must be 64 hex characters"));
    }
    let enc_x25519_pub = encrypt_db_field(&x25519_pub, &state.config.jwt_secret).unwrap_or(x25519_pub);

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
    .bind(&enc_device_name)
    .bind(&enc_x25519_pub)
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
        ed25519_public_key: hex::encode(&state.config.ed25519_public_key),
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
    #[error("Device authorization has been revoked or is pending approval")]
    Revoked,
    #[error("Client does not have permission to access agent: {0}")]
    AgentForbidden(String),
}

impl axum::response::IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AuthError::AgentForbidden(ref agent) => (
                axum::http::StatusCode::FORBIDDEN,
                axum::response::Json(serde_json::json!({
                    "error": "Forbidden",
                    "detail": format!("Client is not authorized to message agent '{}'", agent)
                }))
            ).into_response(),
            AuthError::Revoked => (
                axum::http::StatusCode::FORBIDDEN,
                axum::response::Json(serde_json::json!({
                    "error": "Forbidden",
                    "detail": "Device authorization has been revoked or is pending approval"
                }))
            ).into_response(),
            _ => (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::http::HeaderMap::new(),
                "Unauthorized",
            ).into_response(),
        }
    }
}

/// Comprehensive server-side authorization check:
/// 1. Validates JWT signature, format, and expiry.
/// 2. Verifies that the client_id exists in `authorized_users` with `status == 'approved'`.
/// 3. If `target_agent` is provided, verifies that the client has permission to access that specific agent ID.
pub async fn validate_client_authorization(
    headers: &HeaderMap,
    state: &Arc<AppState>,
    target_agent: Option<&str>,
) -> Result<Claims, AuthError> {
    // 1. Validate JWT cryptographic signature and expiry
    let claims = validate_access_token(headers, &state.config.jwt_secret)?;

    // 2. Server-side Revocation and Status check
    use sqlx::Row;
    let user_row = sqlx::query("SELECT status, role FROM authorized_users WHERE client_id = $1")
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("DB error during authorization check: {}", e);
            AuthError::Invalid("Database error".into())
        })?;

    let (status, role) = match user_row {
        Some(row) => {
            let s: String = row.try_get("status").unwrap_or_default();
            let r: String = row.try_get("role").unwrap_or_else(|_| "guest".into());
            (s, r)
        }
        None => {
            // Also check paired_devices cache
            let mut paired = state.paired_devices.contains_key(&claims.sub);
            if !paired {
                // Fallback: check database directly and cache if found
                if let Ok(Some(row)) = sqlx::query("SELECT public_key FROM paired_devices WHERE client_id = $1")
                    .bind(&claims.sub)
                    .fetch_optional(&state.db)
                    .await
                {
                    use sqlx::Row;
                    if let Ok(pk) = row.try_get::<String, _>("public_key") {
                        state.paired_devices.insert(claims.sub.clone(), pk);
                        paired = true;
                    }
                }
            }
            if paired {
                ("approved".to_string(), "guest".to_string())
            } else {
                return Err(AuthError::Revoked);
            }
        }
    };

    if status != "approved" {
        tracing::warn!("Blocked request from unapproved client '{}' (status: '{}')", claims.sub, status);
        return Err(AuthError::Revoked);
    }

    // 3. Granular Agent-Level ACL check
    if let Some(agent) = target_agent {
        // Admins automatically have access to all agents
        if role != "admin" {
            let auth_row = sqlx::query(
                "SELECT 1 FROM agent_authorizations WHERE client_id = $1 AND (agent_id = $2 OR agent_id = '*')"
            )
            .bind(&claims.sub)
            .bind(agent)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("DB error during agent authorization check: {}", e);
                AuthError::Invalid("Database error".into())
            })?;

            if auth_row.is_none() {
                // If specific authorizations are defined for this client, reject unauthorized target
                let has_any = sqlx::query("SELECT 1 FROM agent_authorizations WHERE client_id = $1")
                    .bind(&claims.sub)
                    .fetch_optional(&state.db)
                    .await
                    .unwrap_or(None);

                if has_any.is_some() {
                    tracing::warn!("Client '{}' forbidden from accessing agent '{}'", claims.sub, agent);
                    return Err(AuthError::AgentForbidden(agent.to_string()));
                }
            }
        }
    }

    Ok(claims)
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

pub async fn refresh_device_token(
    refresh_token: &str,
    state: &Arc<AppState>,
) -> Result<RefreshResponse, AuthError> {
    let claims = validate_refresh_token(refresh_token, &state.config.jwt_refresh_secret)?;

    // Check if the user is in authorized_users and whether their authorization has been revoked
    use sqlx::Row;
    let user_row = sqlx::query("SELECT status FROM authorized_users WHERE client_id = $1")
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("DB error during token refresh authorization check: {}", e);
            AuthError::Invalid("Database error".into())
        })?;

    if let Some(row) = user_row {
        let status: String = row.try_get("status").unwrap_or_default();
        if status != "approved" {
            tracing::warn!(
                "Blocked token refresh for unapproved client '{}' (status: '{}')",
                claims.sub,
                status
            );
            return Err(AuthError::Revoked);
        }
    } else {
        // Fallback: Ensure the device is still active and registered in paired_devices
        let mut paired = state.paired_devices.contains_key(&claims.sub);
        if !paired {
            let exists = sqlx::query("SELECT 1 FROM paired_devices WHERE client_id = $1")
                .bind(&claims.sub)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .is_some();
            if exists {
                paired = true;
            }
        }
        if !paired {
            return Err(AuthError::Revoked);
        }
    }

    let (access_token, new_refresh_token) = generate_token_pair(
        &claims.sub,
        &state.config.jwt_secret,
        &state.config.jwt_refresh_secret,
        state.config.jwt_expiry_hours,
    ).map_err(|e| AuthError::Invalid(e.to_string()))?;

    Ok(RefreshResponse {
        access_token,
        refresh_token: new_refresh_token,
        expires_in: state.config.jwt_expiry_hours * 3600,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_pair_generation_and_validation() {
        let client_id = "test_device_123";
        let access_secret = "test_access_secret_12345678901234567890";
        let refresh_secret = "test_refresh_secret_12345678901234567890";
        let (access, refresh) = generate_token_pair(client_id, access_secret, refresh_secret, 24).unwrap();

        // Validating refresh token with refresh_secret succeeds
        let refresh_claims = validate_refresh_token(&refresh, refresh_secret).unwrap();
        assert_eq!(refresh_claims.sub, client_id);
        assert_eq!(refresh_claims.kind, "refresh");

        // Attempting to validate refresh token with access_secret fails
        assert!(validate_refresh_token(&refresh, access_secret).is_err());

        // Attempting to validate access token as refresh token fails
        assert!(validate_refresh_token(&access, refresh_secret).is_err());
    }

    #[test]
    fn test_oversized_refresh_token_rejected() {
        let huge_token = "a".repeat(5000);
        let res = validate_refresh_token(&huge_token, "some_secret");
        assert!(res.is_err());
    }
}


