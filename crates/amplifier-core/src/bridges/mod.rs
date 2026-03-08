//! Transport bridge implementations.
//!
//! Each bridge wraps a remote module (gRPC, WASM, etc.) as an `Arc<dyn Trait>`,
//! making it indistinguishable from an in-process Rust module.

pub mod grpc_approval;
pub mod grpc_context;
pub mod grpc_hook;
pub mod grpc_orchestrator;
pub mod grpc_provider;
pub mod grpc_tool;
#[cfg(feature = "wasm")]
pub mod wasm_approval;
#[cfg(feature = "wasm")]
pub mod wasm_context;
#[cfg(feature = "wasm")]
pub mod wasm_hook;
#[cfg(feature = "wasm")]
pub mod wasm_orchestrator;
#[cfg(feature = "wasm")]
pub mod wasm_provider;
#[cfg(feature = "wasm")]
pub mod wasm_tool;

// ── WASM engine factory & resource limits ──────────────────────────────

#[cfg(feature = "wasm")]
use std::sync::Arc;
#[cfg(feature = "wasm")]
use wasmtime::Engine;

/// Default WASM execution limits.
#[cfg(feature = "wasm")]
pub struct WasmLimits {
    /// Maximum epoch ticks before trap (at ~100 ticks/sec, 3000 = 30 seconds).
    pub max_epoch_ticks: u64,
    /// Maximum memory in bytes (default: 64 MB).
    pub max_memory_bytes: usize,
}

#[cfg(feature = "wasm")]
impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            max_epoch_ticks: 3000,      // ~30 seconds at 100Hz
            max_memory_bytes: 64 << 20, // 64 MB
        }
    }
}

/// Create a wasmtime Engine with epoch interruption enabled and a background
/// ticker thread that increments the epoch every 10ms (~100Hz).
#[cfg(feature = "wasm")]
pub fn create_wasm_engine() -> Result<Arc<Engine>, Box<dyn std::error::Error + Send + Sync>> {
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    config.epoch_interruption(true);
    let engine = Arc::new(Engine::new(&config)?);

    let engine_clone = Arc::clone(&engine);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(10));
        engine_clone.increment_epoch();
    });

    Ok(engine)
}
