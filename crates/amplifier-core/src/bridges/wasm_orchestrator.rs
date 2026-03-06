//! WASM bridge for sandboxed orchestrator modules (Component Model).
//!
//! [`WasmOrchestratorBridge`] loads a WASM Component via wasmtime and implements
//! the [`Orchestrator`] trait. Unlike Tier-1 bridges, the orchestrator component
//! **imports** `kernel-service` host functions that call back into the Coordinator.
//! These are registered on the [`Linker`] before instantiation.
//!
//! Gated behind the `wasm` feature flag.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Store};

use crate::coordinator::Coordinator;
use crate::errors::{AmplifierError, SessionError};
use crate::traits::{ContextManager, Orchestrator, Provider, Tool};

use super::wasm_tool::{WasmState, create_linker_and_store};

/// WIT interface name for the kernel-service host import (used by orchestrator guests).
const KERNEL_SERVICE_INTERFACE: &str = "amplifier:modules/kernel-service@1.0.0";

/// WIT interface name for the orchestrator export.
const ORCHESTRATOR_INTERFACE: &str = "amplifier:modules/orchestrator@1.0.0";

/// Shorthand for the typed function returned by the orchestrator `execute` export.
type OrchestratorExecuteFunc =
    wasmtime::component::TypedFunc<(Vec<u8>,), (Result<Vec<u8>, String>,)>;

/// A bridge that loads a WASM Component and exposes it as a native [`Orchestrator`].
///
/// The component is compiled once at construction time. Each `execute()` call:
/// 1. Creates a [`Linker`] with WASI + kernel-service host imports registered.
/// 2. Instantiates the component in a fresh [`Store`].
/// 3. Calls the WASM `execute` export inside `spawn_blocking`.
///
/// Host import closures use `tokio::runtime::Handle::current().block_on()` to
/// drive async coordinator operations from within the synchronous WASM context.
pub struct WasmOrchestratorBridge {
    engine: Arc<Engine>,
    component: Component,
    coordinator: Arc<Coordinator>,
}

impl WasmOrchestratorBridge {
    /// Load a WASM orchestrator component from raw bytes.
    ///
    /// Compiles the Component and stores the coordinator for use in host import
    /// closures. Unlike Tier-1 bridges, no eager `get-spec` call is made.
    pub fn from_bytes(
        wasm_bytes: &[u8],
        engine: Arc<Engine>,
        coordinator: Arc<Coordinator>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let component = Component::new(&engine, wasm_bytes)?;
        Ok(Self {
            engine,
            component,
            coordinator,
        })
    }

    /// Convenience: load a WASM orchestrator component from a file path.
    pub fn from_file(
        path: &Path,
        engine: Arc<Engine>,
        coordinator: Arc<Coordinator>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes, engine, coordinator)
    }
}

// ---------------------------------------------------------------------------
// Kernel-service host imports
// ---------------------------------------------------------------------------

