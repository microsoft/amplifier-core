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

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

mod bridges;
mod cancellation;
mod coordinator;
mod errors;
mod helpers;
mod hooks;
mod module_resolver;
mod retry;
mod session;
mod wasm;

// ---------------------------------------------------------------------------
// Explicit re-exports (no wildcard re-exports)
// ---------------------------------------------------------------------------

pub(crate) use cancellation::PyCancellationToken;
pub(crate) use coordinator::PyCoordinator;
pub(crate) use errors::PyProviderError;
pub(crate) use hooks::{PyHookRegistry, PyUnregisterFn};
pub(crate) use module_resolver::{load_wasm_from_path, resolve_module};
pub(crate) use retry::{classify_error_message, compute_delay, PyRetryConfig};
pub(crate) use session::PySession;
pub(crate) use wasm::{
    load_and_mount_wasm, PyWasmApproval, PyWasmContext, PyWasmHook, PyWasmOrchestrator,
    PyWasmProvider, PyWasmTool,
};

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// The compiled Rust extension module.
/// Python imports this as `amplifier_core._engine`.
#[pymodule]
fn _engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
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
mod tests;
