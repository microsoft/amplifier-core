//! PyO3 bridge for amplifier-core.
//!
//! This crate wraps the pure Rust kernel types and exposes them
//! as Python classes via PyO3. It compiles into the `_engine`
//! extension module that ships inside the `amplifier_core` Python package.
//!
//! # Exposed classes
//!
//! | Python name             | Rust wrapper         | Inner type                  |
//! |-------------------------|----------------------|-----------------------------|
//! | `RustSession`           | [`PySession`]        | `amplifier_core::Session`   |
//! | `RustHookRegistry`      | [`PyHookRegistry`]   | `amplifier_core::HookRegistry` |
//! | `RustCancellationToken` | [`PyCancellationToken`] | `amplifier_core::CancellationToken` |
//! | `RustCoordinator`       | [`PyCoordinator`]    | `amplifier_core::Coordinator` |

use pyo3::prelude::*;

mod helpers;
pub(crate) use helpers::*;

mod bridges;
pub(crate) use bridges::*;

mod cancellation;
pub(crate) use cancellation::*;

mod errors;
pub(crate) use errors::*;

mod retry;
pub(crate) use retry::*;

mod hooks;
pub(crate) use hooks::*;

mod session;
pub(crate) use session::*;

mod module_resolver;
pub(crate) use module_resolver::*;

mod coordinator;
pub(crate) use coordinator::*;

mod wasm;
pub(crate) use wasm::*;


// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// The compiled Rust extension module.
/// Python imports this as `amplifier_core._engine`.
#[pymodule]
fn _engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();
    m.add("__version__", "1.0.0")?;
    m.add("RUST_AVAILABLE", true)?;
    m.add_class::<PySession>()?;
    m.add_class::<PyUnregisterFn>()?;
    m.add_class::<PyHookRegistry>()?;
    m.add_class::<PyCancellationToken>()?;
    m.add_class::<PyCoordinator>()?;
    m.add_class::<PyProviderError>()?;
    m.add_class::<PyRetryConfig>()?;
    m.add_class::<PyWasmTool>()?;
    m.add_class::<PyWasmProvider>()?;
    m.add_class::<PyWasmHook>()?;
    m.add_class::<PyWasmContext>()?;
    m.add_class::<PyWasmOrchestrator>()?;
    m.add_class::<PyWasmApproval>()?;
    m.add_function(wrap_pyfunction!(classify_error_message, m)?)?;
    m.add_function(wrap_pyfunction!(compute_delay, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_module, m)?)?;
    m.add_function(wrap_pyfunction!(load_wasm_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(load_and_mount_wasm, m)?)?;

    // -----------------------------------------------------------------------
    // Event constants — expose all 41 canonical events from amplifier_core
    // -----------------------------------------------------------------------

    // Session lifecycle
    m.add("SESSION_START", amplifier_core::events::SESSION_START)?;
    m.add("SESSION_END", amplifier_core::events::SESSION_END)?;
    m.add("SESSION_FORK", amplifier_core::events::SESSION_FORK)?;
    m.add("SESSION_RESUME", amplifier_core::events::SESSION_RESUME)?;

    // Prompt lifecycle
    m.add("PROMPT_SUBMIT", amplifier_core::events::PROMPT_SUBMIT)?;
    m.add("PROMPT_COMPLETE", amplifier_core::events::PROMPT_COMPLETE)?;

    // Planning
    m.add("PLAN_START", amplifier_core::events::PLAN_START)?;
    m.add("PLAN_END", amplifier_core::events::PLAN_END)?;

    // Provider calls
    m.add("PROVIDER_REQUEST", amplifier_core::events::PROVIDER_REQUEST)?;
    m.add(
        "PROVIDER_RESPONSE",
        amplifier_core::events::PROVIDER_RESPONSE,
    )?;
    m.add("PROVIDER_RETRY", amplifier_core::events::PROVIDER_RETRY)?;
    m.add("PROVIDER_ERROR", amplifier_core::events::PROVIDER_ERROR)?;
    m.add(
        "PROVIDER_THROTTLE",
        amplifier_core::events::PROVIDER_THROTTLE,
    )?;
    m.add(
        "PROVIDER_TOOL_SEQUENCE_REPAIRED",
        amplifier_core::events::PROVIDER_TOOL_SEQUENCE_REPAIRED,
    )?;
    m.add("PROVIDER_RESOLVE", amplifier_core::events::PROVIDER_RESOLVE)?;

    // LLM events
    m.add("LLM_REQUEST", amplifier_core::events::LLM_REQUEST)?;
    m.add("LLM_RESPONSE", amplifier_core::events::LLM_RESPONSE)?;

    // Content block events
    m.add(
        "CONTENT_BLOCK_START",
        amplifier_core::events::CONTENT_BLOCK_START,
    )?;
    m.add(
        "CONTENT_BLOCK_DELTA",
        amplifier_core::events::CONTENT_BLOCK_DELTA,
    )?;
    m.add(
        "CONTENT_BLOCK_END",
        amplifier_core::events::CONTENT_BLOCK_END,
    )?;

    // Thinking events
    m.add("THINKING_DELTA", amplifier_core::events::THINKING_DELTA)?;
    m.add("THINKING_FINAL", amplifier_core::events::THINKING_FINAL)?;

    // Tool invocations
    m.add("TOOL_PRE", amplifier_core::events::TOOL_PRE)?;
    m.add("TOOL_POST", amplifier_core::events::TOOL_POST)?;
    m.add("TOOL_ERROR", amplifier_core::events::TOOL_ERROR)?;

    // Context management
    m.add(
        "CONTEXT_PRE_COMPACT",
        amplifier_core::events::CONTEXT_PRE_COMPACT,
    )?;
    m.add(
        "CONTEXT_POST_COMPACT",
        amplifier_core::events::CONTEXT_POST_COMPACT,
    )?;
    m.add(
        "CONTEXT_COMPACTION",
        amplifier_core::events::CONTEXT_COMPACTION,
    )?;
    m.add("CONTEXT_INCLUDE", amplifier_core::events::CONTEXT_INCLUDE)?;

    // Orchestrator lifecycle
    m.add(
        "ORCHESTRATOR_COMPLETE",
        amplifier_core::events::ORCHESTRATOR_COMPLETE,
    )?;
    m.add("EXECUTION_START", amplifier_core::events::EXECUTION_START)?;
    m.add("EXECUTION_END", amplifier_core::events::EXECUTION_END)?;

    // User notifications
    m.add(
        "USER_NOTIFICATION",
        amplifier_core::events::USER_NOTIFICATION,
    )?;

    // Artifacts
    m.add("ARTIFACT_WRITE", amplifier_core::events::ARTIFACT_WRITE)?;
    m.add("ARTIFACT_READ", amplifier_core::events::ARTIFACT_READ)?;

    // Policy / approvals
    m.add("POLICY_VIOLATION", amplifier_core::events::POLICY_VIOLATION)?;
    m.add(
        "APPROVAL_REQUIRED",
        amplifier_core::events::APPROVAL_REQUIRED,
    )?;
    m.add("APPROVAL_GRANTED", amplifier_core::events::APPROVAL_GRANTED)?;
    m.add("APPROVAL_DENIED", amplifier_core::events::APPROVAL_DENIED)?;

    // Cancellation lifecycle
    m.add("CANCEL_REQUESTED", amplifier_core::events::CANCEL_REQUESTED)?;
    m.add("CANCEL_COMPLETED", amplifier_core::events::CANCEL_COMPLETED)?;

    // Aggregate list of all events
    m.add("ALL_EVENTS", amplifier_core::events::ALL_EVENTS.to_vec())?;

    // -----------------------------------------------------------------------
    // Capabilities — expose all 16 well-known capability constants
    // -----------------------------------------------------------------------

    // Capabilities — Tier 1 (core)
    m.add("TOOLS", amplifier_core::capabilities::TOOLS)?;
    m.add("STREAMING", amplifier_core::capabilities::STREAMING)?;
    m.add("THINKING", amplifier_core::capabilities::THINKING)?;
    m.add("VISION", amplifier_core::capabilities::VISION)?;
    m.add("JSON_MODE", amplifier_core::capabilities::JSON_MODE)?;
    // Capabilities — Tier 2 (extended)
    m.add("FAST", amplifier_core::capabilities::FAST)?;
    m.add(
        "CODE_EXECUTION",
        amplifier_core::capabilities::CODE_EXECUTION,
    )?;
    m.add("WEB_SEARCH", amplifier_core::capabilities::WEB_SEARCH)?;
    m.add("DEEP_RESEARCH", amplifier_core::capabilities::DEEP_RESEARCH)?;
    m.add("LOCAL", amplifier_core::capabilities::LOCAL)?;
    m.add("AUDIO", amplifier_core::capabilities::AUDIO)?;
    m.add(
        "IMAGE_GENERATION",
        amplifier_core::capabilities::IMAGE_GENERATION,
    )?;
    m.add("COMPUTER_USE", amplifier_core::capabilities::COMPUTER_USE)?;
    m.add("EMBEDDINGS", amplifier_core::capabilities::EMBEDDINGS)?;
    m.add("LONG_CONTEXT", amplifier_core::capabilities::LONG_CONTEXT)?;
    m.add("BATCH", amplifier_core::capabilities::BATCH)?;

    // Collections
    m.add(
        "ALL_WELL_KNOWN_CAPABILITIES",
        amplifier_core::capabilities::ALL_WELL_KNOWN_CAPABILITIES.to_vec(),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
    ///
    /// PyWasmTool bridges WASM-loaded Rust trait objects into Python mount_points.
    /// Without this wrapper, WASM modules load into throwaway coordinators and are
    /// never visible to the Python session.
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

    /// Document the contract for load_and_mount_wasm:
    ///
    /// - Accepts a PyCoordinator reference and a filesystem path
    /// - Resolves the module manifest (auto-detects module type via amplifier.toml or .wasm inspection)
    /// - Loads the WASM module via WasmEngine
    /// - For tool modules: wraps in PyWasmTool and mounts into coordinator.mount_points["tools"]
    /// - For other types: returns module info for Python-side mounting
    /// - Returns a status dict with "status", "module_type", and optional "name" keys
    ///
    /// The actual function requires the Python GIL; this test documents the contract
    /// and verifies the function compiles. Integration tests (Task 2) verify end-to-end.
    #[test]
    fn load_and_mount_wasm_contract() {
        // Verify the function exists as a callable with the expected signature.
        // It's a #[pyfunction] so we can't call it without the GIL, but we can
        // verify the symbol compiles.
        let _exists =
            load_and_mount_wasm as fn(Python<'_>, &PyCoordinator, String) -> PyResult<Py<PyDict>>;
    }
}
