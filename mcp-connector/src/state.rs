use std::sync::Arc;

use dashmap::DashMap;
use prometheus::{
    register_counter_vec, register_gauge, register_histogram_vec, CounterVec, Gauge, HistogramVec,
};
use reqwest::Client;
use sqlx::AnyPool;

use crate::config::Config;

/// Central application state, shared across all handlers via `Arc`.
pub struct AppState {
    pub config: Config,
    pub db: AnyPool,
    pub http: Client,

    // Paired device store: client_id → public key (hex)
    // Also persisted in SQLite; this is an in-memory cache for speed.
    pub paired_devices: Arc<DashMap<String, String>>,

    // Metrics
    pub requests_total: CounterVec,
    pub streaming_duration: HistogramVec,
    pub active_connections: Gauge,
    pub provider_errors: CounterVec,
    pub upload_bytes: CounterVec,

    // Active MCP stdio clients
    pub mcp_clients: Arc<DashMap<String, Arc<crate::adapters::stdio::StdioMcpClient>>>,

    // Local Vector Search
    pub knowledge_base: Arc<crate::knowledge_base::KnowledgeBase>,

    // Queue for LLM concurrency limit
    pub llm_concurrency_limiter: Arc<tokio::sync::Semaphore>,

    // DLP Engine: blocks sensitive data from leaking to external LLMs (Pillar 5 — V2)
    pub dlp_engine: Arc<crate::dlp::DlpEngine>,
}

impl AppState {
    pub fn new(config: Config, db: AnyPool) -> Self {
        let requests_total = register_counter_vec!(
            "baton_mcp_requests_total",
            "Total MCP requests processed",
            &["provider", "status"]
        )
        .expect("Failed to register Prometheus requests metric");

        let streaming_duration = register_histogram_vec!(
            "baton_mcp_streaming_duration_seconds",
            "Duration of streaming responses in seconds",
            &["provider"],
            vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]
        )
        .expect("Failed to register Prometheus streaming metric");

        let active_connections = register_gauge!(
            "baton_mcp_active_connections",
            "Number of active streaming connections"
        )
        .expect("Failed to register Prometheus active connections metric");

        let provider_errors = register_counter_vec!(
            "baton_mcp_provider_errors_total",
            "Total errors per provider",
            &["provider", "error_kind"]
        )
        .expect("Failed to register Prometheus provider errors metric");

        let upload_bytes = register_counter_vec!(
            "baton_mcp_upload_bytes_total",
            "Total bytes uploaded",
            &["mime_type"]
        )
        .expect("Failed to register Prometheus upload metric");

        Self {
            config: config.clone(),
            db: db.clone(),
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to initialize internal reqwest HTTP client"),
            paired_devices: Arc::new(DashMap::new()),
            requests_total,
            streaming_duration,
            active_connections,
            provider_errors,
            upload_bytes,
            mcp_clients: Arc::new(DashMap::new()),
            knowledge_base: Arc::new(crate::knowledge_base::KnowledgeBase::new(db)),
            llm_concurrency_limiter: Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_chats as usize)),
            dlp_engine: Arc::new(crate::dlp::DlpEngine::new()),
        }
    }
}
