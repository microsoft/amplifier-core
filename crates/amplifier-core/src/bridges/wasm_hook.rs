//! WASM bridge for sandboxed hook handler modules (Component Model).
//!
//! [`WasmHookBridge`] loads a WASM Component via wasmtime and implements the
//! [`HookHandler`] trait, enabling sandboxed in-process hook execution. The guest
//! exports `handle` (accepts a JSON envelope as bytes, returns JSON `HookResult`).
//!
//! Gated behind the `wasm` feature flag.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use crate::errors::HookError;
use crate::models::HookResult;
use crate::traits::HookHandler;
use serde_json::Value;
use wasmtime::component::Component;
use wasmtime::Engine;

use super::wasm_tool::create_linker_and_store;

/// The WIT interface name used by `cargo component` for hook handler exports.
const INTERFACE_NAME: &str = "amplifier:modules/hook-handler@1.0.0";

/// Helper: call the `handle` export on a fresh component instance.
///
/// The envelope bytes must be a JSON-serialized object:
/// `{"event": "<event-name>", "data": <data-value>}`
fn call_handle(
    engine: &Engine,
    component: &Component,
    envelope_bytes: Vec<u8>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let (linker, mut store) = create_linker_and_store(engine, &super::WasmLimits::default())?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = super::get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
        &instance,
        &mut store,
        "handle",
        INTERFACE_NAME,
    )?;
    let (result,) = func.call(&mut store, (envelope_bytes,))?;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(err.into()),
    }
}

/// A bridge that loads a WASM Component and exposes it as a native [`HookHandler`].
///
/// The component is compiled once and can be instantiated for each hook invocation.
/// `handle` is called per invocation inside a `spawn_blocking` task (wasmtime is synchronous).
pub struct WasmHookBridge {
    engine: Arc<Engine>,
    component: Component,
}

impl WasmHookBridge {
    /// Load a WASM hook component from raw bytes.
    ///
    /// Compiles the Component and caches it for reuse across `handle()` calls.
    pub fn from_bytes(
        wasm_bytes: &[u8],
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let component = Component::new(&engine, wasm_bytes)?;
        Ok(Self { engine, component })
    }

    /// Convenience: load a WASM hook component from a file path.
    pub fn from_file(
        path: &Path,
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes, engine)
    }
}

impl HookHandler for WasmHookBridge {
    fn handle(
        &self,
        event: &str,
        data: Value,
    ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
        let event = event.to_string();
        Box::pin(async move {
            // Serialize event + data as the JSON envelope the WASM guest expects.
            let envelope = serde_json::json!({"event": event, "data": data});
            let envelope_bytes = serde_json::to_vec(&envelope).map_err(|e| HookError::Other {
                message: format!("failed to serialize hook envelope: {e}"),
            })?;

            let engine = Arc::clone(&self.engine);
            let component = self.component.clone(); // Component is Arc-backed, cheap clone

            let result_bytes = tokio::task::spawn_blocking(move || {
                call_handle(&engine, &component, envelope_bytes)
            })
            .await
            .map_err(|e| HookError::Other {
                message: format!("WASM hook execution task panicked: {e}"),
            })?
            .map_err(|e| HookError::Other {
                message: format!("WASM handle failed: {e}"),
            })?;

            let hook_result: HookResult =
                serde_json::from_slice(&result_bytes).map_err(|e| HookError::Other {
                    message: format!("failed to deserialize HookResult: {e}"),
                })?;

            Ok(hook_result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Compile-time check: WasmHookBridge satisfies Arc<dyn HookHandler>.
    ///
    /// Note: the integration test in `tests/wasm_hook_e2e.rs` would have an equivalent
    /// check from the *public* API surface. This one catches breakage during unit-test
    /// runs without needing the integration test.
    #[allow(dead_code)]
    fn _assert_wasm_hook_bridge_is_hook_handler(bridge: WasmHookBridge) {
        let _: Arc<dyn crate::traits::HookHandler> = Arc::new(bridge);
    }

    /// Helper: read the deny-hook.wasm fixture bytes.
    ///
    /// The fixture lives at the workspace root under `tests/fixtures/wasm/`.
    /// CARGO_MANIFEST_DIR points to `amplifier-core/crates/amplifier-core`,
    /// so we walk up to the workspace root first.
    fn deny_hook_wasm_bytes() -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // Two candidates because the workspace root may be at different depths
        // depending on how the repo is checked out:
        //   - 3 levels up: used as a git submodule (super-repo/amplifier-core/crates/amplifier-core)
        //   - 2 levels up: standalone checkout (amplifier-core/crates/amplifier-core)
        let candidates = [
            manifest.join("../../../tests/fixtures/wasm/deny-hook.wasm"),
            manifest.join("../../tests/fixtures/wasm/deny-hook.wasm"),
        ];
        for p in &candidates {
            if p.exists() {
                return std::fs::read(p)
                    .unwrap_or_else(|e| panic!("Failed to read deny-hook.wasm at {p:?}: {e}"));
            }
        }
        panic!(
            "deny-hook.wasm not found. Tried: {:?}",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        );
    }

    /// Helper: create a shared engine with component model enabled.
    fn make_engine() -> Arc<Engine> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        Arc::new(Engine::new(&config).expect("engine creation failed"))
    }

    #[tokio::test]
    async fn deny_hook_returns_deny_action() {
        let engine = make_engine();
        let bytes = deny_hook_wasm_bytes();
        let bridge = WasmHookBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        let data = serde_json::json!({"key": "value"});
        let result = bridge.handle("test:event", data).await;
        let result = result.expect("handle should succeed");

        assert_eq!(result.action, crate::models::HookAction::Deny);
        assert!(
            result.reason.as_deref().unwrap_or("").contains("Denied"),
            "expected reason to contain 'Denied', got: {:?}",
            result.reason
        );
    }
}
