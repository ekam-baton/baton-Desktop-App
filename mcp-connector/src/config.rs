/// All configuration loaded from environment variables at startup.
/// Use `.env` file for local development.
#[derive(Clone, Debug)]
pub struct Config {
    // Server
    pub port: u16,
    pub host_ip: String,

    // Auth — separate secrets for access and refresh tokens (MED-03)
    pub jwt_secret: String,
    pub jwt_refresh_secret: String,
    pub jwt_expiry_hours: u64,

    // Database
    pub db_path: String,

    // Rate limiting (MED-06)
    pub rate_limit_rpm: u32,
    pub pair_rate_limit_per_min: u32,

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
    // Separate secret for authenticating with the A2A router
    // (MUST be different from jwt_secret — never send JWT signing keys over the wire)
    pub a2a_router_secret: String,

    // MCP Agents configured via env vars (MCP_AGENT_<NAME>_CMD and MCP_AGENT_<NAME>_ARGS)
    pub mcp_agents: std::collections::HashMap<String, (String, Vec<String>)>,

    pub admin_password: String,
    
    // Identity & Ownership
    pub agent_owner_id: String,
    pub agent_owner_name: String,
    
    // E2EE
    pub x25519_private_key: [u8; 32],
    pub x25519_public_key: [u8; 32],
}

pub fn get_app_data_dir() -> std::path::PathBuf {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "ekam", "baton") {
        let dir = proj_dirs.data_dir();
        std::fs::create_dir_all(dir).unwrap_or_default();
        dir.to_path_buf()
    } else {
        std::path::PathBuf::from(".")
    }
}

fn get_or_create_secret(env_name: &str, filename: &str) -> String {
    if let Ok(from_env) = std::env::var(env_name) {
        if from_env.len() >= 32 { return from_env; }
    }
    let path = get_app_data_dir().join(filename);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let secret = existing.trim().to_string();
        if secret.len() >= 32 { return secret; }
    }
    let new_secret = hex::encode(rand::random::<[u8; 32]>());
    let _ = std::fs::write(&path, &new_secret);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    new_secret
}

impl Config {
    pub fn from_env() -> Self {
        let host_ip = detect_local_ip();
        let mut config = Self {
            port: env_u16("MCP_PORT", 8081),
            host_ip,

            jwt_secret: get_or_create_secret("JWT_SECRET", "baton_jwt_secret.txt"),
            jwt_refresh_secret: get_or_create_secret("JWT_REFRESH_SECRET", "baton_jwt_refresh_secret.txt"),
            jwt_expiry_hours: env_u64("JWT_EXPIRY_HOURS", 24),

            db_path: env_str("DB_PATH", get_app_data_dir().join("baton.db").to_str().unwrap_or("baton.db")),

            rate_limit_rpm: env_u32("RATE_LIMIT_RPM", 60),
            pair_rate_limit_per_min: env_u32("PAIR_RATE_LIMIT_PER_MIN", 5),

            upload_dir: env_str("UPLOAD_DIR", std::env::temp_dir().to_str().unwrap_or("/tmp")),
            max_upload_bytes: env_u64("MAX_UPLOAD_BYTES", 50 * 1024 * 1024), // 50 MB

            // Nvidia NIM
            nvidia_api_key: std::env::var("NVIDIA_API_KEY").ok(),
            nvidia_base_url: env_str("NVIDIA_BASE_URL", "https://integrate.api.nvidia.com/v1"),
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

            a2a_router_url: env_str("A2A_ROUTER_URL", "wss://baton-router.ekam.com"),
            a2a_router_secret: get_or_create_secret("A2A_ROUTER_SECRET", "baton_a2a_secret.txt"),
            mcp_agents: parse_mcp_agents_from_env(),
            admin_password: get_or_create_admin_password(),
            
            agent_owner_id: env_str("AGENT_OWNER_ID", ""),
            agent_owner_name: env_str("AGENT_OWNER_NAME", "Unknown"),
            
            x25519_private_key: [0; 32], // Placeholder, populated below
            x25519_public_key: [0; 32],
        };
        
        let (priv_key, pub_key) = get_or_create_x25519_keypair();
        config.x25519_private_key = priv_key;
        config.x25519_public_key = pub_key;

        // LOW-07 / MED-07: warn loudly if non-TLS WebSocket is configured for A2A.
        if config.a2a_router_url.starts_with("ws://") {
            tracing::warn!(
                "SECURITY WARNING: A2A_ROUTER_URL uses insecure ws:// protocol. \
                 Use wss:// in production to prevent MITM attacks on agent traffic."
            );
        }

        // LOW-07: warn if running without TLS.
        if std::env::var("TLS_CERT_PATH").is_err() {
            tracing::warn!(
                "SECURITY WARNING: TLS is not configured (TLS_CERT_PATH not set). \
                 All traffic including admin credentials are transmitted in plaintext. \
                 Use a TLS reverse proxy (caddy, nginx) or set TLS_CERT_PATH + TLS_KEY_PATH."
            );
        }

        config
    }
}

