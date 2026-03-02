//! WASM bridge for sandboxed tool modules.
//!
//! [`WasmToolBridge`] loads a compiled WASM module via wasmtime and
//! implements the [`Tool`] trait, enabling sandboxed in-process tool
//! execution with the same proto message format as gRPC.
//!
//! Gated behind the `wasm` feature flag.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::errors::ToolError;
use crate::messages::ToolSpec;
use crate::models::ToolResult;
use crate::traits::Tool;

/// A bridge that loads a WASM module and exposes it as a native [`Tool`].
///
/// The WASM module is compiled once via wasmtime and can be instantiated
/// for each execution. Uses the same proto message serialization format
/// as gRPC bridges for consistency.
pub struct WasmToolBridge {
    _engine: wasmtime::Engine,
    _module: wasmtime::Module,
    name: String,
}

impl WasmToolBridge {
    /// Load a WASM tool from raw bytes.
    ///
    /// Compiles the WASM module and prepares it for execution.
    pub fn from_bytes(wasm_bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, wasm_bytes)?;
        let name = module.name().unwrap_or("wasm-tool").to_string();

        Ok(Self {
            _engine: engine,
            _module: module,
            name,
        })
    }
}

impl Tool for WasmToolBridge {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "WASM tool module"
    }

    fn get_spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            parameters: HashMap::new(),
            description: Some("WASM tool module".into()),
            extensions: HashMap::new(),
        }
    }

    fn execute(
        &self,
        _input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<ToolResult, ToolError>> + Send + '_>> {
        Box::pin(async move {
            // Phase 5 stub: full WASM ABI integration is future work.
            // The module is compiled and ready; execution requires defining
            // the host↔guest function interface (imports/exports).
            Err(ToolError::Other {
                message: "WasmToolBridge::execute() not yet implemented: \
                          WASM ABI host↔guest interface is future work"
                    .into(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[allow(dead_code)]
    fn assert_tool_trait_object(_: Arc<dyn crate::traits::Tool>) {}

    /// Compile-time check: WasmToolBridge satisfies Arc<dyn Tool>.
    #[allow(dead_code)]
    fn wasm_tool_bridge_is_tool() {
        fn _check(bridge: WasmToolBridge) {
            assert_tool_trait_object(Arc::new(bridge));
        }
    }
}
