/*!
 * Baton Wasm MCP Sandbox
 *
 * Executes MCP tools compiled to WebAssembly (.wasm) inside a Wasmtime
 * sandbox. Enforces memory caps, file system isolation, and timeouts.
 */

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tracing::{info, warn};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, Dir};

const WASM_TIMEOUT_SECS: u64 = 30;

struct SandboxState {
    wasi: WasiCtx,
    limits: wasmtime::StoreLimits,
}

pub async fn run_wasm_tool(
    wasm_path: &str,
    granted_dirs: &[PathBuf],
    memory_mb: u64,
    input_json: &str,
) -> Result<String> {
    let wasm_path = wasm_path.to_owned();
    let granted_dirs = granted_dirs.to_vec();
    let input_json = input_json.to_owned();

    // Spawn execution on an isolated, dedicated OS worker thread rather than Tokio's shared blocking threadpool.
    // This strictly protects Tokio's blocking pool (which is shared with SQLite transactions,
    // disk writes, and file streaming) from being starved by long-running or looping Wasm tools.
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("wasm-sandbox".to_string())
        .spawn(move || {
            let res = run_wasm_tool_blocking(&wasm_path, &granted_dirs, memory_mb, &input_json);
            let _ = tx.send(res);
        })
        .map_err(|e| anyhow!("Failed to spawn wasm sandbox worker thread: {}", e))?;

    match tokio::time::timeout(std::time::Duration::from_secs(WASM_TIMEOUT_SECS), rx).await {
        Ok(Ok(inner)) => inner,
        Ok(Err(_)) => Err(anyhow!("Wasm sandbox worker thread terminated without sending output")),
        Err(_) => Err(anyhow!("Wasm tool timed out after {}s", WASM_TIMEOUT_SECS)),
    }
}

fn run_wasm_tool_blocking(
    wasm_path: &str,
    granted_dirs: &[PathBuf],
    memory_mb: u64,
    input_json: &str,
) -> Result<String> {
    info!("🧱 Wasm Sandbox: loading {}", wasm_path);

    let mut engine_cfg = wasmtime::Config::new();
    engine_cfg.cranelift_opt_level(wasmtime::OptLevel::Speed);
    engine_cfg.max_wasm_stack(1 * 1024 * 1024);
    engine_cfg.consume_fuel(true);
    let engine = Engine::new(&engine_cfg)?;

    let metadata = std::fs::metadata(wasm_path)
        .map_err(|e| anyhow!("Could not inspect wasm file '{}': {}", wasm_path, e))?;
    if metadata.len() > 50 * 1024 * 1024 {
        return Err(anyhow!("Wasm binary exceeds 50MB limit"));
    }
    if metadata.len() < 8 {
        return Err(anyhow!("File too small to be a valid WebAssembly module"));
    }

    let wasm_bytes = std::fs::read(wasm_path)
        .map_err(|e| anyhow!("Could not read wasm file '{}': {}", wasm_path, e))?;
    if !wasm_bytes.starts_with(b"\0asm") {
        return Err(anyhow!("File '{}' is not a valid WebAssembly binary (missing \\0asm magic bytes)", wasm_path));
    }
    let module = Module::new(&engine, &wasm_bytes)?;

    let mut wasi_builder = WasiCtxBuilder::new();

    // Setup pipes
    let stdin_bytes = input_json.as_bytes().to_vec();
    let stdin_file = wasi_common::pipe::ReadPipe::from(stdin_bytes);
    wasi_builder.stdin(Box::new(stdin_file));

    // For stdout, we use a custom pipe to capture output
    let stdout_pipe = wasi_common::pipe::WritePipe::new_in_memory();
    wasi_builder.stdout(Box::new(stdout_pipe.clone()));

    if granted_dirs.is_empty() {
        warn!("🧱 Wasm Sandbox: no directories granted — zero FS access.");
    }
    for dir_path in granted_dirs {
        let dir_path_str = dir_path.to_string_lossy().to_string();
        let dir = Dir::open_ambient_dir(dir_path, wasmtime_wasi::sync::ambient_authority())
            .map_err(|e| anyhow!("Cannot open granted dir '{}': {}", dir_path_str, e))?;
        wasi_builder.preopened_dir(
            dir,
            &dir_path_str,
        )?;
    }

    let limits = wasmtime::StoreLimitsBuilder::new()
        .memory_size((memory_mb * 1024 * 1024) as usize)
        .build();

    let mut store = Store::new(
        &engine,
        SandboxState {
            wasi: wasi_builder.build(),
            limits,
        },
    );
    store.limiter(|state| &mut state.limits);
    // Enforce an instruction fuel cap (250 million instructions, ~100-200ms CPU runtime)
    // preventing infinite loops from running away with CPU cycles.
    store.set_fuel(250_000_000).map_err(|e| anyhow!("Failed to set wasm fuel: {}", e))?;

    let mut linker: Linker<SandboxState> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |s| &mut s.wasi)?;

    let instance = linker.instantiate(&mut store, &module)?;
    let start_fn = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .map_err(|_| anyhow!("Wasm module has no '_start' export"))?;

    start_fn.call(&mut store, ())?;

    drop(store);
    let stdout_buf = stdout_pipe
        .try_into_inner()
        .map_err(|_| anyhow!("Failed to extract stdout buffer from WASI pipe"))?
        .into_inner();

    const MAX_WASM_STDOUT_BYTES: usize = 10 * 1024 * 1024; // 10MB
    if stdout_buf.len() > MAX_WASM_STDOUT_BYTES {
        return Err(anyhow!(
            "Wasm tool stdout output exceeded 10MB limit ({} bytes produced)",
            stdout_buf.len()
        ));
    }

    let output = String::from_utf8(stdout_buf)
        .map_err(|e| anyhow!("Wasm stdout contained invalid UTF-8: {}", e))?;

    info!("🧱 Wasm Sandbox: '{}' completed ({} bytes)", wasm_path, output.len());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_engine_fuel_and_limits_config() {
        let mut engine_cfg = wasmtime::Config::new();
        engine_cfg.cranelift_opt_level(wasmtime::OptLevel::Speed);
        engine_cfg.max_wasm_stack(1 * 1024 * 1024);
        engine_cfg.consume_fuel(true);
        let engine = Engine::new(&engine_cfg);
        assert!(engine.is_ok(), "Engine with fuel configuration must initialize cleanly");

        let engine = engine.unwrap();
        let mut store = Store::new(&engine, ());
        assert!(store.set_fuel(250_000_000).is_ok(), "Fuel cap must be set successfully");
        assert_eq!(store.get_fuel().unwrap(), 250_000_000);
    }

    #[tokio::test]
    async fn test_wasm_sandbox_isolated_thread_execution() {
        // Verifies that a non-existent wasm path returns an immediate error without panicking
        // while running across the isolated thread channel
        let result = run_wasm_tool("non_existent_tool.wasm", &[], 16, "{}").await;
        assert!(result.is_err());
    }
}
