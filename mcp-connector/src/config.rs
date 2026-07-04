use std::net::IpAddr;

/// All configuration loaded from environment variables at startup.
/// Use `.env` file for local development.
#[derive(Clone, Debug)]
pub struct Config {
    // Server
    pub port: u16,
    pub host_ip: String,

    // Auth
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,

    // Database
    pub db_path: String,

    // Rate limiting
    pub rate_limit_rpm: u32,

    // Upload
    pub upload_dir: String,
    pub max_upload_bytes: u64,

    // LLM Providers — all optional; set whichever you have keys for
    pub nvidia_api_key: Option<String>,
    pub nvidia_base_url: String,
    pub nvidia_model: String,

    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,

    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,

    pub gemini_api_key: Option<String>,
    pub gemini_model: String,

    pub ollama_base_url: String,
    pub ollama_model: String,

    // Default provider when client doesn't specify
    pub default_provider: String,

    // A2A router URL (for relay sessions)
    pub a2a_router_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let host_ip = detect_local_ip();
        Self {
            port: env_u16("MCP_PORT", 8081),
            host_ip,

            jwt_secret: env_str(
                "JWT_SECRET",
                "baton-dev-secret-change-in-production",
            ),
            jwt_expiry_hours: env_u64("JWT_EXPIRY_HOURS", 24),

            db_path: env_str("DB_PATH", "baton.db"),

            rate_limit_rpm: env_u32("RATE_LIMIT_RPM", 60),

            upload_dir: env_str("UPLOAD_DIR", std::env::temp_dir().to_str().unwrap_or("/tmp")),
            max_upload_bytes: env_u64("MAX_UPLOAD_BYTES", 50 * 1024 * 1024), // 50 MB

            // Nvidia NIM (original provider)
            nvidia_api_key: std::env::var("NVIDIA_API_KEY").ok(),
            nvidia_base_url: env_str(
                "NVIDIA_BASE_URL",
                "https://integrate.api.nvidia.com/v1",
            ),
            nvidia_model: env_str("NVIDIA_MODEL", "meta/llama-3.1-70b-instruct"),

            // OpenAI-compatible
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            openai_base_url: env_str("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            openai_model: env_str("OPENAI_MODEL", "gpt-4o"),

            // Anthropic
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_model: env_str("ANTHROPIC_MODEL", "claude-3-5-sonnet-20241022"),

            // Google Gemini
            gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
            gemini_model: env_str("GEMINI_MODEL", "gemini-1.5-pro"),

            // Ollama (local)
            ollama_base_url: env_str("OLLAMA_BASE_URL", "http://localhost:11434"),
            ollama_model: env_str("OLLAMA_MODEL", "llama3"),

            default_provider: env_str("DEFAULT_PROVIDER", "Nvidianim"),

            a2a_router_url: env_str("A2A_ROUTER_URL", "ws://127.0.0.1:8080"),
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn env_u16(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Auto-detect the best local IP address (non-loopback IPv4)
fn detect_local_ip() -> String {
    // Try to find a non-loopback IPv4 address
    if let Ok(interfaces) = std::net::TcpListener::bind("0.0.0.0:0") {
        // Connect outward to detect our IP
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            let _ = socket.connect("8.8.8.8:80");
            if let Ok(addr) = socket.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_owned()
}