/// Register all `kernel-service` host import functions on a component linker.
///
/// Each function captures an `Arc<Coordinator>` clone and dispatches to the
/// appropriate coordinator method. Async coordinator calls are driven via
/// `tokio::runtime::Handle::current().block_on()` (safe because WASM runs
/// inside `spawn_blocking` which executes on a non-async blocking thread that
/// still holds the outer Tokio runtime handle).
fn register_kernel_service_imports(
    linker: &mut Linker<WasmState>,
    coordinator: Arc<Coordinator>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut instance = linker.instance(KERNEL_SERVICE_INTERFACE)?;

    // ------------------------------------------------------------------
    // execute-tool: func(request: list<u8>) -> result<list<u8>, string>
    //
    // Request JSON: {"name": "<tool-name>", "input": <json-value>}
    // Response JSON: serialized ToolResult
    // ------------------------------------------------------------------
    {
        let coord = Arc::clone(&coordinator);
        instance.func_wrap(
            "execute-tool",
            move |_caller,
                  (request_bytes,): (Vec<u8>,)|
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("execute-tool: bad request: {e}"))?;
                    let name = req
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "execute-tool: missing 'name' field".to_string())?;
                    let input = req.get("input").cloned().unwrap_or(Value::Null);
                    let tool = coord
                        .get_tool(name)
                        .ok_or_else(|| format!("execute-tool: tool not found: {name}"))?;
                    let tool_result = tool
                        .execute(input)
                        .await
                        .map_err(|e| format!("execute-tool: execution failed: {e}"))?;
                    serde_json::to_vec(&tool_result)
                        .map_err(|e| format!("execute-tool: serialize failed: {e}"))
                });
                Ok((result,))
            },
        )?;
    }

    // ------------------------------------------------------------------
    // complete-with-provider: func(request: list<u8>) -> result<list<u8>, string>
    //
    // Request JSON: {"name": "<provider-name>", "request": <ChatRequest>}
    // Response JSON: serialized ChatResponse
    // ------------------------------------------------------------------
    {
        let coord = Arc::clone(&coordinator);
        instance.func_wrap(
            "complete-with-provider",
            move |_caller,
                  (request_bytes,): (Vec<u8>,)|
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("complete-with-provider: bad request: {e}"))?;
                    let name = req
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            "complete-with-provider: missing 'name' field".to_string()
                        })?;
                    let request_val = req.get("request").cloned().unwrap_or(Value::Null);
                    let provider = coord.get_provider(name).ok_or_else(|| {
                        format!("complete-with-provider: provider not found: {name}")
                    })?;
                    let chat_request: crate::messages::ChatRequest =
                        serde_json::from_value(request_val).map_err(|e| {
                            format!("complete-with-provider: bad ChatRequest: {e}")
                        })?;
                    let response = provider
                        .complete(chat_request)
                        .await
                        .map_err(|e| format!("complete-with-provider: failed: {e}"))?;
                    serde_json::to_vec(&response)
                        .map_err(|e| format!("complete-with-provider: serialize failed: {e}"))
                });
                Ok((result,))
            },
        )?;
    }

    // ------------------------------------------------------------------
    // emit-hook: func(request: list<u8>) -> result<list<u8>, string>
    //
    // Request JSON: {"event": "<event-name>", "data": <json-value>}
    // Response JSON: serialized HookResult
    // ------------------------------------------------------------------
    {
        let coord = Arc::clone(&coordinator);
        instance.func_wrap(
            "emit-hook",
            move |_caller,
                  (request_bytes,): (Vec<u8>,)|
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("emit-hook: bad request: {e}"))?;
                    let event = req
                        .get("event")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "emit-hook: missing 'event' field".to_string())?;
                    let data = req.get("data").cloned().unwrap_or(Value::Null);
                    let hook_result = coord.hooks().emit(event, data).await;
                    serde_json::to_vec(&hook_result)
                        .map_err(|e| format!("emit-hook: serialize failed: {e}"))
                });
                Ok((result,))
            },
        )?;
    }

    // ------------------------------------------------------------------
    // get-messages: func(request: list<u8>) -> result<list<u8>, string>
    //
    // Request JSON: {} (empty, request bytes are ignored)
    // Response JSON: serialized Vec<Value>
    // ------------------------------------------------------------------
    {
        let coord = Arc::clone(&coordinator);
        instance.func_wrap(
            "get-messages",
            move |_caller,
                  (_request_bytes,): (Vec<u8>,)|
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let context = coord
                        .context()
                        .ok_or_else(|| "get-messages: no context manager mounted".to_string())?;
                    let messages = context
                        .get_messages()
                        .await
                        .map_err(|e| format!("get-messages: failed: {e}"))?;
                    serde_json::to_vec(&messages)
                        .map_err(|e| format!("get-messages: serialize failed: {e}"))
                });
                Ok((result,))
            },
        )?;
    }

    // ------------------------------------------------------------------
    // add-message: func(request: list<u8>) -> result<_, string>
    //
    // Request JSON: <message-value>
    // Returns unit on success.
    // ------------------------------------------------------------------
    {
        let coord = Arc::clone(&coordinator);
        instance.func_wrap(
            "add-message",
            move |_caller,
                  (request_bytes,): (Vec<u8>,)|
                  -> wasmtime::Result<(Result<(), String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let message: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("add-message: bad request: {e}"))?;
                    let context = coord
                        .context()
                        .ok_or_else(|| "add-message: no context manager mounted".to_string())?;
                    context
                        .add_message(message)
                        .await
                        .map_err(|e| format!("add-message: failed: {e}"))
                });
                Ok((result,))
            },
        )?;
    }

    // ------------------------------------------------------------------
    // get-capability: func(request: list<u8>) -> result<list<u8>, string>
    //
    // Request JSON: {"name": "<capability-name>"}
    // Response JSON: serialized capability Value
    // ------------------------------------------------------------------
    {
        let coord = Arc::clone(&coordinator);
        instance.func_wrap(
            "get-capability",
            move |_caller,
                  (request_bytes,): (Vec<u8>,)|
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result: Result<Vec<u8>, String> = (|| {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("get-capability: bad request: {e}"))?;
                    let name = req
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "get-capability: missing 'name' field".to_string())?;
                    match coord.get_capability(name) {
                        Some(val) => serde_json::to_vec(&val)
                            .map_err(|e| format!("get-capability: serialize failed: {e}")),
                        None => Err(format!("get-capability: not found: {name}")),
                    }
                })();
                Ok((result,))
            },
        )?;
    }

    // ------------------------------------------------------------------
    // register-capability: func(request: list<u8>) -> result<_, string>
    //
    // Request JSON: {"name": "<capability-name>", "value": <json-value>}
    // Returns unit on success.
    // ------------------------------------------------------------------
    {
        let coord = Arc::clone(&coordinator);
        instance.func_wrap(
            "register-capability",
            move |_caller,
                  (request_bytes,): (Vec<u8>,)|
                  -> wasmtime::Result<(Result<(), String>,)> {
                let result: Result<(), String> = (|| {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("register-capability: bad request: {e}"))?;
                    let name = req
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            "register-capability: missing 'name' field".to_string()
                        })?;
                    let value = req.get("value").cloned().unwrap_or(Value::Null);
                    coord.register_capability(name, value);
                    Ok(())
                })();
                Ok((result,))
            },
        )?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute export lookup
