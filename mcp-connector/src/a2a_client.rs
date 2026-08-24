use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{info, error, warn};
use tokio::sync::mpsc;

use crate::state::AppState;

/// Maximum size of a single incoming A2A WebSocket message (1 MB).
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Maximum concurrent in-flight A2A requests being processed at one time.
const MAX_CONCURRENT_REQUESTS: usize = 50;

/// Reconnection back-off: starts at 2s, grows to 60s max.
const MIN_BACKOFF_SECS: u64 = 2;
const MAX_BACKOFF_SECS: u64 = 60;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct A2aEnvelope {
    pub sender_id: String,
    pub receiver_id: String,
    pub group_id: Option<String>,
    /// Base64-encoded payload.
    /// NOTE: Real E2EE (X25519 + AES-256-GCM) must be layered on top before launch.
    pub payload_encrypted: String,
}

pub async fn start_a2a_client(state: Arc<AppState>, connector_id: String) {
    let url_str = format!("{}/ws/{}", state.config.a2a_router_url, connector_id);
    let http_url_str = state.config.a2a_router_url.replace("ws://", "http://").replace("wss://", "https://") + "/login";
    let mut backoff_secs = MIN_BACKOFF_SECS;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let reqwest_client = reqwest::Client::new();

    loop {
        info!("Authenticating with A2A router at {}", http_url_str);
        let login_payload = serde_json::json!({
            "client_id": connector_id,
            // Use the dedicated A2A secret, never the JWT signing key
            "secret": state.config.a2a_router_secret
        });

        let token = match reqwest_client.post(&http_url_str).json(&login_payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        if let Some(t) = json.get("token").and_then(|v| v.as_str()) {
                            t.to_string()
                        } else {
                            error!("A2A login response missing 'token'");
                            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                            backoff_secs = std::cmp::min(backoff_secs * 2, MAX_BACKOFF_SECS);
                            continue;
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse A2A login response: {}", e);
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = std::cmp::min(backoff_secs * 2, MAX_BACKOFF_SECS);
                        continue;
                    }
                }
            }
            Ok(resp) => {
                error!("A2A router login failed: HTTP {}", resp.status());
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = std::cmp::min(backoff_secs * 2, MAX_BACKOFF_SECS);
                continue;
            }
            Err(e) => {
                error!("Failed to reach A2A router for login: {}", e);
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = std::cmp::min(backoff_secs * 2, MAX_BACKOFF_SECS);
                continue;
            }
        };

        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = match url_str.as_str().into_client_request() {
            Ok(req) => req,
            Err(e) => {
                error!("Invalid WS URL: {}", e);
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                continue;
            }
        };
        
        if let Ok(header_val) = axum::http::HeaderValue::from_str(&format!("Bearer {}", token)) {
            request.headers_mut().insert("Authorization", header_val);
        }

        info!("Connecting to A2A router at {}", url_str);
        match connect_async(request).await {
            Ok((ws_stream, _)) => {
                info!("Connected to A2A router as {}", connector_id);
                backoff_secs = MIN_BACKOFF_SECS; // reset on successful connect

                let (write_sink, mut read_stream) = ws_stream.split();

                // Use a channel to serialize all outbound writes through a single task.
                let (out_tx, mut out_rx) = mpsc::channel::<Message>(64);

                // Writer task: drains the out_tx channel and sends over the WS sink.
                let mut write_sink = write_sink;
                let writer_task = tokio::spawn(async move {
                    while let Some(msg) = out_rx.recv().await {
                        if let Err(e) = write_sink.send(msg).await {
                            error!("A2A WebSocket write error: {}", e);
                            break;
                        }
                    }
                });

                // Reader loop
                while let Some(msg) = read_stream.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            // Hard size limit before deserialising
                            if text.len() > MAX_MESSAGE_BYTES {
                                warn!("Dropping oversized A2A message ({} bytes)", text.len());
                                continue;
                            }

                            let envelope = match serde_json::from_str::<A2aEnvelope>(&text) {
                                Ok(e) => e,
                                Err(err) => {
                                    warn!("Malformed A2A envelope: {}", err);
                                    continue;
                                }
                            };

                            // Validate sender_id: allow only hex chars (our client_id format)
                            if !envelope.sender_id.chars().all(|c| c.is_ascii_hexdigit()) || envelope.sender_id.len() > 64 {
                                warn!("Dropping A2A envelope with invalid sender_id");
                                continue;
                            }

                            let permit = match semaphore.clone().try_acquire_owned() {
                                Ok(p) => p,
                                Err(_) => {
                                    warn!("A2A request queue full — dropping message from {}", envelope.sender_id);
                                    continue;
                                }
                            };

                            let state_clone = state.clone();
                            let out_tx_clone = out_tx.clone();

                            tokio::spawn(async move {
                                let _permit = permit; // Released when this task ends.
                                process_envelope(state_clone, envelope, out_tx_clone).await;
                            });
                        }
                        Ok(Message::Binary(_)) => {
                            warn!("Received binary A2A message — ignoring (text-only protocol)");
                        }
                        Ok(Message::Ping(data)) => {
                            // Respond to pings to keep the connection alive.
                            let _ = out_tx.send(Message::Pong(data)).await;
                        }
                        Ok(Message::Close(_)) => {
                            warn!("A2A router requested connection close");
                            break;
                        }
                        Err(e) => {
                            error!("A2A WebSocket read error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }

                // Stop the writer task cleanly.
                writer_task.abort();
            }
            Err(e) => {
                error!("Failed to connect to A2A router: {}. Retrying in {}s...", e, backoff_secs);
            }
        }

        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
    }
}

