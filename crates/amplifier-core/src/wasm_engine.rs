//! Shared Wasmtime engine infrastructure.
//!
//! Provides a `WasmEngine` wrapper holding a shared `Arc<wasmtime::Engine>`
//! with the component model and epoch interruption enabled.

use std::sync::Arc;
use wasmtime::Engine;

/// Shared Wasmtime engine wrapper.
///
/// Holds an `Arc<Engine>` so clones share the same underlying engine.
/// The engine is configured with:
/// - Component Model support (`wasm_component_model(true)`)
/// - Epoch-based interruption (`epoch_interruption(true)`) — C-02
///
/// A background ticker thread is spawned on construction that increments
/// the engine's epoch counter every 10 ms (~100 Hz). Bridge stores set a
/// deadline of 3 000 ticks (~30 s) so runaway WASM modules are terminated
/// automatically.
#[derive(Clone)]
pub struct WasmEngine {
    engine: Arc<Engine>,
}

impl WasmEngine {
    /// Create a new `WasmEngine` with the component model and epoch
    /// interruption enabled, and spawn a background ticker thread.
    ///
    /// # Errors
    ///
    /// Returns an error if wasmtime fails to build the engine (e.g. unsupported
    /// CPU features). This is extremely rare in practice.
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        // C-02: enable epoch interruption so stores can set a time-budget.
        config.epoch_interruption(true);
        let engine = Arc::new(Engine::new(&config)?);

        // Spawn a background thread that increments the epoch every 10 ms
        // (~100 Hz). Stores set a deadline of 3 000 ticks (~30 s).
        // The thread holds a weak reference to the engine so that when the
        // last strong Arc is dropped the thread exits cleanly.
        let engine_weak = Arc::downgrade(&engine);
        std::thread::Builder::new()
            .name("wasm-epoch-ticker".into())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(10));
                match engine_weak.upgrade() {
                    Some(e) => e.increment_epoch(),
                    None => break, // engine dropped — stop the ticker
                }
            })?;

        Ok(Self { engine })
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

    /// Verify that `WasmEngine` has epoch interruption enabled.
    ///
    /// When `epoch_interruption(true)` is set on the engine config, the compiler
    /// inserts epoch check points at function entries and loop back-edges. Calling
    /// a function on a store whose epoch deadline has already been exceeded will
    /// immediately trap. Without epoch interruption the same function succeeds.
    ///
    /// RED: fails with current WasmEngine (no epoch_interruption) — nop() succeeds.
    /// GREEN: passes after enabling epoch_interruption + ticker (nop() traps).
    #[test]
    fn epoch_interruption_is_enabled() {
        let we = WasmEngine::new().expect("WasmEngine::new() should succeed");
        let engine = we.inner();

        // A minimal WAT module: one exported function with no body.
        // When epoch_interruption is enabled, wasmtime inserts an epoch check
        // at every function entry, so calling "nop" with an exceeded deadline traps.
        let wat = r#"(module (func (export "nop")))"#;

        let module = wasmtime::Module::new(&engine, wat).expect("simple WAT module should compile");

        let mut store = wasmtime::Store::new(&engine, ());
        // deadline = current_epoch (0) + 1 = 1
        store.set_epoch_deadline(1);
        // Manually advance the epoch so it equals the deadline.
        // With epoch_interruption enabled the next function call will trap.
        engine.increment_epoch();

        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .expect("module instantiation should succeed");
        let nop = instance
            .get_typed_func::<(), ()>(&mut store, "nop")
            .expect("nop export should exist");

        // With epoch_interruption enabled: epoch >= deadline → immediate trap.
        // With epoch_interruption disabled: epoch checks absent → Ok(()).
        let result = nop.call(&mut store, ());
        assert!(
            result.is_err(),
            "nop() should trap due to exceeded epoch deadline — \
             epoch_interruption may not be enabled in WasmEngine::new()"
        );
    }
}
