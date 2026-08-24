use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tracing::info;
use tokio::sync::Mutex;
use anyhow::{anyhow, Result};
use serde_json::Value;

/// Maximum bytes we'll read from a single agent response line (4 MB).
const MAX_LINE_BYTES: u64 = 4 * 1024 * 1024;

/// Maximum seconds to wait for the agent to respond before timing out.
const RESPONSE_TIMEOUT_SECS: u64 = 30;

/// Default memory cap for Wasm sandboxed tools (128 MB).
const WASM_DEFAULT_MEMORY_MB: u64 = 128;

pub struct StdioMcpClient {
    child: Mutex<Option<Child>>,
    cmd: String,
    args: Vec<String>,
    /// Directories to grant to Wasm tools. Empty = zero FS access.
    wasm_granted_dirs: Vec<PathBuf>,
}

impl StdioMcpClient {
    pub fn new(cmd: String, args: Vec<String>) -> Self {
        Self {
            child: Mutex::new(None),
            cmd,
            args,
            wasm_granted_dirs: vec![],
        }
    }

    /// Create a client with explicit Wasm directory grants.
    pub fn new_wasm(cmd: String, args: Vec<String>, granted_dirs: Vec<PathBuf>) -> Self {
        Self {
            child: Mutex::new(None),
            cmd,
            args,
            wasm_granted_dirs: granted_dirs,
        }
    }

    async fn spawn_child(&self) -> Result<Child> {
        info!("Starting MCP Stdio agent: {} {:?}", self.cmd, self.args);
        let child = Command::new(&self.cmd)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Redirect stderr to /dev/null to prevent agent logs from leaking
            // into our stdout and corrupting the JSON-RPC stream.
            .stderr(Stdio::null())
            .kill_on_drop(true)   // Kill subprocess when the guard is dropped
            .spawn()?;
        Ok(child)
    }

    pub async fn ensure_started(&self) -> Result<()> {
        // Wasm tools are stateless per-call — no persistent child process needed.
        if self.cmd.ends_with(".wasm") {
            return Ok(());
        }
        let mut child_guard = self.child.lock().await;
        if child_guard.is_none() {
            *child_guard = Some(self.spawn_child().await?);
        }
        Ok(())
    }

    /// Send a JSON-RPC request to the agent and receive the matching response.
    ///
    /// For `.wasm` tools: routes through the Wasmtime sandbox (Pillar 4 — V2).
    /// For native tools: uses the existing stdio subprocess path (unchanged).
    pub async fn send_request(&self, request: &crate::mcp_models::McpRequest) -> Result<Value> {
        // ── Wasm Sandbox Path (Pillar 4) ────────────────────────────────────
        if self.cmd.ends_with(".wasm") {
            let input_json = serde_json::to_string(request)?;
            info!("🧱 Routing MCP tool through Wasm sandbox: {}", self.cmd);

            let raw_output = crate::wasm_sandbox::run_wasm_tool(
                &self.cmd,
                &self.wasm_granted_dirs,
                WASM_DEFAULT_MEMORY_MB,
                &input_json,
            )
            .await
            .map_err(|e| anyhow!("Wasm sandbox execution failed: {}", e))?;

            // Parse the first valid JSON-RPC response line from wasm stdout.
            for line in raw_output.lines() {
                if let Ok(val) = serde_json::from_str::<Value>(line) {
                    if let Some(response_id) = val.get("id") {
                        if response_id == &request.id {
                            return Ok(val);
                        }
                    }
                }
            }
            return Err(anyhow!("Wasm tool produced no matching JSON-RPC response"));
        }

        // ── Native Subprocess Path (unchanged) ──────────────────────────────
        self.ensure_started().await?;

        let mut child_guard = self.child.lock().await;

        // If the child exited (crash/restart), respawn it.
        let child = child_guard.as_mut().ok_or_else(|| anyhow!("Agent process not running"))?;
        if let Ok(Some(_)) = child.try_wait() {
            *child_guard = Some(self.spawn_child().await?);
        }

        let child = child_guard.as_mut().ok_or_else(|| anyhow!("Agent process not running"))?;
        let stdin = child.stdin.as_mut().ok_or_else(|| anyhow!("No stdin handle"))?;
        let stdout = child.stdout.as_mut().ok_or_else(|| anyhow!("No stdout handle"))?;

        // Serialize and send (newline-delimited JSON)
        let mut req_str = serde_json::to_string(request)?;
        req_str.push('\n');
        stdin.write_all(req_str.as_bytes()).await?;
        stdin.flush().await?;

        // Read response with per-line size limit and overall timeout
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        let result = tokio::time::timeout(
            Duration::from_secs(RESPONSE_TIMEOUT_SECS),
            async {
                loop {
                    let line = lines.next_line().await?;
                    match line {
                        None => return Err(anyhow!("Agent stdout closed unexpectedly")),
                        Some(l) => {
                            if l.len() as u64 > MAX_LINE_BYTES {
                                return Err(anyhow!("Agent response line exceeded {} bytes", MAX_LINE_BYTES));
                            }
                            if let Ok(val) = serde_json::from_str::<Value>(&l) {
                                let matches = match val.get("id") {
                                    Some(response_id) => response_id == &request.id,
                                    None => false,
                                };
                                if matches {
                                    return Ok(val);
                                }
                            }
                        }
                    }
                }
            }
        ).await;

        match result {
            Ok(inner) => inner,
            Err(_elapsed) => Err(anyhow!("Agent timed out after {}s", RESPONSE_TIMEOUT_SECS)),
        }
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        // The `kill_on_drop(true)` on the Command handles cleanup.
    }
}
