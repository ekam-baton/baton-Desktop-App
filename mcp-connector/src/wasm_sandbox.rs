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

    let result = tokio::task::spawn_blocking(move || {
        run_wasm_tool_blocking(&wasm_path, &granted_dirs, memory_mb, &input_json)
    });

    match tokio::time::timeout(std::time::Duration::from_secs(WASM_TIMEOUT_SECS), result).await {
        Ok(Ok(inner)) => inner,
        Ok(Err(join_err)) => Err(anyhow!("Wasm sandbox thread panicked: {}", join_err)),
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

    let wasm_bytes = std::fs::read(wasm_path)
        .map_err(|e| anyhow!("Could not read wasm file '{}': {}", wasm_path, e))?;
    let module = Module::new(&engine, &wasm_bytes)?;

    let mut wasi_builder = WasiCtxBuilder::new();
    let mut stdout_buf = Vec::new();

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
        let dir = std::fs::File::open(&dir_path)
            .map_err(|e| anyhow!("Cannot open granted dir '{}': {}", dir_path_str, e))?;
        wasi_builder.preopened_dir(
            Dir::from_std_file(dir),
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
    store.set_fuel(10_000_000_000).unwrap(); // Give it ~10B instructions of fuel before trapping

    let mut linker: Linker<SandboxState> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |s| &mut s.wasi)?;

    let instance = linker.instantiate(&mut store, &module)?;
    let start_fn = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .map_err(|_| anyhow!("Wasm module has no '_start' export"))?;

    start_fn.call(&mut store, ())?;

    drop(store);
    stdout_buf = stdout_pipe.try_into_inner().expect("WASI engine execution failed").into_inner();

    let output = String::from_utf8(stdout_buf)
        .map_err(|e| anyhow!("Wasm stdout contained invalid UTF-8: {}", e))?;

    info!("🧱 Wasm Sandbox: '{}' completed ({} bytes)", wasm_path, output.len());
    Ok(output)
}
