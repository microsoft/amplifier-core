//! WASM bridge for sandboxed context manager modules (Component Model).
//!
//! [`WasmContextBridge`] loads a WASM Component via wasmtime and implements the
//! [`ContextManager`] trait, enabling sandboxed in-process context management.
//!
//! UNLIKE tool and hook bridges, this bridge is **stateful**: the same WASM instance
//! persists across all calls. This allows the context manager to maintain an internal
//! message store (e.g., the `memory-context` fixture's `Vec<Value>`).
//!
//! Gated behind the `wasm` feature flag.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use wasmtime::component::Component;
use wasmtime::{Engine, Store};

use crate::errors::ContextError;
use crate::traits::{ContextManager, Provider};

use super::wasm_tool::{WasmState, create_linker_and_store};

/// The WIT interface name used by `cargo component` for context manager exports.
const INTERFACE_NAME: &str = "amplifier:modules/context-manager@1.0.0";

/// Shorthand for the fallible return type used by helper functions.
type WasmResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Look up a typed function export from the context manager component instance.
///
/// Tries:
/// 1. Direct root-level export by `func_name`
/// 2. Nested inside the [`INTERFACE_NAME`] exported instance
fn get_context_func<Params, Results>(
    instance: &wasmtime::component::Instance,
    store: &mut Store<WasmState>,
    func_name: &str,
) -> WasmResult<wasmtime::component::TypedFunc<Params, Results>>
where
    Params: wasmtime::component::Lower + wasmtime::component::ComponentNamedList,
    Results: wasmtime::component::Lift + wasmtime::component::ComponentNamedList,
{
    // Try direct root-level export first.
    if let Ok(f) = instance.get_typed_func::<Params, Results>(&mut *store, func_name) {
        return Ok(f);
    }

    // Try nested inside the interface-exported instance.
    let iface_idx = instance
        .get_export_index(&mut *store, None, INTERFACE_NAME)
        .ok_or_else(|| format!("export instance '{INTERFACE_NAME}' not found"))?;
    let func_idx = instance
        .get_export_index(&mut *store, Some(&iface_idx), func_name)
        .ok_or_else(|| {
            format!("export function '{func_name}' not found in '{INTERFACE_NAME}'")
        })?;
    let func = instance
        .get_typed_func::<Params, Results>(&mut *store, &func_idx)
        .map_err(|e| format!("typed func lookup failed for '{func_name}': {e}"))?;
    Ok(func)
}

/// A bridge that loads a WASM Component and exposes it as a native [`ContextManager`].
///
/// Unlike [`WasmToolBridge`] and [`WasmHookBridge`], this bridge is **stateful**.
/// The same WASM instance is reused across all calls, allowing the context manager
/// to maintain internal state (e.g., a `Vec<Value>` of messages). The store and
/// instance are protected by a [`tokio::sync::Mutex`].
///
/// # Concurrency note
///
/// WASM calls are synchronous CPU-bound work. For this bridge the WASM operations
/// are in-memory (no I/O), so holding the async mutex across them is acceptable.
/// A `spawn_blocking` offload is intentionally omitted here to keep the stateful
/// borrow simple; revisit if the context WASM modules become compute-heavy.
pub struct WasmContextBridge {
    /// Kept alive to ensure the engine outlives the compiled component/store.
    #[allow(dead_code)]
    engine: Arc<Engine>,
    /// Persistent (store, instance) pair — reused across every method call.
    state: tokio::sync::Mutex<(Store<WasmState>, wasmtime::component::Instance)>,
}

impl WasmContextBridge {
    /// Load a WASM context component from raw bytes.
    ///
    /// Compiles the Component and creates a **single** persistent store + instance
    /// that is reused for all subsequent method calls.
    pub fn from_bytes(
        wasm_bytes: &[u8],
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let component = Component::new(&engine, wasm_bytes)?;
        let (linker, mut store) = create_linker_and_store(&engine)?;
        let instance = linker.instantiate(&mut store, &component)?;

        Ok(Self {
            engine,
            state: tokio::sync::Mutex::new((store, instance)),
        })
    }