// ---------------------------------------------------------------------------

/// Look up the `execute` export from an orchestrator component instance.
///
/// Tries:
/// 1. Direct root-level export by `"execute"`
/// 2. Nested inside the [`ORCHESTRATOR_INTERFACE`] exported instance
fn get_execute_func(
    instance: &wasmtime::component::Instance,
    store: &mut Store<WasmState>,
) -> Result<OrchestratorExecuteFunc, Box<dyn std::error::Error + Send + Sync>> {
    // Try root-level first.
    if let Ok(f) = instance
        .get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(&mut *store, "execute")
    {
        return Ok(f);
    }

    // Try nested inside the interface-exported instance.
    let iface_idx = instance
        .get_export_index(&mut *store, None, ORCHESTRATOR_INTERFACE)
        .ok_or_else(|| {
            format!("export instance '{ORCHESTRATOR_INTERFACE}' not found")
        })?;
    let func_idx = instance
        .get_export_index(&mut *store, Some(&iface_idx), "execute")
        .ok_or_else(|| {
            format!("export 'execute' not found in '{ORCHESTRATOR_INTERFACE}'")
        })?;
    let func = instance
        .get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(&mut *store, &func_idx)
        .map_err(|e| format!("typed func lookup failed for 'execute': {e}"))?;
    Ok(func)
}

// ---------------------------------------------------------------------------
// Synchronous WASM call (for spawn_blocking)
// ---------------------------------------------------------------------------

/// Run the orchestrator `execute` call synchronously.
///
/// Creates a fresh linker (with WASI + kernel-service imports) and store,
/// instantiates the component, and calls the `execute` export.
/// Intended to be called from inside `tokio::task::spawn_blocking`.
fn call_execute_sync(
    engine: &Engine,
    component: &Component,
    coordinator: Arc<Coordinator>,
    request_bytes: Vec<u8>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Start with WASI-equipped linker + store.
    let (mut linker, mut store) = create_linker_and_store(engine)?;

    // Extend the linker with kernel-service host imports.
    register_kernel_service_imports(&mut linker, coordinator)?;

    let instance = linker.instantiate(&mut store, component)?;
    let func = get_execute_func(&instance, &mut store)?;
    let (result,) = func.call(&mut store, (request_bytes,))?;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(err.into()),
    }
}

// ---------------------------------------------------------------------------
// Orchestrator trait impl
// ---------------------------------------------------------------------------

