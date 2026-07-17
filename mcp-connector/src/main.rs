/*!
 * Baton MCP Connector — Production Grade
 *
 * Routes MCP tool calls from the Baton mobile app to local agent harnesses
 * and cloud LLM providers. Handles device pairing, JWT auth, mDNS discovery,
 * streaming chat, file uploads, and Prometheus metrics.
 *
 * Port: 8081 (configurable via MCP_PORT)
 */

mod config;
mod state;
mod auth;
mod mdns;
mod providers;
mod adapters;
mod handlers;
pub mod mcp_models;
pub mod a2a_client;
mod scheduler;
pub mod knowledge_base;

use std::{
    net::SocketAddr,
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};
use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use governor::{Quota, RateLimiter};
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use sqlx::any::AnyPoolOptions;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing::info;

use crate::config::Config;
use crate::state::AppState;

/// Global rate limiter type alias for clarity.
type GlobalLimiter = Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>;

fn get_or_create_connector_id() -> String {
    let app_dir = if let Some(proj) = directories::ProjectDirs::from("com", "ekam", "baton") {
        proj.data_dir().to_path_buf()
    } else {
        std::path::PathBuf::from(".")
    };
    std::fs::create_dir_all(&app_dir).unwrap_or_default();
    
    let path = app_dir.join("baton_connector_id.txt");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let id = existing.trim().to_string();
        // LOW-02: validate that the persisted ID is valid hex (not tampered).
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit()) && id.len() <= 64 {
            return id;
        }
        tracing::warn!("Connector ID file '{}' is invalid or tampered — regenerating.", path.display());
    }
    let id = hex::encode(rand::random::<[u8; 16]>());
    if let Err(e) = std::fs::write(&path, &id) {
        tracing::warn!("Could not persist connector ID: {}", e);
    } else {
        // LOW-02: restrict file to owner-read-only on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
    id
}

/// Axum middleware: check the global rate limiter and return 429 if over quota.
async fn rate_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<GlobalLimiter>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if limiter.check().is_err() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(req).await)
}

