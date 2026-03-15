use super::*;
use pyo3::types::PyDict;

/// Verify PySession type exists and is constructable.
#[test]
fn py_session_type_exists() {
    let _: fn() -> PySession = || panic!("just checking type exists");
}

/// Verify PyHookRegistry type exists and is constructable.
#[test]
fn py_hook_registry_type_exists() {
    let _: fn() -> PyHookRegistry = || panic!("just checking type exists");
}

/// Verify PyCancellationToken type exists and is constructable.
#[test]
fn py_cancellation_token_type_exists() {
    let _: fn() -> PyCancellationToken = || panic!("just checking type exists");
}

/// Verify PyCoordinator type name exists (no longer constructable without Python GIL).
#[test]
fn py_coordinator_type_exists() {
    // PyCoordinator now requires a Python session object in its constructor,
    // so we can only verify the type compiles.
    fn _assert_type_compiles(_: &PyCoordinator) {}
}

/// Verify CancellationToken can be created and used without Python.
#[test]
fn cancellation_token_works_standalone() {
    let token = amplifier_core::CancellationToken::new();
    assert!(!token.is_cancelled());
    token.request_graceful();
    assert!(token.is_cancelled());
    assert!(token.is_graceful());
}

/// Verify HookRegistry can be created without Python.
#[test]
fn hook_registry_works_standalone() {
    let registry = amplifier_core::HookRegistry::new();
    let handlers = registry.list_handlers(None);
    assert!(handlers.is_empty());
}

/// Verify Session can be created without Python.
#[test]
fn session_works_standalone() {
    let config = amplifier_core::SessionConfig::minimal("loop-basic", "context-simple");
    let session = amplifier_core::Session::new(config, None, None);
    assert!(!session.session_id().is_empty());
    assert!(!session.is_initialized());
}

/// Verify that `log` and `pyo3-log` crates are available in the bindings crate.
/// The `log` macros should compile, and `pyo3_log::init` should be a callable function.
#[test]
fn log_and_pyo3_log_available() {
    // log macros compile (no-ops without a logger installed)
    log::info!("test log from bindings crate");
    // pyo3_log::init exists as a function — returns ResetHandle
    let _: fn() -> pyo3_log::ResetHandle = pyo3_log::init;
}

/// Verify PyWasmTool wrapper type exists and can hold an Arc<dyn Tool>.
#[test]
fn py_wasm_tool_type_exists() {
    fn _assert_type_compiles(_: &PyWasmTool) {}
}

/// Verify PyWasmHook wrapper type exists.
#[test]
fn py_wasm_hook_type_exists() {
    fn _assert_type_compiles(_: &PyWasmHook) {}
}

/// Verify PyWasmContext wrapper type exists.
#[test]
fn py_wasm_context_type_exists() {
    fn _assert_type_compiles(_: &PyWasmContext) {}
}

/// Verify PyWasmOrchestrator wrapper type exists.
#[test]
fn py_wasm_orchestrator_type_exists() {
    fn _assert_type_compiles(_: &PyWasmOrchestrator) {}
}

/// Verify PyWasmApproval wrapper type exists.
#[test]
fn py_wasm_approval_type_exists() {
    fn _assert_type_compiles(_: &PyWasmApproval) {}
}

/// Verify load_and_mount_wasm function signature compiles.
/// The actual function requires the Python GIL; integration tests verify end-to-end.
#[test]
fn load_and_mount_wasm_contract() {
    let _exists =
        load_and_mount_wasm as fn(Python<'_>, &PyCoordinator, String) -> PyResult<Py<PyDict>>;
}
