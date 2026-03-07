//! WASM bridge for sandboxed approval provider modules (Component Model).
//!
//! [`WasmApprovalBridge`] loads a WASM Component via wasmtime and implements the
//! [`ApprovalProvider`] trait, enabling sandboxed in-process approval decisions. The guest
//! exports `request-approval` (accepts JSON-serialized `ApprovalRequest` as bytes,
//! returns JSON-serialized `ApprovalResponse` bytes).
//!
//! Gated behind the `wasm` feature flag.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use wasmtime::component::Component;
use wasmtime::{Engine, Store};

use crate::errors::{AmplifierError, SessionError};
use crate::models::{ApprovalRequest, ApprovalResponse};
use crate::traits::ApprovalProvider;

use super::wasm_tool::{create_linker_and_store, WasmState};

/// The WIT interface name used by `cargo component` for approval provider exports.
const INTERFACE_NAME: &str = "amplifier:modules/approval-provider@1.0.0";

/// Convenience alias for the wasmtime typed function handle: takes (bytes) → result(bytes, string).
type RequestApprovalFunc = wasmtime::component::TypedFunc<(Vec<u8>,), (Result<Vec<u8>, String>,)>;

/// Shorthand for the fallible return type used by helper functions.
type WasmResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Look up the `request-approval` typed function export from a component instance.
///
/// Tries:
/// 1. Direct root-level export `"request-approval"`
/// 2. Nested inside the [`INTERFACE_NAME`] exported instance
fn get_request_approval_func(
    instance: &wasmtime::component::Instance,
    store: &mut Store<WasmState>,
) -> WasmResult<RequestApprovalFunc> {
    // Try direct root-level export first.
    if let Ok(f) = instance
        .get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(&mut *store, "request-approval")
    {
        return Ok(f);
    }

    // Try nested inside the interface-exported instance.
    let iface_idx = instance
        .get_export_index(&mut *store, None, INTERFACE_NAME)
        .ok_or_else(|| format!("export instance '{INTERFACE_NAME}' not found"))?;
    let func_idx = instance
        .get_export_index(&mut *store, Some(&iface_idx), "request-approval")
        .ok_or_else(|| {
            format!("export function 'request-approval' not found in '{INTERFACE_NAME}'")
        })?;
    let func = instance
        .get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(&mut *store, &func_idx)
        .map_err(|e| format!("typed func lookup failed for 'request-approval': {e}"))?;
    Ok(func)
}

/// Helper: call the `request-approval` export on a fresh component instance.
///
/// The request bytes must be a JSON-serialized `ApprovalRequest`.
fn call_request_approval(
    engine: &Engine,
    component: &Component,
    request_bytes: Vec<u8>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let (linker, mut store) = create_linker_and_store(engine)?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = get_request_approval_func(&instance, &mut store)?;
    let (result,) = func.call(&mut store, (request_bytes,))?;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(err.into()),
    }
}

/// A bridge that loads a WASM Component and exposes it as a native [`ApprovalProvider`].
///
/// The component is compiled once and can be instantiated for each approval request.
/// `request-approval` is called per invocation inside a `spawn_blocking` task
/// (wasmtime is synchronous). Each call gets a fresh WASM instance — the bridge is stateless.
pub struct WasmApprovalBridge {
    engine: Arc<Engine>,
    component: Component,
}

impl WasmApprovalBridge {
    /// Load a WASM approval component from raw bytes.
    ///
    /// Compiles the Component and caches it for reuse across `request_approval()` calls.
    pub fn from_bytes(
        wasm_bytes: &[u8],
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let component = Component::new(&engine, wasm_bytes)?;
        Ok(Self { engine, component })
    }

    /// Convenience: load a WASM approval component from a file path.
    pub fn from_file(
        path: &Path,
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes, engine)
    }
}

impl ApprovalProvider for WasmApprovalBridge {
    fn request_approval(
        &self,
        request: ApprovalRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ApprovalResponse, AmplifierError>> + Send + '_>> {
        Box::pin(async move {
            // Serialize the ApprovalRequest as JSON bytes for the WASM guest.
            let request_bytes = serde_json::to_vec(&request).map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("WASM approval: failed to serialize ApprovalRequest: {e}"),
                })
            })?;

            let engine = Arc::clone(&self.engine);
            let component = self.component.clone(); // Component is Arc-backed, cheap clone

            let result_bytes = tokio::task::spawn_blocking(move || {
                call_request_approval(&engine, &component, request_bytes)
            })
            .await
            .map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("WASM approval execution task panicked: {e}"),
                })
            })?
            .map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("WASM request-approval failed: {e}"),
                })
            })?;

            let approval_response: ApprovalResponse = serde_json::from_slice(&result_bytes)
                .map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!(
                            "WASM approval: failed to deserialize ApprovalResponse: {e}"
                        ),
                    })
                })?;

            Ok(approval_response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Compile-time check: WasmApprovalBridge satisfies Arc<dyn ApprovalProvider>.
    ///
    /// Note: the integration test in `tests/wasm_approval_e2e.rs` would have an equivalent
    /// check from the *public* API surface. This one catches breakage during unit-test
    /// runs without needing the integration test.
    #[allow(dead_code)]
    fn _assert_wasm_approval_bridge_is_approval_provider(bridge: WasmApprovalBridge) {
        let _: Arc<dyn crate::traits::ApprovalProvider> = Arc::new(bridge);
    }

    /// Helper: read the auto-approve.wasm fixture bytes.
    ///
    /// The fixture lives at the workspace root under `tests/fixtures/wasm/`.
    /// CARGO_MANIFEST_DIR points to `amplifier-core/crates/amplifier-core`,
    /// so we walk up to the workspace root first.
    fn auto_approve_wasm_bytes() -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // Two candidates because the workspace root may be at different depths
        // depending on how the repo is checked out:
        //   - 3 levels up: used as a git submodule (super-repo/amplifier-core/crates/amplifier-core)
        //   - 2 levels up: standalone checkout (amplifier-core/crates/amplifier-core)
        let candidates = [
            manifest.join("../../../tests/fixtures/wasm/auto-approve.wasm"),
            manifest.join("../../tests/fixtures/wasm/auto-approve.wasm"),
        ];
        for p in &candidates {
            if p.exists() {
                return std::fs::read(p)
                    .unwrap_or_else(|e| panic!("Failed to read auto-approve.wasm at {p:?}: {e}"));
            }
        }
        panic!(
            "auto-approve.wasm not found. Tried: {:?}",
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

    /// E2E test: auto-approve.wasm always returns approved=true with a reason.
    #[tokio::test]
    async fn auto_approve_returns_approved_with_reason() {
        let engine = make_engine();
        let bytes = auto_approve_wasm_bytes();
        let bridge =
            WasmApprovalBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        let request = ApprovalRequest {
            tool_name: "test-tool".to_string(),
            action: "delete all files".to_string(),
            details: Default::default(),
            risk_level: "high".to_string(),
            timeout: None,
        };

        let response = bridge.request_approval(request).await;
        let response = response.expect("request_approval should succeed");

        assert!(
            response.approved,
            "expected approved=true from auto-approve fixture"
        );
        assert!(
            response.reason.is_some(),
            "expected a reason from auto-approve fixture, got None"
        );
    }
}