/// Process one incoming A2A envelope end-to-end.
async fn process_envelope(
    state: Arc<AppState>,
    envelope: A2aEnvelope,
    out_tx: mpsc::Sender<Message>,
) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use sqlx::Row;

    if let Some(ref gid) = envelope.group_id {
        info!("Processing group message for group {}", gid);
    }

    // ── 1. ACL check ─────────────────────────────────────────────────────────
    let auth_record = sqlx::query(
        "SELECT a.status, p.x25519_public_key, p.ratchet_state 
         FROM authorized_users a
         JOIN paired_devices p ON a.client_id = p.client_id
         WHERE a.client_id = $1"
    )
    .bind(&envelope.sender_id)
    .fetch_optional(&state.db).await
    .unwrap_or(None);

    let (is_authorized, sender_x25519_hex, ratchet_state_enc) = match auth_record {
        Some(r) => {
            let s: String = r.get("status");
            let pk_enc: Option<String> = r.get("x25519_public_key");
            let rs_enc: Option<String> = r.get("ratchet_state");
            
            let pk_dec = if let Some(enc) = pk_enc {
                crate::auth::decrypt_db_field(&enc, &state.config.jwt_secret).unwrap_or_default()
            } else {
                "".to_string()
            };
            
            (s == "approved", pk_dec, rs_enc)
        }
        None => (false, "".to_string(), None),
    };

    if !is_authorized {
        let _ = sqlx::query(
            "INSERT INTO authorized_users (client_id, public_key, status, role) \
             VALUES ($1, 'unknown', 'pending', 'guest') \
             ON CONFLICT (client_id) DO NOTHING"
        )
        .bind(&envelope.sender_id)
        .execute(&state.db).await;

        warn!("Unauthorized A2A request from '{}' — queued as pending", envelope.sender_id);
        return;
    }

    if sender_x25519_hex.is_empty() {
        error!("No X25519 public key found for '{}' - cannot perform E2EE", envelope.sender_id);
        return;
    }

    // ── 2. Decode & validate payload (Double Ratchet) ────────────────────────
    #[derive(Deserialize, Serialize)]
    struct RatchetPayload {
        header_pub: String, // Base64
        header_n: u32,
        header_pn: u32,
        ciphertext: String, // Base64 (IV + Ciphertext)
    }

    let payload_json = match STANDARD.decode(&envelope.payload_encrypted) {
        Ok(b) => match String::from_utf8(b) {
            Ok(s) => s,
            Err(_) => return,
        },
        Err(_) => return,
    };

    let ratchet_payload = match serde_json::from_str::<RatchetPayload>(&payload_json) {
        Ok(p) => p,
        Err(e) => {
            error!("Malformed ratchet payload from '{}': {}", envelope.sender_id, e);
            return;
        }
    };

    // Initialize or load RatchetState
    let sender_x25519_bytes = match hex::decode(&sender_x25519_hex) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            error!("Invalid X25519 public key hex for '{}'", envelope.sender_id);
            return;
        }
    };
    
    let sender_pub = x25519_dalek::PublicKey::from(sender_x25519_bytes);
    let my_secret_bytes = state.config.x25519_private_key;
    let my_secret = x25519_dalek::StaticSecret::from(my_secret_bytes);
    let shared_secret = my_secret.diffie_hellman(&sender_pub);
    
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(shared_secret.as_bytes());
    let root_key = hasher.finalize().into(); // 32 bytes

    let mut ratchet_state = if let Some(enc) = ratchet_state_enc {
        let dec_json = crate::auth::decrypt_db_field(&enc, &state.config.jwt_secret).unwrap_or_default();
        match serde_json::from_str::<crate::ratchet::RatchetState>(&dec_json) {
            Ok(s) => s,
            Err(_) => {
                warn!("Failed to deserialize RatchetState for '{}', re-initializing as Bob.", envelope.sender_id);
                crate::ratchet::RatchetState::init_bob(root_key, my_secret_bytes)
            }
        }
    } else {
        // First message ever received from this peer, initialize as Bob
        crate::ratchet::RatchetState::init_bob(root_key, my_secret_bytes)
    };

    let hpub_bytes = match STANDARD.decode(&ratchet_payload.header_pub) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            error!("Invalid header_pub from '{}'", envelope.sender_id);
            return;
        }
    };

    let ct_bytes = match STANDARD.decode(&ratchet_payload.ciphertext) {
        Ok(b) => b,
        Err(_) => return,
    };

    // Decrypt using Double Ratchet
    let decrypted_bytes = match ratchet_state.ratchet_decrypt(
        hpub_bytes,
        ratchet_payload.header_n,
        ratchet_payload.header_pn,
        &ct_bytes
    ) {
        Ok(b) => b,
        Err(e) => {
            error!("Ratchet decryption failed for '{}': {}", envelope.sender_id, e);
            return;
        }
    };

    let decrypted_json = match String::from_utf8(decrypted_bytes) {
        Ok(s) => s,
        Err(_) => return,
    };

    if decrypted_json.len() > 256 * 1024 {
        warn!("A2A payload too large from '{}' — dropping", envelope.sender_id);
        return;
    }

    // ── 3. Parse as MCP request ───────────────────────────────────────────────
    let req = match serde_json::from_str::<crate::mcp_models::McpRequest>(&decrypted_json) {
        Ok(r) => r,
        Err(e) => {
            error!("Invalid MCP JSON in A2A payload from '{}': {}", envelope.sender_id, e);
            return;
        }
    };

    // ── 4. Validate that the requested method is allowed ─────────────────────
    let allowed_methods = ["tools/call", "tools/list", "initialize", "notifications/initialized"];
    if !allowed_methods.contains(&req.method.as_str()) {
        warn!("A2A request from '{}' used disallowed method '{}' — dropping", envelope.sender_id, req.method);
        return;
    }

    // ── 5. Route to the correct MCP agent ────────────────────────────────────
    let target = state.mcp_clients.iter().next().map(|e| e.key().clone())
        .unwrap_or_else(|| "openclaw".to_string());

    let client = match state.mcp_clients.get(&target) {
        Some(c) => c.clone(),
        None => {
            error!("No MCP agent available to handle A2A request");
            return;
        }
    };

    // ── 6. Execute with timeout ───────────────────────────────────────────────
    let res = match tokio::time::timeout(
        Duration::from_secs(60),
        client.send_request(&req)
    ).await {
        Ok(Ok(val)) => val,
        Ok(Err(e)) => {
            error!("Agent error for A2A request from '{}': {}", envelope.sender_id, e);
            return;
        }
        Err(_) => {
            error!("Agent timed out for A2A request from '{}'", envelope.sender_id);
            return;
        }
    };

    // ── 7. Encode, Encrypt, and send response ─────────────────────────────────
    let res_json = match serde_json::to_string(&res) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to serialize agent response: {}", e);
            return;
        }
    };

    let (header_pub, header_n, header_pn, out_ciphertext) = match ratchet_state.ratchet_encrypt(res_json.as_bytes()) {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to encrypt response via Double Ratchet: {}", e);
            return;
        }
    };
    
    let out_payload = RatchetPayload {
        header_pub: STANDARD.encode(header_pub),
        header_n,
        header_pn,
        ciphertext: STANDARD.encode(out_ciphertext),
    };

    // Update Ratchet State in DB (Encrypted at rest)
    if let Ok(new_state_json) = serde_json::to_string(&ratchet_state) {
        let new_state_enc = crate::auth::encrypt_db_field(&new_state_json, &state.config.jwt_secret).unwrap_or_default();
        let _ = sqlx::query(
            "UPDATE paired_devices SET ratchet_state = $1 WHERE client_id = $2"
        )
        .bind(new_state_enc)
        .bind(&envelope.sender_id)
        .execute(&state.db).await;
    }

    let out_payload_json = match serde_json::to_string(&out_payload) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to serialize out_payload: {}", e);
            return;
        }
    };

    let response_envelope = A2aEnvelope {
        sender_id: envelope.receiver_id.clone(),
        receiver_id: envelope.sender_id.clone(),
        group_id: envelope.group_id.clone(),
        payload_encrypted: STANDARD.encode(&out_payload_json),
    };

    let out_str = match serde_json::to_string(&response_envelope) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to serialize A2A response envelope: {}", e);
            return;
        }
    };

    if let Err(e) = out_tx.send(Message::Text(out_str.into())).await {
        error!("A2A output channel closed — response dropped: {}", e);
    }
}