/// Persist admin password across restarts so the owner always knows it.
/// Reads from ADMIN_PASSWORD env var first, then from baton_admin_password.txt,
/// generating a fresh one only on the very first run.
fn get_or_create_admin_password() -> String {
    if let Ok(from_env) = std::env::var("ADMIN_PASSWORD") {
        if !from_env.is_empty() {
            return from_env;
        }
    }
    let path = get_app_data_dir().join("baton_admin_password.txt");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let pw = existing.trim().to_string();
        if !pw.is_empty() {
            return pw;
        }
    }
    // Generate a strong 128-bit random password on first run and persist it.
    let new_pw = hex::encode(rand::random::<[u8; 16]>());
    if let Err(e) = std::fs::write(&path, &new_pw) {
        eprintln!("WARNING: Could not persist admin password to {}: {}", path.display(), e);
    } else {
        // Set restrictive file permissions on Unix (owner-read-only).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        eprintln!("==> Admin dashboard password saved to: {} (keep this file secure!)", path.display());
    }
    new_pw
}

/// Persist an X25519 keypair for E2EE across restarts.
fn get_or_create_x25519_keypair() -> ([u8; 32], [u8; 32]) {
    let path = get_app_data_dir().join("baton_x25519_key.bin");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let mut priv_bytes = [0u8; 32];
            priv_bytes.copy_from_slice(&bytes);
            let secret = x25519_dalek::StaticSecret::from(priv_bytes);
            let public = x25519_dalek::PublicKey::from(&secret);
            return (priv_bytes, public.to_bytes());
        }
    }
    
    use rand::rngs::OsRng;
    let secret = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let public = x25519_dalek::PublicKey::from(&secret);
    
    let priv_bytes = secret.to_bytes();
    if let Err(e) = std::fs::write(&path, &priv_bytes) {
        eprintln!("WARNING: Could not persist X25519 key to {}: {}", path.display(), e);
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
    }
    
    (priv_bytes, public.to_bytes())
}

/// Parse MCP agent definitions from environment variables.
/// Uses shell-words for safe argument parsing that handles quoted strings. (LOW-03)
fn parse_mcp_agents_from_env() -> std::collections::HashMap<String, (String, Vec<String>)> {
    let mut agents = std::collections::HashMap::new();
    for (key, cmd) in std::env::vars() {
        if key.starts_with("MCP_AGENT_") && key.ends_with("_CMD") {
            let name = key
                .trim_start_matches("MCP_AGENT_")
                .trim_end_matches("_CMD")
                .to_string();
            let args_key = format!("MCP_AGENT_{}_ARGS", name);
            let args_str = std::env::var(&args_key).unwrap_or_default();

            // Use shell-words to properly parse quoted arguments (LOW-03).
            // e.g. --config "/path with spaces/config.json" is handled correctly.
            let args = shell_words::split(&args_str).unwrap_or_else(|e| {
                tracing::warn!("Could not parse args for agent {}: {} — using empty args", name, e);
                vec![]
            });

            agents.insert(name.to_lowercase(), (cmd, args));
        }
    }
    agents
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

/// Auto-detect the best local IP address (non-loopback IPv4).
/// Uses a UDP socket routing trick — no packet is actually sent (LOW-01, LOW-05).
fn detect_local_ip() -> String {
    // Bind a UDP socket and "connect" it to a remote address.
    // The OS selects the correct outbound interface without sending any packet.
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("1.1.1.1:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip().to_string();
                // Reject loopback — only return real LAN addresses.
                if !ip.starts_with("127.") && ip != "::1" {
                    return ip;
                }
            }
        }
    }
    "127.0.0.1".to_owned()
}
