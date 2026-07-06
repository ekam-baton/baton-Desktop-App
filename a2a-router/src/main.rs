use std::{
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use dotenvy::dotenv;
use futures::{sink::SinkExt, stream::StreamExt};
use governor::{Quota, RateLimiter};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use prometheus::{register_counter_vec, register_gauge, CounterVec, Encoder, Gauge, TextEncoder};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use deadpool_redis::{Config as RedisConfig, Runtime};
use redis::AsyncCommands;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Config {
    port: u16,
    internal_url: String, // The URL other instances will use to reach this node (e.g., http://10.0.0.5:8080)
    jwt_secret: String,
    max_payload_bytes: usize,
    rate_limit_per_minute: u32,
    redis_url: String,
    database_url: String,
}

impl Config {
    fn from_env() -> Self {
        let port = std::env::var("A2A_PORT")
            .unwrap_or_else(|_| "8080".into())
            .parse()
            .unwrap_or(8080);
            
        // Use hostname for docker networks, or localhost for simple local testing
        let default_internal = format!("http://{}:{}", std::env::var("HOSTNAME").unwrap_or_else(|_| "127.0.0.1".into()), port);

        Self {
            port,
            internal_url: std::env::var("INTERNAL_URL").unwrap_or(default_internal),
            jwt_secret: std::env::var("JWT_SECRET")
                .expect("JWT_SECRET must be set in the environment for real-world usage"),
            max_payload_bytes: std::env::var("A2A_MAX_PAYLOAD_BYTES")
                .unwrap_or_else(|_| "102400".into())
                .parse()
                .unwrap_or(102_400),
            rate_limit_per_minute: std::env::var("A2A_RATE_LIMIT_RPM")
                .unwrap_or_else(|_| "120".into())
                .parse()
                .unwrap_or(120),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://baton:batonpassword@localhost:5432/baton".into()),
        }
    }
}

// ─── JWT Claims ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Deserialize, Debug)]
struct LoginRequest {
    client_id: String,
    secret: String,
}

#[derive(Serialize, Debug)]
struct LoginResponse {
    token: String,
}

// ─── State ───────────────────────────────────────────────────────────────────

type SessionSender = mpsc::UnboundedSender<Message>;

#[derive(Clone)]
struct AppState {
    /// Local connected sessions: client_id → websocket sender channel
    local_sessions: Arc<DashMap<String, SessionSender>>,
    config: Config,
    redis_pool: deadpool_redis::Pool,
    pg_pool: PgPool,
    http_client: reqwest::Client,
    
    // Prometheus metrics
    connections_gauge: Gauge,
    messages_routed: CounterVec,
    errors_total: CounterVec,
}

// ─── Wire types ──────────────────────────────────────────────────────────────

#[derive(Deserialize, Serialize, Clone, Debug)]
struct A2AMessage {
    sender_id: String,
    receiver_id: String,
    payload: serde_json::Value,
}

#[derive(Deserialize, Serialize)]
struct OfflineMessageRecord {
    id: Uuid,
    receiver_id: String,
    sender_id: String,
    payload: serde_json::Value,
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenv().ok();

    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".into());
    if log_format == "json" {
        tracing_subscriber::fmt().json().with_env_filter("info").init();
    } else {
        tracing_subscriber::fmt().pretty().with_env_filter("info").init();
    }

    let config = Config::from_env();

    // Redis
    let redis_cfg = RedisConfig::from_url(&config.redis_url);
    let redis_pool = redis_cfg.create_pool(Some(Runtime::Tokio1)).expect("Failed to create Redis pool");

    // Postgres
    let pg_pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await
        .unwrap_or_else(|_| panic!("Failed to connect to Postgres at {}", config.database_url));

    // Metrics
    let connections_gauge = register_gauge!("baton_a2a_active_connections", "Number of active WebSocket connections").unwrap();
    let messages_routed = register_counter_vec!("baton_a2a_messages_routed_total", "Total messages routed", &["status"]).unwrap();
    let errors_total = register_counter_vec!("baton_a2a_errors_total", "Total errors in the A2A relay", &["kind"]).unwrap();

    let state = Arc::new(AppState {
        local_sessions: Arc::new(DashMap::new()),
        config: config.clone(),
        redis_pool,
        pg_pool,
        http_client: reqwest::Client::new(),
        connections_gauge,
        messages_routed,
        errors_total,
    });

    let app = Router::new()
        .route("/ws/{client_id}", get(ws_handler))
        .route("/login", post(login_handler))
        .route("/internal/forward", post(internal_forward_handler))
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr: SocketAddr = format!("0.0.0.0:{}", config.port).parse().unwrap();
    info!("🔀  A2A Router (Direct Routing) listening on {}. Internal URL: {}", addr, config.internal_url);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "a2a-router" }))
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    if req.secret != state.config.jwt_secret {
        return (StatusCode::UNAUTHORIZED, "Invalid secret").into_response();
    }
    
    match mint_jwt(&req.client_id, &state.config.jwt_secret) {
        Ok(token) => Json(LoginResponse { token }).into_response(),
        Err(e) => {
            error!("Failed to mint JWT: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Error").into_response()
        }
    }
}