    /// Convenience: load a WASM context component from a file path.
    pub fn from_file(
        path: &Path,
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes, engine)
    }
}

impl ContextManager for WasmContextBridge {
    fn add_message(
        &self,
        message: Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        Box::pin(async move {
            let message_bytes =
                serde_json::to_vec(&message).map_err(|e| ContextError::Other {
                    message: format!("failed to serialize message: {e}"),
                })?;

            let mut guard = self.state.lock().await;
            let (store, instance) = &mut *guard;

            let func = get_context_func::<(Vec<u8>,), (Result<(), String>,)>(
                instance,
                store,
                "add-message",
            )
            .map_err(|e| ContextError::Other {
                message: format!("WASM add-message lookup failed: {e}"),
            })?;

            let (result,) =
                func.call(store, (message_bytes,))
                    .map_err(|e| ContextError::Other {
                        message: format!("WASM add-message call failed: {e}"),
                    })?;

            result.map_err(|e| ContextError::Other {
                message: format!("WASM add-message returned error: {e}"),
            })
        })
    }

    fn get_messages(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>> {
        Box::pin(async move {
            let mut guard = self.state.lock().await;
            let (store, instance) = &mut *guard;

            let func = get_context_func::<(), (Result<Vec<u8>, String>,)>(
                instance,
                store,
                "get-messages",
            )
            .map_err(|e| ContextError::Other {
                message: format!("WASM get-messages lookup failed: {e}"),
            })?;

            let (result,) = func.call(store, ()).map_err(|e| ContextError::Other {
                message: format!("WASM get-messages call failed: {e}"),
            })?;

            let bytes = result.map_err(|e| ContextError::Other {
                message: format!("WASM get-messages returned error: {e}"),
            })?;

            serde_json::from_slice::<Vec<Value>>(&bytes).map_err(|e| ContextError::Other {
                message: format!("failed to deserialize messages: {e}"),
            })
        })
    }

    fn get_messages_for_request(
        &self,
        token_budget: Option<i64>,
        provider: Option<Arc<dyn Provider>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>> {
        Box::pin(async move {
            let provider_name = provider
                .as_ref()
                .map(|p| p.name().to_string())
                .unwrap_or_default();

            let params = serde_json::json!({
                "token_budget": token_budget,
                "provider_name": provider_name,
            });
            let params_bytes =
                serde_json::to_vec(&params).map_err(|e| ContextError::Other {
                    message: format!("failed to serialize get-messages-for-request params: {e}"),
                })?;

            let mut guard = self.state.lock().await;
            let (store, instance) = &mut *guard;

            let func = get_context_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
                instance,
                store,
                "get-messages-for-request",
            )
            .map_err(|e| ContextError::Other {
                message: format!("WASM get-messages-for-request lookup failed: {e}"),
            })?;

            let (result,) =
                func.call(store, (params_bytes,))
                    .map_err(|e| ContextError::Other {
                        message: format!("WASM get-messages-for-request call failed: {e}"),
                    })?;

            let bytes = result.map_err(|e| ContextError::Other {
                message: format!("WASM get-messages-for-request returned error: {e}"),
            })?;

            serde_json::from_slice::<Vec<Value>>(&bytes).map_err(|e| ContextError::Other {
                message: format!("failed to deserialize messages for request: {e}"),
            })
        })
    }

    fn set_messages(
        &self,
        messages: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        Box::pin(async move {
            let messages_bytes =
                serde_json::to_vec(&messages).map_err(|e| ContextError::Other {
                    message: format!("failed to serialize messages: {e}"),
                })?;

            let mut guard = self.state.lock().await;
            let (store, instance) = &mut *guard;

            let func = get_context_func::<(Vec<u8>,), (Result<(), String>,)>(
                instance,
                store,
                "set-messages",
            )
            .map_err(|e| ContextError::Other {
                message: format!("WASM set-messages lookup failed: {e}"),
            })?;

            let (result,) =
                func.call(store, (messages_bytes,))
                    .map_err(|e| ContextError::Other {
                        message: format!("WASM set-messages call failed: {e}"),
                    })?;

            result.map_err(|e| ContextError::Other {
                message: format!("WASM set-messages returned error: {e}"),
            })
        })
    }

    fn clear(&self) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        Box::pin(async move {
            let mut guard = self.state.lock().await;
            let (store, instance) = &mut *guard;

            let func = get_context_func::<(), (Result<(), String>,)>(instance, store, "clear")
                .map_err(|e| ContextError::Other {
                    message: format!("WASM clear lookup failed: {e}"),
                })?;

            let (result,) = func.call(store, ()).map_err(|e| ContextError::Other {
                message: format!("WASM clear call failed: {e}"),
            })?;

            result.map_err(|e| ContextError::Other {
                message: format!("WASM clear returned error: {e}"),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    /// Compile-time check: WasmContextBridge satisfies Arc<dyn ContextManager>.
    ///
    /// This catches breakage during unit-test runs without needing the integration test.
    #[allow(dead_code)]
    fn _assert_wasm_context_bridge_is_context_manager(bridge: WasmContextBridge) {
        let _: Arc<dyn crate::traits::ContextManager> = Arc::new(bridge);
    }

    /// Helper: read the memory-context.wasm fixture bytes.
    ///
    /// The fixture lives at the workspace root under `tests/fixtures/wasm/`.
    /// CARGO_MANIFEST_DIR points to `amplifier-core/crates/amplifier-core`,
    /// so we walk up to the workspace root first.
    fn memory_context_wasm_bytes() -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // Two candidates because the workspace root may be at different depths
        // depending on how the repo is checked out:
        //   - 3 levels up: used as a git submodule (super-repo/amplifier-core/crates/amplifier-core)
        //   - 2 levels up: standalone checkout (amplifier-core/crates/amplifier-core)
        let candidates = [
            manifest.join("../../../tests/fixtures/wasm/memory-context.wasm"),
            manifest.join("../../tests/fixtures/wasm/memory-context.wasm"),
        ];
        for p in &candidates {
            if p.exists() {
                return std::fs::read(p).unwrap_or_else(|e| {
                    panic!("Failed to read memory-context.wasm at {p:?}: {e}")
                });
            }
        }
        panic!(
            "memory-context.wasm not found. Tried: {:?}",
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

    /// E2E stateful roundtrip: add → get → add → get → clear → get.
    ///
    /// This test verifies that the SAME WASM instance is reused across calls.
    /// If a new instance were created per call, the fixture's `MESSAGES` static
    /// would reset to empty on each invocation and the counts would never grow.
    #[tokio::test]
    async fn memory_context_stateful_roundtrip() {
        let engine = make_engine();
        let bytes = memory_context_wasm_bytes();
        let bridge =
            WasmContextBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        // Initially empty.
        let messages = bridge
            .get_messages()
            .await
            .expect("get_messages should succeed");
        assert_eq!(messages.len(), 0, "expected 0 messages initially");

        // Add first message.
        bridge
            .add_message(json!({"role": "user", "content": "hello"}))
            .await
            .expect("add_message should succeed");

        // Should have 1 message.
        let messages = bridge
            .get_messages()
            .await
            .expect("get_messages should succeed");
        assert_eq!(messages.len(), 1, "expected 1 message after first add");

        // Add second message.
        bridge
            .add_message(json!({"role": "assistant", "content": "hi"}))
            .await
            .expect("add_message should succeed");

        // Should have 2 messages.
        let messages = bridge
            .get_messages()
            .await
            .expect("get_messages should succeed");
        assert_eq!(messages.len(), 2, "expected 2 messages after second add");

        // Clear.
        bridge.clear().await.expect("clear should succeed");

        // Should be empty again.
        let messages = bridge
            .get_messages()
            .await
            .expect("get_messages should succeed");
        assert_eq!(messages.len(), 0, "expected 0 messages after clear");
    }
}
