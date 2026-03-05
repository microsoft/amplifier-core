//! WASM bridge for sandboxed tool modules (Component Model).
//!
//! [`WasmToolBridge`] loads a WASM Component via wasmtime and implements the
//! [`Tool`] trait, enabling sandboxed in-process tool execution. The guest
//! exports `get-spec` (returns JSON-serialized `ToolSpec`) and `execute`
//! (accepts JSON input, returns JSON `ToolResult`).
//!
//! Gated behind the `wasm` feature flag.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Store};
use crate::errors::ToolError;
use crate::messages::ToolSpec;
use crate::models::ToolResult;
use crate::traits::Tool;

/// The WIT interface name used by `cargo component` for tool exports.
const INTERFACE_NAME: &str = "amplifier:modules/tool@1.0.0";

/// Store state for wasmtime, holding the WASI context required by
/// `cargo component`-generated modules.
pub(crate) struct WasmState {
    wasi: wasmtime_wasi::WasiCtx,
    table: wasmtime::component::ResourceTable,
}

impl wasmtime_wasi::WasiView for WasmState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// A bridge that loads a WASM Component and exposes it as a native [`Tool`].
///
/// The component is compiled once and can be instantiated for each execution.
/// `get-spec` is called at construction time; `execute` is called per invocation
/// inside a `spawn_blocking` task (wasmtime is synchronous).
pub struct WasmToolBridge {
    engine: Arc<Engine>,
    component: Component,
    name: String,
    spec: ToolSpec,
}

/// Create a linker with WASI imports registered and a store with WASI context.
fn create_linker_and_store(
    engine: &Engine,
) -> Result<(Linker<WasmState>, Store<WasmState>), Box<dyn std::error::Error + Send + Sync>> {
    let mut linker = Linker::<WasmState>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    let wasi = wasmtime_wasi::WasiCtxBuilder::new().build();
    let table = wasmtime::component::ResourceTable::new();
    let store = Store::new(engine, WasmState { wasi, table });
    Ok((linker, store))
}

/// Look up a typed function export from a component instance.
///
/// Component Model exports may be at the root level or nested inside an
/// exported interface instance. This helper tries:
/// 1. Direct root-level export by `func_name`
/// 2. Nested inside the [`INTERFACE_NAME`] exported instance
fn get_typed_func_from_instance<Params, Results>(
    instance: &wasmtime::component::Instance,
    store: &mut Store<WasmState>,
    func_name: &str,
) -> Result<wasmtime::component::TypedFunc<Params, Results>, Box<dyn std::error::Error + Send + Sync>>
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
        .ok_or_else(|| format!("export function '{func_name}' not found in '{INTERFACE_NAME}'"))?;
    let func = instance
        .get_typed_func::<Params, Results>(&mut *store, &func_idx)
        .map_err(|e| format!("typed func lookup failed for '{func_name}': {e}"))?;
    Ok(func)
}

/// Helper: call the `get-spec` export on a fresh component instance.
fn call_get_spec(
    engine: &Engine,
    component: &Component,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let (linker, mut store) = create_linker_and_store(engine)?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = get_typed_func_from_instance::<(), (Vec<u8>,)>(&instance, &mut store, "get-spec")?;
    let (spec_bytes,) = func.call(&mut store, ())?;
    Ok(spec_bytes)
}

/// Helper: call the `execute` export on a fresh component instance.
fn call_execute(
    engine: &Engine,
    component: &Component,
    input_bytes: Vec<u8>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let (linker, mut store) = create_linker_and_store(engine)?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = get_typed_func_from_instance::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
        &instance,
        &mut store,
        "execute",
    )?;
    let (result,) = func.call(&mut store, (input_bytes,))?;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(err.into()),
    }
}

