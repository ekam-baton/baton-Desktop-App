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

use std::{net::SocketAddr, sync::Arc};

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    trace::TraceLayer,
};
use tracing::info;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://baton:batonpassword@localhost:5432/baton".to_owned());
    
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .connect(&db_url)
        .await
        .unwrap_or_else(|_| {
            panic!("Could not connect to Postgres at {}. Ensure the database exists and credentials are correct.", db_url)
        });

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    // ── State ─────────────────────────────────────────────────────────────────
    let state = Arc::new(AppState::new(config.clone(), pool));

    // ── mDNS ──────────────────────────────────────────────────────────────────
    let mdns_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = mdns::start_mdns_broadcast(mdns_state).await {
            tracing::warn!("mDNS broadcast failed: {}", e);
        }
    });

    // ── Router ────────────────────────────────────────────────────────────────
    let app = Router::new()
        // Public endpoints (no auth required)
        .route("/health",   get(handlers::health::health_handler))
        .route("/ready",    get(handlers::health::ready_handler))
        .route("/metrics",  get(handlers::metrics::metrics_handler))
        .route("/pair",     post(handlers::pair::pair_handler))
        // Authenticated endpoints
        .route("/mcp/chat",    post(handlers::chat::chat_handler))
        .route("/mcp/execute", post(handlers::chat::execute_handler))
        .route("/mcp/tools",   get(handlers::tools::tools_handler))
        .route("/upload",      post(handlers::upload::upload_handler))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(
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

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    info!("Shutting down MCP Connector...");
}