impl Orchestrator for WasmOrchestratorBridge {
    /// Run the WASM agent loop for a single prompt.
    ///
    /// Only `prompt` is forwarded to the WASM guest as `{"prompt": "..."}` bytes.
    /// The `context`, `providers`, `tools`, `hooks`, and `coordinator` parameters
    /// are not serialized — the WASM guest accesses these via `kernel-service`
    /// host import callbacks that route through `self.coordinator`.
    fn execute(
        &self,
        prompt: String,
        _context: Arc<dyn ContextManager>,
        _providers: HashMap<String, Arc<dyn Provider>>,
        _tools: HashMap<String, Arc<dyn Tool>>,
        _hooks: Value,
        _coordinator: Value,
    ) -> Pin<Box<dyn Future<Output = Result<String, AmplifierError>> + Send + '_>> {
        Box::pin(async move {
            log::debug!(
                "WasmOrchestratorBridge::execute — context, providers, tools, hooks, and \
                 coordinator parameters are not forwarded to the WASM guest; the guest uses \
                 kernel-service host import callbacks routed through self.coordinator"
            );

            // Serialize request: {"prompt": "..."}
            let request_bytes =
                serde_json::to_vec(&serde_json::json!({"prompt": prompt})).map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!("failed to serialize orchestrator request: {e}"),
                    })
                })?;

            let engine = Arc::clone(&self.engine);
            let component = self.component.clone(); // Component is Arc-backed, cheap clone
            let coordinator = Arc::clone(&self.coordinator);

            let result_bytes = tokio::task::spawn_blocking(move || {
                call_execute_sync(&engine, &component, coordinator, request_bytes)
            })
            .await
            .map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("WASM orchestrator task panicked: {e}"),
                })
            })?
            .map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("WASM orchestrator execute failed: {e}"),
                })
            })?;

            // The guest macro serializes its String result as a JSON string,
            // so we deserialize the bytes back into a String.
            let result: String = serde_json::from_slice(&result_bytes).map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("failed to deserialize orchestrator result: {e}"),
                })
            })?;

            Ok(result)
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::testing::FakeTool;
    use crate::models::ToolResult;

    // ------------------------------------------------------------------
    // Compile-time check
    // ------------------------------------------------------------------

    /// Compile-time check: WasmOrchestratorBridge satisfies Arc<dyn Orchestrator>.
    #[allow(dead_code)]
    fn _assert_wasm_orchestrator_bridge_is_orchestrator(bridge: WasmOrchestratorBridge) {
        let _: Arc<dyn crate::traits::Orchestrator> = Arc::new(bridge);
    }

    // ------------------------------------------------------------------
    // WASM fixture helpers
    // ------------------------------------------------------------------

    /// Helper: read the passthrough-orchestrator.wasm fixture bytes.
    fn passthrough_orchestrator_wasm_bytes() -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let candidates = [
            manifest.join("../../../tests/fixtures/wasm/passthrough-orchestrator.wasm"),
            manifest.join("../../tests/fixtures/wasm/passthrough-orchestrator.wasm"),
        ];
        for p in &candidates {
            if p.exists() {
                return std::fs::read(p).unwrap_or_else(|e| {
                    panic!("Failed to read passthrough-orchestrator.wasm at {p:?}: {e}")
                });
            }
        }
        panic!(
            "passthrough-orchestrator.wasm not found. Tried: {:?}",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        );
    }

    /// Helper: read the echo-tool.wasm fixture bytes.
    fn echo_tool_wasm_bytes() -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let candidates = [
            manifest.join("../../../tests/fixtures/wasm/echo-tool.wasm"),
            manifest.join("../../tests/fixtures/wasm/echo-tool.wasm"),
        ];
        for p in &candidates {
            if p.exists() {
                return std::fs::read(p)
                    .unwrap_or_else(|e| panic!("Failed to read echo-tool.wasm at {p:?}: {e}"));
            }
        }
        panic!(
            "echo-tool.wasm not found. Tried: {:?}",
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

    // ------------------------------------------------------------------
    // Tests
    // ------------------------------------------------------------------

    /// E2E: passthrough-orchestrator calls execute-tool via kernel-service host import.
    ///
    /// Setup:
    ///   - Coordinator with FakeTool "echo-tool" that echoes input back
    ///   - WasmOrchestratorBridge wrapping passthrough-orchestrator.wasm
    ///
    /// Flow:
    ///   host execute() -> WASM execute() -> kernel-service::execute-tool (host import)
    ///   -> coordinator.get_tool("echo-tool") -> FakeTool.execute() -> returns ToolResult
    ///   -> WASM serializes result.to_string() -> host deserializes -> returns String
    #[tokio::test]
    async fn passthrough_orchestrator_calls_echo_tool() {
        let engine = make_engine();
        let bytes = passthrough_orchestrator_wasm_bytes();

        // Build a coordinator with a FakeTool that echoes the input back.
        let coordinator = Arc::new(crate::coordinator::Coordinator::new_for_test());
        let echo = Arc::new(FakeTool::with_responses(
            "echo-tool",
            "Echoes input back",
            vec![ToolResult {
                success: true,
                output: Some(serde_json::json!({"prompt": "hello from test"})),
                error: None,
            }],
        ));
        coordinator.mount_tool("echo-tool", echo);

        // Create the bridge.
        let bridge = WasmOrchestratorBridge::from_bytes(&bytes, engine, coordinator)
            .expect("from_bytes should succeed");

        // Execute the orchestrator.
        let result = bridge
            .execute(
                "hello from test".to_string(),
                Arc::new(crate::testing::FakeContextManager::new()),
                Default::default(),
                Default::default(),
                serde_json::json!({}),
                serde_json::json!({}),
            )
            .await;

        let response = result.expect("execute should succeed");
        // The passthrough-orchestrator returns result.to_string() where result is
        // the deserialized ToolResult JSON value.
        assert!(
            !response.is_empty(),
            "expected non-empty orchestrator response"
        );
        assert!(
            response.contains("echo-tool") || response.contains("prompt") || response.contains("hello"),
            "expected response to contain echoed data, got: {response}"
        );
    }

    /// E2E: passthrough-orchestrator with a native FakeTool that returns default output.
    ///
    /// Uses FakeTool::new (no preconfigured responses) — it echoes the input JSON back.
    #[tokio::test]
    async fn passthrough_orchestrator_with_default_fake_tool() {
        let engine = make_engine();
        let bytes = passthrough_orchestrator_wasm_bytes();

        let coordinator = Arc::new(crate::coordinator::Coordinator::new_for_test());
        // FakeTool::new echoes input back as output when no responses are preconfigured.
        coordinator.mount_tool("echo-tool", Arc::new(FakeTool::new("echo-tool", "echoes")));

        let bridge = WasmOrchestratorBridge::from_bytes(&bytes, Arc::clone(&engine), coordinator)
            .expect("from_bytes should succeed");

        let result = bridge
            .execute(
                "test prompt".to_string(),
                Arc::new(crate::testing::FakeContextManager::new()),
                Default::default(),
                Default::default(),
                serde_json::json!({}),
                serde_json::json!({}),
            )
            .await;

        let response = result.expect("execute should succeed");
        assert!(
            !response.is_empty(),
            "expected non-empty response, got: {response:?}"
        );
    }

    /// E2E: passthrough-orchestrator with the real WasmToolBridge (echo-tool.wasm).
    ///
    /// This is the full WASM-to-WASM path:
    ///   orchestrator WASM -> kernel-service import -> WasmToolBridge -> echo-tool WASM
    #[tokio::test]
    async fn passthrough_orchestrator_with_wasm_echo_tool() {
        let engine = make_engine();
        let orch_bytes = passthrough_orchestrator_wasm_bytes();
        let echo_bytes = echo_tool_wasm_bytes();

        let coordinator = Arc::new(crate::coordinator::Coordinator::new_for_test());

        // Mount the real WasmToolBridge for echo-tool.
        let echo_bridge = super::super::wasm_tool::WasmToolBridge::from_bytes(
            &echo_bytes,
            Arc::clone(&engine),
        )
        .expect("echo-tool bridge should load");
        coordinator.mount_tool("echo-tool", Arc::new(echo_bridge));

        let bridge = WasmOrchestratorBridge::from_bytes(&orch_bytes, Arc::clone(&engine), coordinator)
            .expect("from_bytes should succeed");

        let result = bridge
            .execute(
                "wasm-to-wasm".to_string(),
                Arc::new(crate::testing::FakeContextManager::new()),
                Default::default(),
                Default::default(),
                serde_json::json!({}),
                serde_json::json!({}),
            )
            .await;

        let response = result.expect("wasm-to-wasm execute should succeed");
        assert!(
            !response.is_empty(),
            "expected non-empty response from wasm-to-wasm path, got: {response:?}"
        );
    }

    /// Error case: execute-tool fails when tool is not mounted.
    #[tokio::test]
    async fn passthrough_orchestrator_tool_not_found_returns_error() {
        let engine = make_engine();
        let bytes = passthrough_orchestrator_wasm_bytes();

        // Coordinator with NO tools mounted.
        let coordinator = Arc::new(crate::coordinator::Coordinator::new_for_test());

        let bridge = WasmOrchestratorBridge::from_bytes(&bytes, engine, coordinator)
            .expect("from_bytes should succeed");

        let result = bridge
            .execute(
                "prompt".to_string(),
                Arc::new(crate::testing::FakeContextManager::new()),
                Default::default(),
                Default::default(),
                serde_json::json!({}),
                serde_json::json!({}),
            )
            .await;

        // Should fail because echo-tool is not mounted.
        assert!(
            result.is_err(),
            "expected error when tool not mounted, got: {result:?}"
        );
    }
}
