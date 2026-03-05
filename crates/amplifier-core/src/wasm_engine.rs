//! Shared Wasmtime engine infrastructure.
//!
//! Provides a `WasmEngine` wrapper holding a shared `Arc<wasmtime::Engine>`
//! with the component model enabled.

use std::sync::Arc;
use wasmtime::Engine;

/// Shared Wasmtime engine wrapper.
///
/// Holds an `Arc<Engine>` so clones share the same underlying engine.
#[derive(Clone)]
pub struct WasmEngine {
    engine: Arc<Engine>,
}

impl WasmEngine {
    /// Create a new `WasmEngine` with the component model enabled.
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config)?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Return a clone of the inner `Arc<Engine>`.
    pub fn inner(&self) -> Arc<Engine> {
        Arc::clone(&self.engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_creates_successfully() {
        let result = WasmEngine::new();
        assert!(result.is_ok(), "WasmEngine::new() should succeed");
    }

    #[test]
    fn engine_clone_shares_same_arc() {
        let engine1 = WasmEngine::new().expect("engine creation should succeed");
        let engine2 = engine1.clone();
        assert!(
            Arc::ptr_eq(&engine1.engine, &engine2.engine),
            "Cloned WasmEngine should share the same Arc"
        );
    }

    #[test]
    fn engine_inner_returns_valid_arc() {
        let engine = WasmEngine::new().expect("engine creation should succeed");
        let inner = engine.inner();
        assert!(
            Arc::strong_count(&inner) >= 2,
            "inner() should return an Arc with strong_count >= 2"
        );
    }
}
