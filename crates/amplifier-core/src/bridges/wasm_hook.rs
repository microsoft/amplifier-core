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

/// Shorthand for the common boxed-error result used throughout WASM bridges.
type WasmResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Rust mirror of the WIT `event-subscription` record exported by hook modules.
///
/// Used exclusively for lifting the Component Model return value of
/// `get-subscriptions`.  Converted to `(String, i32, String)` tuples at the
/// public API boundary.
#[derive(wasmtime::component::ComponentType, wasmtime::component::Lift, Debug, Clone)]
#[component(record)]
struct WasmEventSubscription {
    #[component(name = "event")]
    event: String,
    #[component(name = "priority")]
    priority: i32,
    #[component(name = "name")]
    name: String,
}

/// Helper: call the `handle` export on a fresh component instance.
///
/// The envelope bytes must be a JSON-serialized object:
/// `{"event": "<event-name>", "data": <data-value>}`
fn call_handle(
    engine: &Engine,
    component: &Component,
    envelope_bytes: Vec<u8>,
) -> WasmResult<Vec<u8>> {
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

/// Helper: call the `get-subscriptions` export on a fresh component instance.
///
/// `config_bytes` must be a JSON-serialized configuration blob (from bundle YAML).
/// Returns a vec of `(event, priority, name)` tuples describing the hook's
/// desired subscriptions.
fn call_get_subscriptions(
    engine: &Engine,
    component: &Component,
    config_bytes: Vec<u8>,
) -> WasmResult<Vec<(String, i32, String)>> {
    let (linker, mut store) = create_linker_and_store(engine, &super::WasmLimits::default())?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = super::get_typed_func::<(Vec<u8>,), (Vec<WasmEventSubscription>,)>(
        &instance,
        &mut store,
        "get-subscriptions",
        INTERFACE_NAME,
    )?;
    let (subs,) = func.call(&mut store, (config_bytes,))?;
    Ok(subs
        .into_iter()
        .map(|s| (s.event, s.priority, s.name))
        .collect())
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
    pub fn from_bytes(wasm_bytes: &[u8], engine: Arc<Engine>) -> WasmResult<Self> {
        let component = Component::new(&engine, wasm_bytes)?;
        Ok(Self { engine, component })
    }

    /// Query the component for its event subscriptions.
    ///
    /// Instantiates the component, calls `get-subscriptions` with the given
    /// JSON config (serialized to bytes), and returns a vec of
    /// `(event, priority, name)` tuples.
    pub fn get_subscriptions(
        &self,
        config: &serde_json::Value,
    ) -> WasmResult<Vec<(String, i32, String)>> {
        let config_bytes = serde_json::to_vec(config)
            .map_err(|e| format!("failed to serialize config for get-subscriptions: {e}"))?;
        call_get_subscriptions(&self.engine, &self.component, config_bytes)
    }

    /// Convenience: load a WASM hook component from a file path.
    pub fn from_file(path: &Path, engine: Arc<Engine>) -> WasmResult<Self> {
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

    #[test]
    fn deny_hook_get_subscriptions_returns_expected() {
        let engine = make_engine();
        let bytes = deny_hook_wasm_bytes();
        let bridge = WasmHookBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        let config = serde_json::json!({});
        let subs = bridge
            .get_subscriptions(&config)
            .expect("get_subscriptions should succeed");

        assert_eq!(subs.len(), 1, "deny-hook declares exactly one subscription");
        let (event, priority, name) = &subs[0];
        assert_eq!(event, "tool:pre");
        assert_eq!(*priority, 0);
        assert_eq!(name, "deny-all");
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
