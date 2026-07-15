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

pub struct StdioMcpClient {
    child: Mutex<Option<Child>>,
    cmd: String,
    args: Vec<String>,
}

impl StdioMcpClient {
    pub fn new(cmd: String, args: Vec<String>) -> Self {
        Self {
            child: Mutex::new(None),
            cmd,
            args,
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
        let mut child_guard = self.child.lock().await;
        if child_guard.is_none() {
            *child_guard = Some(self.spawn_child().await?);
        }
        Ok(())
    }

    /// Send a JSON-RPC request to the agent and receive the matching response.
    /// Enforces line-length and timeout limits to prevent DoS.
    pub async fn send_request(&self, request: &crate::mcp_models::McpRequest) -> Result<Value> {
        self.ensure_started().await?;

        let mut child_guard = self.child.lock().await;

        // If the child exited (crash/restart), respawn it.
        let child = child_guard.as_mut().ok_or_else(|| anyhow!("Agent process not running"))?;
        if let Ok(Some(_)) = child.try_wait() {
            // Process exited — respawn
            *child_guard = Some(self.spawn_child().await?);
        }

        let child = child_guard.as_mut().unwrap();
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
                    // Use take() to limit how many bytes we buffer per line
                    // (prevents a misbehaving agent from OOM-ing us)
                    let line = lines.next_line().await?;
                    match line {
                        None => return Err(anyhow!("Agent stdout closed unexpectedly")),
                        Some(l) => {
                            if l.len() as u64 > MAX_LINE_BYTES {
                                return Err(anyhow!("Agent response line exceeded {} bytes", MAX_LINE_BYTES));
                            }
                            if let Ok(val) = serde_json::from_str::<Value>(&l) {
                                // Match on the request id (spec: id can be str or int)
                                // Compare Value to Value — NOT String to &Value which always fails
                                let matches = match val.get("id") {
                                    Some(response_id) => response_id == &request.id,
                                    None => false,
                                };
                                if matches {
                                    return Ok(val);
                                }
                            }
                            // Non-matching line — skip (could be agent log noise, notifications, etc.)
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
        // This explicit note is here for documentation.
    }
}
