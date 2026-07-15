use std::sync::Arc;

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use qrcode::QrCode;
use tracing::{info, warn};

use crate::config::Config;
use crate::state::AppState;

const SERVICE_TYPE: &str = "_baton._tcp.local.";

/// Broadcast this machine as a Baton MCP Connector via mDNS.
/// The Baton Android app listens for `_baton._tcp.local.` and auto-connects.
pub async fn start_mdns_broadcast(state: Arc<AppState>) -> Result<()> {
    let config = &state.config;
    let mdns = ServiceDaemon::new()?;

    let instance_name = format!("Baton-{}", &config.host_ip.replace('.', "-"));
    let host_name = format!("{}.local.", gethostname());

    let mut properties = std::collections::HashMap::new();
    properties.insert("version".to_owned(), env!("CARGO_PKG_VERSION").to_owned());
    properties.insert("secure".to_owned(), "true".to_owned());
    properties.insert("owner_name".to_owned(), config.agent_owner_name.clone());

    let service_info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &host_name,
        &config.host_ip.as_str(),
        config.port,
        Some(properties),
    )?;

    mdns.register(service_info)?;

    info!(
        "📡  mDNS: Registered as {} — Baton app can now auto-discover this server",
        instance_name
    );

    // Keep the daemon alive
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

/// Print QR code to the terminal so the developer can scan with Baton.
pub fn print_connection_qr(config: &Config) {
    let url = format!("http://{}:{}", config.host_ip, config.port);

    let banner = format!(
        "\n╔══════════════════════════════════════════╗\n\
         ║       📱  Scan to Connect in Baton        ║\n\
         ╚══════════════════════════════════════════╝"
    );
    println!("{}", banner);

    match QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let image = code
                .render::<char>()
                .quiet_zone(false)
                .module_dimensions(2, 1)
                .build();
            println!("{}", image);
        }
        Err(e) => {
            warn!("Failed to render QR code: {}", e);
        }
    }

    println!("  URL: {}", url);
    println!("  Or: open Baton → tap + → paste URL above\n");
}

fn gethostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_owned())
                .ok_or(std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "baton-server".to_owned())
}