pub async fn run_server() -> anyhow::Result<()> {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "ekam", "baton") {
        dotenvy::from_path(proj_dirs.data_dir().join(".env")).ok();
    }
    dotenv().ok();

    // ── Logging ──────────────────────────────────────────────────────────────
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".into());
    if log_format == "json" {
        tracing_subscriber::fmt().json().with_env_filter("info").init();
    } else {
        tracing_subscriber::fmt().pretty().with_env_filter("info").init();
    }

    let config = Config::from_env();

    // ── Database ─────────────────────────────────────────────────────────────
    // Defaults to embedded SQLite (zero-config) stored in the OS app data directory.
    // Set DATABASE_URL=postgres://... to use Postgres for production/VPS deployments.
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| {
            let db_path = config::get_app_data_dir().join("baton.db");
            format!("sqlite:{}?mode=rwc", db_path.to_str().unwrap_or("./baton.db"))
        });

    // Register both drivers so AnyPool can auto-detect from the URL scheme.
    sqlx::any::install_default_drivers();

    let pool = AnyPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&db_url)
        .await
        .unwrap_or_else(|e| {
            panic!("Could not connect to database at '{}': {}", db_url, e)
        });

    // Run migrations (works for both SQLite and Postgres)
    sqlx::migrate!("./migrations").run(&pool).await?;

    // ── State ─────────────────────────────────────────────────────────────────
    let state = Arc::new(AppState::new(config.clone(), pool));

    // ── Connector Identity ────────────────────────────────────────────────────
    let connector_id = get_or_create_connector_id();
    let data_dir = config::get_app_data_dir();
    
    // Print startup info for the user
    println!("\n╔══════════════════════════════════════════════════╗");
    println!("║          🔌  Baton MCP Connector v{}             ║", env!("CARGO_PKG_VERSION"));
    println!("╚══════════════════════════════════════════════════╝");
    println!("  Data directory : {}", data_dir.display());
    println!("  Admin dashboard: http://{}:{}/admin", config.host_ip, config.port);
    println!("  Admin password : stored in baton_admin_password.txt inside data directory");
    println!("  Connector ID   : {}", connector_id);
    println!();

    // ── Rate Limiters (MED-06) ────────────────────────────────────────────────
    // Global limiter: applied to all routes.
    let global_rpm = config.rate_limit_rpm;
    let global_limiter: GlobalLimiter = Arc::new(RateLimiter::direct(
        Quota::per_minute(NonZeroU32::new(global_rpm).unwrap_or(NonZeroU32::new(60).unwrap()))
    ));

    // Pairing limiter: much tighter — pairing is a sensitive unauthenticated endpoint.
    let pair_rpm = config.pair_rate_limit_per_min;
    let pair_limiter: GlobalLimiter = Arc::new(RateLimiter::direct(
        Quota::per_minute(NonZeroU32::new(pair_rpm).unwrap_or(NonZeroU32::new(5).unwrap()))
    ));

    // ── mDNS ──────────────────────────────────────────────────────────────────
    let mdns_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = mdns::start_mdns_broadcast(mdns_state).await {
            tracing::warn!("mDNS broadcast failed: {}", e);
        }
    });

    // ── A2A Client ────────────────────────────────────────────────────────────
    let a2a_state = state.clone();
    let a2a_id = connector_id.clone();
    tokio::spawn(async move {
        a2a_client::start_a2a_client(a2a_state, a2a_id).await;
    });

    // ── Scheduler ─────────────────────────────────────────────────────────────
    let scheduler_state = state.clone();
    tokio::spawn(async move {
        scheduler::start_scheduler(scheduler_state).await;
    });

    // ── Owner CLI (Fallback Permissions Management) ────────────────────────────
    let cli_state = state.clone();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.is_err() { break; }
            let cmd = line.trim();
            match cmd {
                "pending" => {
                    match sqlx::query("SELECT client_id, device_name FROM authorized_users WHERE status = 'pending' LIMIT 50")
                        .fetch_all(&cli_state.db).await
                    {
                        Ok(rows) => {
                            use sqlx::Row;
                            println!("--- Pending Approvals ---");
                            for row in rows {
                                let id: String = row.get("client_id");
                                let name: Option<String> = row.get("device_name");
                                println!("ID: {} | Name: {:?}", id, name);
                            }
                            println!("-------------------------");
                        }
                        Err(e) => tracing::error!("CLI DB error: {}", e),
                    }
                }
                _ if cmd.starts_with("approve ") => {
                    let id = cmd.trim_start_matches("approve ").trim();
                    // Validate hex format before using as query param.
                    if id.chars().all(|c| c.is_ascii_hexdigit()) && id.len() <= 64 {
                        let _ = sqlx::query("UPDATE authorized_users SET status = 'approved', role = 'guest' WHERE client_id = $1")
                            .bind(id)
                            .execute(&cli_state.db).await;
                        println!("Approved {}", id);
                    } else {
                        println!("Invalid client_id format.");
                    }
                }
                _ if cmd.starts_with("deny ") => {
                    let id = cmd.trim_start_matches("deny ").trim();
                    if id.chars().all(|c| c.is_ascii_hexdigit()) && id.len() <= 64 {
                        let _ = sqlx::query("UPDATE authorized_users SET status = 'denied' WHERE client_id = $1")
                            .bind(id)
                            .execute(&cli_state.db).await;
                        println!("Denied {}", id);
                    } else {
                        println!("Invalid client_id format.");
                    }
                }
                "help" => println!("Commands: pending | approve <id> | deny <id>"),
                "" => {}  // ignore blank lines
                _ => println!("Unknown command. Type 'help'."),
            }
        }
    });

    // ── Router ────────────────────────────────────────────────────────────────

    // Tight-rate-limited pairing sub-router.
    let pair_router = Router::new()
        .route("/pair", post(handlers::pair::pair_handler))
        .layer(middleware::from_fn_with_state(
            pair_limiter.clone(),
            rate_limit_middleware,
        ));

    // Metrics sub-router — protected by a separate bearer token.
    let metrics_router = Router::new()
        .route("/metrics", get(handlers::metrics::metrics_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics_auth_middleware,
        ));

    // Upload sub-router with a higher body size limit (50 MB for file uploads)
    let upload_router = Router::new()
        .route("/upload", post(handlers::upload::upload_handler))
        .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024)) // 50 MB for uploads
        .with_state(state.clone());

    let app = Router::new()
        // Health probes (no rate limit needed — used by orchestrators)
        .route("/health", get(handlers::health::health_handler))
        .route("/ready",  get(handlers::health::ready_handler))
        // Authenticated MCP endpoints
        .route("/mcp/chat",    post(handlers::chat::chat_handler))
        .route("/initialize",  post(handlers::mcp::initialize_handler))
        .route("/notifications/initialized", post(handlers::mcp::notifications_initialized_handler))
        .route("/tools/list",  post(handlers::mcp::tools_list_handler))
        .route("/tools/call",  post(handlers::mcp::tools_call_handler))
        // Admin dashboard
        .route("/admin",       get(handlers::admin::dashboard_handler))
        .route("/admin/api/pending", get(handlers::admin::pending_handler))
        .route("/admin/api/authorized", get(handlers::admin::authorized_handler))
        .route("/admin/api/approve/{client_id}", post(handlers::admin::approve_handler))
        .route("/admin/api/deny/{client_id}",    post(handlers::admin::deny_handler))
        .route("/admin/api/revoke/{client_id}",  post(handlers::admin::revoke_handler))
        // Features
        .route("/api/audit", get(handlers::features::get_audit_logs))
        .route("/api/permissions", get(handlers::features::get_permissions))
        .route("/api/permissions/{key}", post(handlers::features::update_permission))
        .route("/api/schedule", get(handlers::features::get_schedule))
        .route("/api/schedule", post(handlers::features::create_schedule))
        .route("/api/schedule/{id}", axum::routing::delete(handlers::features::delete_schedule))
        .route("/api/inbox", get(handlers::features::get_inbox))
        .route("/api/knowledge", get(handlers::features::get_knowledge))
        .route("/api/knowledge", post(handlers::features::create_knowledge))
        .route("/api/handoff", post(handlers::features::handoff_upload))
        // Merge sub-routers
        .merge(pair_router)
        .merge(metrics_router)
        .merge(upload_router)
        .with_state(state.clone())
        // Global rate limit middleware (MED-06)
        .layer(middleware::from_fn_with_state(
            global_limiter,
            rate_limit_middleware,
        ))
        // Hard body size limit — 1 MB max for non-upload requests
        .layer(RequestBodyLimitLayer::new(1 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(
            // Allow local-network origins for mobile app connections.
            // The mobile app connects from a different IP on the same WiFi network.
            CorsLayer::permissive()
        );

    let addr: SocketAddr = format!("0.0.0.0:{}", config.port).parse()?;
    info!("🔌  Baton MCP Connector v{} listening on {}", env!("CARGO_PKG_VERSION"), addr);

    // Print QR code for easy mobile pairing
    mdns::print_connection_qr(&config);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Middleware to protect /metrics with a bearer token (HIGH-06).
/// Set METRICS_TOKEN env var. If not set, /metrics is disabled entirely.
async fn metrics_auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let metrics_token = match std::env::var("METRICS_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => {
            // If METRICS_TOKEN is not configured, block access entirely.
            return Err(StatusCode::FORBIDDEN);
        }
    };

    let provided = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    // Constant-time comparison to prevent timing oracle.
    let token_bytes = metrics_token.as_bytes();
    let provided_bytes = provided.as_bytes();
    if token_bytes.len() != provided_bytes.len() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let diff = token_bytes.iter().zip(provided_bytes.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b));
    if diff != 0 {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let _ = state; // state available for future use
    Ok(next.run(req).await)
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    info!("Shutting down MCP Connector...");
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    run_server().await
}