impl WasmToolBridge {
    /// Load a WASM tool component from raw bytes.
    ///
    /// Compiles the Component, instantiates it once to call `get-spec`,
    /// and caches the resulting name and spec.
    pub fn from_bytes(
        wasm_bytes: &[u8],
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let component = Component::new(&engine, wasm_bytes)?;

        // Call get-spec to discover the tool's name and specification.
        let spec_bytes = call_get_spec(&engine, &component)?;
        let spec: ToolSpec = serde_json::from_slice(&spec_bytes)?;
        let name = spec.name.clone();

        Ok(Self {
            engine,
            component,
            name,
            spec,
        })
    }

    /// Convenience: load a WASM tool component from a file path.
    pub fn from_file(
        path: &Path,
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes, engine)
    }
}

impl Tool for WasmToolBridge {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        self.spec
            .description
            .as_deref()
            .unwrap_or("WASM tool module")
    }

    fn get_spec(&self) -> ToolSpec {
        self.spec.clone()
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<ToolResult, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let input_bytes = serde_json::to_vec(&input).map_err(|e| ToolError::Other {
                message: format!("failed to serialize input: {e}"),
            })?;

            let engine = Arc::clone(&self.engine);
            let component = self.component.clone(); // Component is Arc-backed, cheap clone

            let result_bytes = tokio::task::spawn_blocking(move || {
                call_execute(&engine, &component, input_bytes)
            })
            .await
            .map_err(|e| ToolError::Other {
                message: format!("WASM execution task panicked: {e}"),
            })?
            .map_err(|e| ToolError::Other {
                message: format!("WASM execute failed: {e}"),
            })?;

            let tool_result: ToolResult =
                serde_json::from_slice(&result_bytes).map_err(|e| ToolError::Other {
                    message: format!("failed to deserialize ToolResult: {e}"),
                })?;

            Ok(tool_result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Compile-time check: WasmToolBridge satisfies Arc<dyn Tool>.
    ///
    /// Note: the integration test in `tests/wasm_tool_e2e.rs` has an equivalent
    /// check from the *public* API surface. Both are intentional — this one
    /// catches breakage during unit-test runs without needing the integration
    /// test, while the integration test verifies the public export path.
    #[allow(dead_code)]
    fn _assert_wasm_tool_bridge_is_tool(bridge: WasmToolBridge) {
        let _: Arc<dyn crate::traits::Tool> = Arc::new(bridge);
    }

    /// Helper: read the echo-tool.wasm fixture bytes.
    ///
    /// The fixture lives at the workspace root under `tests/fixtures/wasm/`.
    /// CARGO_MANIFEST_DIR points to `amplifier-core/crates/amplifier-core`,
    /// so we walk up to the workspace root first.
    fn echo_tool_wasm_bytes() -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // Two candidates because the workspace root may be at different depths
        // depending on how the repo is checked out:
        //   - 3 levels up: used as a git submodule (super-repo/amplifier-core/crates/amplifier-core)
        //   - 2 levels up: standalone checkout (amplifier-core/crates/amplifier-core)
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
            candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
        );
    }

    /// Helper: create a shared engine with component model enabled.
    fn make_engine() -> Arc<Engine> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        Arc::new(Engine::new(&config).expect("engine creation failed"))
    }

    #[test]
    fn load_echo_tool_from_bytes() {
        let engine = make_engine();
        let bytes = echo_tool_wasm_bytes();
        let bridge =
            WasmToolBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        assert_eq!(bridge.name(), "echo-tool");

        let spec = bridge.get_spec();
        assert_eq!(spec.name, "echo-tool");
        assert_eq!(
            spec.description.as_deref(),
            Some("Echoes input back as output")
        );
        assert!(spec.parameters.contains_key("type"));
    }

    #[tokio::test]
    async fn echo_tool_execute_roundtrip() {
        let engine = make_engine();
        let bytes = echo_tool_wasm_bytes();
        let bridge =
            WasmToolBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        let input = serde_json::json!({"message": "hello", "count": 42});
        let result = bridge.execute(input.clone()).await;
        let result = result.expect("execute should succeed");

        assert!(result.success);
        assert_eq!(result.output, Some(input));
    }
}