async fn ready_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").execute(&state.pg_pool).await.is_ok();
    let mut redis_ok = false;
    if let Ok(mut conn) = state.redis_pool.get().await {
        let res: redis::RedisResult<String> = redis::cmd("PING").query_async(&mut conn).await;
        redis_ok = res.is_ok();
    }
    
    if db_ok && redis_ok {
        Json(serde_json::json!({ "status": "ready", "local_sessions": state.local_sessions.len() })).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Dependencies unavailable").into_response()
    }
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        buffer,
    )
}

/// Internal endpoint to receive messages routed from peer nodes.
async fn internal_forward_handler(
    State(state): State<Arc<AppState>>,
    Json(msg): Json<A2AMessage>,
) -> impl IntoResponse {
    if let Some(tx) = state.local_sessions.get(&msg.receiver_id) {
        if let Ok(payload_str) = serde_json::to_string(&msg) {
            let _ = tx.send(Message::Text(payload_str.into()));
            state.messages_routed.with_label_values(&["delivered_local_from_peer"]).inc();
            return StatusCode::OK;
        }
    }
    // If not connected locally, it means they disconnected between the redis lookup and the HTTP call.
    // Fallback: save to offline queue.
    if let Err(e) = save_offline_message(&state.pg_pool, &msg).await {
        error!("Failed to save offline message during forward fallback: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    
    StatusCode::ACCEPTED
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(client_id): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let token = match extract_bearer(&headers) {
        Some(t) => t,
        None => return (StatusCode::UNAUTHORIZED, "Missing Authorization header").into_response(),
    };

    let claims = match validate_jwt(&token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid token").into_response(),
    };

    if claims.sub != client_id {
        return (StatusCode::FORBIDDEN, "client_id mismatch").into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, client_id, state))
        .into_response()
}

// ─── Socket handler ──────────────────────────────────────────────────────────

async fn handle_socket(socket: WebSocket, client_id: String, state: Arc<AppState>) {
    info!("Client connected: {}", client_id);
    state.connections_gauge.inc();

    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    state.local_sessions.insert(client_id.clone(), tx.clone());

    // 1. Maintain Presence in Redis
    let presence_key = format!("presence:{}", client_id);
    let my_url = state.config.internal_url.clone();
    let state_presence = state.clone();
    let p_key = presence_key.clone();
    
    let mut presence_task = tokio::spawn(async move {
        loop {
            if let Ok(mut conn) = state_presence.redis_pool.get().await {
                // Set presence with 30s TTL
                let _: redis::RedisResult<()> = conn.set_ex(&p_key, &my_url, 30).await;
            }
            tokio::time::sleep(Duration::from_secs(10)).await; // Refresh every 10s
        }
    });

    // 2. Flush Offline Messages
    let state_flush = state.clone();
    let cid_flush = client_id.clone();
    let tx_flush = tx.clone();
    tokio::spawn(async move {
        if let Ok(msgs) = fetch_and_delete_offline_messages(&state_flush.pg_pool, &cid_flush).await {
            for m in msgs {
                if let Ok(payload_str) = serde_json::to_string(&m) {
                    let _ = tx_flush.send(Message::Text(payload_str.into()));
                    state_flush.messages_routed.with_label_values(&["delivered_offline"]).inc();
                }
            }
        }
    });

    let quota = Quota::per_minute(std::num::NonZeroU32::new(state.config.rate_limit_per_minute).unwrap_or(std::num::NonZeroU32::new(120).unwrap()));
    let limiter = Arc::new(RateLimiter::direct(quota));
    let max_bytes = state.config.max_payload_bytes;

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let state_clone = state.clone();
    let cid = client_id.clone();
    
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Message::Text(ref text) = msg {
                if limiter.check().is_err() {
                    state_clone.errors_total.with_label_values(&["rate_limit"]).inc();
                    continue;
                }
                if text.len() > max_bytes {
                    state_clone.errors_total.with_label_values(&["payload_too_large"]).inc();
                    continue;
                }

                if let Ok(a2a_msg) = serde_json::from_str::<A2AMessage>(text) {
                    if a2a_msg.sender_id != cid {
                        continue; // spoof
                    }
                    
                    route_message(a2a_msg, &state_clone).await;
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => { recv_task.abort(); presence_task.abort(); },
        _ = &mut recv_task => { send_task.abort(); presence_task.abort(); },
    }

    state.local_sessions.remove(&client_id);
    state.connections_gauge.dec();
    
    // Clean up presence immediately
    if let Ok(mut conn) = state.redis_pool.get().await {
        let _: redis::RedisResult<()> = conn.del(presence_key).await;
    }
    info!("Client disconnected: {}", client_id);
}

// ─── Routing Core ────────────────────────────────────────────────────────────

async fn route_message(msg: A2AMessage, state: &Arc<AppState>) {
    // 1. Is it local?
    if let Some(tx) = state.local_sessions.get(&msg.receiver_id) {
        if let Ok(payload_str) = serde_json::to_string(&msg) {
            let _ = tx.send(Message::Text(payload_str.into()));
            state.messages_routed.with_label_values(&["delivered_local"]).inc();
            return;
        }
    }

    // 2. Is it on another node?
    let target_key = format!("presence:{}", msg.receiver_id);
    let mut routed_remote = false;
    
    if let Ok(mut conn) = state.redis_pool.get().await {
        if let Ok(Some(remote_url)) = conn.get::<_, Option<String>>(&target_key).await {
            let forward_url = format!("{}/internal/forward", remote_url);
            match state.http_client.post(&forward_url).json(&msg).send().await {
                Ok(resp) if resp.status().is_success() => {
                    state.messages_routed.with_label_values(&["forwarded_to_peer"]).inc();
                    routed_remote = true;
                }
                _ => {
                    warn!("Failed to forward to peer {}, falling back to offline queue", remote_url);
                }
            }
        }
    }

    // 3. If not routed remotely (either offline, or forward failed), queue it
    if !routed_remote {
        if let Err(e) = save_offline_message(&state.pg_pool, &msg).await {
            error!("Failed to save offline message: {}", e);
            state.errors_total.with_label_values(&["offline_queue_err"]).inc();
        } else {
            state.messages_routed.with_label_values(&["queued_offline"]).inc();
        }
    }
}

// ─── Database Helpers ────────────────────────────────────────────────────────

async fn save_offline_message(pool: &PgPool, msg: &A2AMessage) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO offline_messages (id, receiver_id, sender_id, payload) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(&msg.receiver_id)
    .bind(&msg.sender_id)
    .bind(&msg.payload)
    .execute(pool)
    .await?;
    Ok(())
}

async fn fetch_and_delete_offline_messages(pool: &PgPool, receiver_id: &str) -> Result<Vec<A2AMessage>, sqlx::Error> {
    use sqlx::Row;
    let records = sqlx::query(
        "SELECT id, sender_id, payload FROM offline_messages WHERE receiver_id = $1 ORDER BY created_at ASC",
    )
    .bind(receiver_id)
    .fetch_all(pool)
    .await?;

    let mut msgs = Vec::new();
    let mut ids = Vec::new();

    for r in records {
        let r_id: Uuid = r.get("id");
        let r_sender_id: String = r.get("sender_id");
        let r_payload: serde_json::Value = r.get("payload");

        msgs.push(A2AMessage {
            sender_id: r_sender_id,
            receiver_id: receiver_id.to_string(),
            payload: r_payload,
        });
        ids.push(r_id);
    }

    if !ids.is_empty() {
        sqlx::query("DELETE FROM offline_messages WHERE id = ANY($1)")
            .bind(&ids)
            .execute(pool)
            .await?;
    }

    Ok(msgs)
}

// ─── Security Helpers ────────────────────────────────────────────────────────

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_owned())
}

fn validate_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    let data = decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)?;
    Ok(data.claims)
}

fn mint_jwt(client_id: &str, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
        + 3600; // 1 hour token

    let claims = Claims {
        sub: client_id.to_string(),
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("Failed to install CTRL+C signal handler");
    info!("Shutting down A2A Router...");
}
