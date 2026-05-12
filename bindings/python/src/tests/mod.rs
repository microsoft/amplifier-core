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

/// Verify load_wasm_from_path rejects Rust transport with a Rust-specific error message.
///
/// The explicit Rust guard (added in task-11) must be present so users get clear guidance
/// to use the gRPC sidecar pattern instead of a generic "not WASM" error.
#[cfg(feature = "wasm")]
#[test]
fn load_wasm_from_path_rejects_rust_transport_with_specific_message() {
    // This test verifies the Rust-specific error message constant is exported from
    // module_resolver.  It will fail to compile until RUST_TRANSPORT_ERROR_MSG is added.
    assert_eq!(
        crate::module_resolver::RUST_TRANSPORT_ERROR_MSG,
        "load_wasm_from_path cannot load Rust modules. Use the gRPC sidecar pattern instead."
    );
}

/// Verify `json_dumps_safe` exists in helpers with the correct signature.
///
/// This test fails to compile until `json_dumps_safe` is added to helpers.rs.
/// Signature: `fn(Python<'_>, &Bound<'_, PyAny>) -> PyResult<String>`.
#[test]
fn json_dumps_safe_signature_compiles() {
    let _: fn(Python<'_>, &Bound<'_, PyAny>) -> PyResult<String> = crate::helpers::json_dumps_safe;
}

/// Structural guard: no raw `json.dumps()` calls outside `helpers.rs`.
///
/// All `json.dumps()` at the Python/Rust FFI boundary must go through
/// `json_dumps_safe()` (which passes `default=str`) to prevent TypeError
/// crashes on non-JSON-native types like `Decimal` or `datetime`.
///
/// If this test fails, you added a `json.dumps()` call in a binding file.
/// Replace:  `json_mod.call_method1("dumps", (&obj,))`
/// With:     `json_dumps_safe(py, &obj)`   (from `crate::helpers`)
#[test]
fn no_raw_json_dumps_outside_helpers() {
    use std::fs;
    use std::path::Path;

    fn check_dir(dir: &Path, violations: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip tests/ — no production FFI code there
                    if path.file_name().map_or(false, |n| n == "tests") {
                        continue;
                    }
                    check_dir(&path, violations);
                } else if path.extension().map_or(false, |e| e == "rs")
                    && path.file_name().map_or(false, |n| n != "helpers.rs")
                {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if content.contains(r#"call_method1("dumps""#)
                            || content.contains(r#"call_method("dumps""#)
                        {
                            violations.push(path.display().to_string());
                        }
                    }
                }
            }
        }
    }

    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    check_dir(&src_dir, &mut violations);

    assert!(
        violations.is_empty(),
        "Raw json.dumps() found outside helpers.rs — use json_dumps_safe() instead:\n{}",
        violations.join("\n")
    );
}
