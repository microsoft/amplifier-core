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

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

use amplifier_core::errors::HookError;
use amplifier_core::models::HookResult;
use amplifier_core::traits::HookHandler;

// ---------------------------------------------------------------------------
// PyHookHandlerBridge — wraps a Python callable as a Rust HookHandler
// ---------------------------------------------------------------------------

/// Bridges a Python callable into the Rust [`HookHandler`] trait.
///
/// Stores a `Py<PyAny>` (the Python callable) and calls it via the GIL
/// when `handle()` is invoked. The callable should accept `(event, data)`
/// and return a dict (or None for a default continue result).
struct PyHookHandlerBridge {
    callable: Py<PyAny>,
}

// Safety: Py<PyAny> is Send+Sync (PyO3 handles GIL acquisition).
unsafe impl Send for PyHookHandlerBridge {}
unsafe impl Sync for PyHookHandlerBridge {}

impl HookHandler for PyHookHandlerBridge {
    fn handle(
        &self,
        event: &str,
        data: Value,
    ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
        let event = event.to_string();
        let data_str = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());

        Box::pin(async move {
            // Acquire GIL to call the Python callable.
            // Python::try_attach is the PyO3 0.28 way to get the GIL.
            let result = Python::try_attach(|py| -> PyResult<HookResult> {
                let json_mod = py.import("json")?;
                let py_data = json_mod.call_method1("loads", (&data_str,))?;

                let result = self.callable.call(py, (&event, py_data), None)?;

                // If the callable returns None, treat as continue
                if result.is_none(py) {
                    return Ok(HookResult::default());
                }

                // For any non-None return, default to continue
                // TODO(milestone-6): Parse dict result into full HookResult
                Ok(HookResult::default())
            });

            match result {
                Some(Ok(hook_result)) => Ok(hook_result),
                Some(Err(py_err)) => Err(HookError::Other {
                    message: format!("Python hook handler error: {py_err}"),
                }),
                None => {
                    // No Python interpreter attached — return default
                    Ok(HookResult::default())
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// PySession — wraps amplifier_core::Session
// ---------------------------------------------------------------------------

/// Python-visible session wrapper.
///
/// Exposes the Rust `Session` lifecycle to Python consumers.
/// Uses `tokio::sync::Mutex` so the lock can be held across `.await` points
/// (required because `Session::execute` and `Session::cleanup` are async).
#[pyclass(name = "RustSession")]
struct PySession {
    inner: Arc<tokio::sync::Mutex<amplifier_core::Session>>,
}

#[pymethods]
impl PySession {
    /// Create a new session from a Python config dict.
    ///
    /// The dict must contain `session.orchestrator` and `session.context`.
    #[new]
    #[pyo3(signature = (config))]
    fn new(config: &Bound<'_, PyDict>) -> PyResult<Self> {
        // Convert Python dict to serde_json::Value via JSON round-trip
        let json_mod = config.py().import("json")?;
        let json_str: String = json_mod
            .call_method1("dumps", (config,))?
            .extract()?;

        let value: Value = serde_json::from_str(&json_str).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Invalid config JSON: {e}"))
        })?;

        let session_config =
            amplifier_core::SessionConfig::from_value(value).map_err(|e| {
                PyErr::new::<PyRuntimeError, _>(format!("Invalid session config: {e}"))
            })?;

        let session = amplifier_core::Session::new(session_config, None, None);

        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(session)),
        })
    }

    /// The session ID (UUID string).
    #[getter]
    fn session_id(&self) -> PyResult<String> {
        let session = self.inner.blocking_lock();
        Ok(session.session_id().to_string())
    }

    /// The parent session ID, if any.
    #[getter]
    fn parent_id(&self) -> PyResult<Option<String>> {
        let session = self.inner.blocking_lock();
        Ok(session.parent_id().map(|s| s.to_string()))
    }

    /// Whether the session has been initialized.
    #[getter]
    fn initialized(&self) -> PyResult<bool> {
        let session = self.inner.blocking_lock();
        Ok(session.is_initialized())
    }

    /// Initialize the session (marks it ready for execution).
    ///
    /// In the Rust kernel, module loading is external (done by the Python
    /// bridge). This method marks the session as initialized after modules
    /// have been mounted.
    fn initialize<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut session = inner.lock().await;
            session.set_initialized();
            Ok(())
        })
    }

    /// Execute a prompt through the orchestrator.
    ///
    /// The session must be initialized first. Returns the orchestrator's
    /// response string.
    fn execute<'py>(
        &self,
        py: Python<'py>,
        prompt: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut session = inner.lock().await;
            let result = session.execute(&prompt).await.map_err(|e| {
                PyErr::new::<PyRuntimeError, _>(e.to_string())
            })?;
            Ok(result)
        })
    }

    /// Clean up session resources.
    ///
    /// Emits `session:end` event and runs cleanup functions.
    fn cleanup<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let session = inner.lock().await;
            session.cleanup().await;
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// PyHookRegistry — wraps amplifier_core::HookRegistry
// ---------------------------------------------------------------------------

/// Python-visible hook registry wrapper.
///
/// Provides `register`, `emit`, and `unregister` methods for Python consumers
/// to participate in the Rust hook dispatch pipeline.
#[pyclass(name = "RustHookRegistry")]
struct PyHookRegistry {
    inner: Arc<amplifier_core::HookRegistry>,
    /// Stored unregister closures keyed by handler name.
    #[allow(clippy::type_complexity)]
    unregister_fns: Arc<std::sync::Mutex<HashMap<String, Box<dyn Fn() + Send + Sync>>>>,
}

#[pymethods]
impl PyHookRegistry {
    /// Create a new empty hook registry.
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(amplifier_core::HookRegistry::new()),
            unregister_fns: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Register a Python callable as a hook handler.
    ///
    /// # Arguments
    ///
    /// * `event` — Event name to hook (e.g., `"tool:pre"`).
    /// * `name` — Handler name (used for unregister).
    /// * `handler` — Python callable `(event: str, data: dict) -> dict | None`.
    /// * `priority` — Execution priority (lower = earlier). Default: 100.
    #[pyo3(signature = (event, name, handler, priority = 100))]
    fn register(
        &self,
        event: &str,
        name: &str,
        handler: Py<PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        let bridge = Arc::new(PyHookHandlerBridge { callable: handler });
        let unregister_fn = self.inner.register(
            event,
            bridge,
            priority,
            Some(name.to_string()),
        );

        self.unregister_fns
            .lock()
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Lock poisoned: {e}")))?
            .insert(name.to_string(), unregister_fn);

        Ok(())
    }

    /// Emit an event and return the aggregated result as a JSON string.
    ///
    /// Calls all registered handlers for the event in priority order.
    fn emit<'py>(
        &self,
        py: Python<'py>,
        event: String,
        data: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        // Convert Python data to serde_json::Value
        let json_mod = py.import("json")?;
        let json_str: String = json_mod
            .call_method1("dumps", (&data,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}"))
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = inner.emit(&event, value).await;
            // Convert HookResult to a simple JSON dict representation
            let result_json = serde_json::json!({
                "action": format!("{:?}", result.action).to_lowercase(),
                "data": result.data,
            });
            let result_str = serde_json::to_string(&result_json).unwrap_or_default();
            Ok(result_str)
        })
    }

    /// Unregister a handler by name.
    fn unregister(&self, name: &str) -> PyResult<()> {
        let mut fns = self
            .unregister_fns
            .lock()
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Lock poisoned: {e}")))?;

        if let Some(unreg) = fns.remove(name) {
            unreg();
        }
        Ok(())
    }

    /// Set default fields merged into every emit() call.
    ///
    /// Accepts keyword arguments matching the Python `set_default_fields(**kwargs)`.
    /// Internally converts to a serde_json::Value and delegates to the Rust registry.
    #[pyo3(signature = (**kwargs))]
    fn set_default_fields(&self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let value = match kwargs {
            Some(dict) => {
                let json_mod = dict.py().import("json")?;
                let json_str: String = json_mod
                    .call_method1("dumps", (dict,))?
                    .extract()?;
                serde_json::from_str(&json_str).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}"))
                })?
            }
            None => serde_json::json!({}),
        };
        self.inner.set_default_fields(value);
        Ok(())
    }

    /// Alias for `register()` -- backward compatibility with Python HookRegistry.
    #[pyo3(signature = (event, name, handler, priority = 100))]
    fn on(
        &self,
        event: &str,
        name: &str,
        handler: Py<PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        self.register(event, name, handler, priority)
    }

    /// List registered handlers, optionally filtered by event.
    ///
    /// Returns dict of event names to lists of handler names.
    /// Matches Python `HookRegistry.list_handlers(event=None)`.
    #[pyo3(signature = (event = None))]
    fn list_handlers(&self, event: Option<&str>) -> PyResult<HashMap<String, Vec<String>>> {
        Ok(self.inner.list_handlers(event))
    }

    /// Emit event and collect data from all handler responses.
    ///
    /// Unlike emit() which processes action semantics (deny short-circuits, etc.),
    /// this method simply collects result.data from all handlers for aggregation.
    ///
    /// Returns a list of JSON strings, each representing a handler's result.data.
    /// The Python switchover shim (Milestone 4) will parse these into dicts.
    ///
    /// Matches Python `HookRegistry.emit_and_collect(event, data, timeout=1.0)`.
    #[pyo3(signature = (event, data, timeout = 1.0))]
    fn emit_and_collect<'py>(
        &self,
        py: Python<'py>,
        event: String,
        data: Bound<'py, PyAny>,
        timeout: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let json_mod = py.import("json")?;
        let json_str: String = json_mod
            .call_method1("dumps", (&data,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}"))
        })?;
        let timeout_dur = std::time::Duration::from_secs_f64(timeout);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let results = inner.emit_and_collect(&event, value, timeout_dur).await;
            // Convert each HashMap<String, Value> to a JSON string.
            // Returns Vec<String> which becomes a Python list of strings.
            let json_strings: Vec<String> = results
                .iter()
                .map(|r| serde_json::to_string(r).unwrap_or_else(|_| "{}".to_string()))
                .collect();
            Ok(json_strings)
        })
    }

    // Class-level event name constants matching Python HookRegistry
    #[classattr]
    const SESSION_START: &'static str = "session:start";
    #[classattr]
    const SESSION_END: &'static str = "session:end";
    #[classattr]
    const PROMPT_SUBMIT: &'static str = "prompt:submit";
    #[classattr]
    const TOOL_PRE: &'static str = "tool:pre";
    #[classattr]
    const TOOL_POST: &'static str = "tool:post";
    #[classattr]
    const CONTEXT_PRE_COMPACT: &'static str = "context:pre_compact";
    #[classattr]
    const ORCHESTRATOR_COMPLETE: &'static str = "orchestrator:complete";
    #[classattr]
    const USER_NOTIFICATION: &'static str = "user:notification";
}

// ---------------------------------------------------------------------------
// PyCancellationToken — wraps amplifier_core::CancellationToken
// ---------------------------------------------------------------------------

/// Python-visible cancellation token wrapper.
///
/// Provides cooperative cancellation for Python consumers.
#[pyclass(name = "RustCancellationToken")]
struct PyCancellationToken {
    inner: amplifier_core::CancellationToken,
}

#[pymethods]
impl PyCancellationToken {
    /// Create a new cancellation token in the `None` state.
    #[new]
    fn new() -> Self {
        Self {
            inner: amplifier_core::CancellationToken::new(),
        }
    }

    /// Request graceful cancellation (waits for current tools to complete).
    fn request_cancellation(&self) {
        self.inner.request_graceful();
    }

    /// Whether any cancellation has been requested.
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// Current cancellation state as a string (`"none"`, `"graceful"`, `"immediate"`).
    #[getter]
    fn state(&self) -> String {
        format!("{:?}", self.inner.state()).to_lowercase()
    }
}

// ---------------------------------------------------------------------------
// PyCoordinator — wraps amplifier_core::Coordinator
// ---------------------------------------------------------------------------

/// Python-visible coordinator wrapper.
///
/// Provides access to the hook registry, cancellation token, and config.
#[pyclass(name = "RustCoordinator")]
struct PyCoordinator {
    inner: Arc<amplifier_core::Coordinator>,
}

#[pymethods]
impl PyCoordinator {
    /// Create a new coordinator with default (empty) config.
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(amplifier_core::Coordinator::new(HashMap::new())),
        }
    }

    /// Access the hook registry.
    ///
    /// Note: Returns a standalone registry. The coordinator's internal
    /// registry is not yet shared via Arc (planned for milestone 6).
    #[getter]
    fn hooks(&self) -> PyHookRegistry {
        // We can't extract the inner HookRegistry from Coordinator (it's owned),
        // so we create a new one. In practice, the Python layer uses its own
        // registry or accesses hooks through the session.
        // TODO(milestone-6): Share the coordinator's registry via Arc.
        PyHookRegistry::new()
    }

    /// Access the cancellation token.
    ///
    /// Note: Returns a standalone token. The coordinator's internal
    /// token is not yet shared (planned for milestone 6).
    #[getter]
    fn cancellation(&self) -> PyCancellationToken {
        // Same limitation as hooks — create a standalone token.
        // TODO(milestone-6): Share the coordinator's token.
        PyCancellationToken::new()
    }

    /// Session configuration as a Python dict.
    #[getter]
    fn config<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let config = self.inner.config();
        let json_str = serde_json::to_string(config).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Config serialization error: {e}"))
        })?;

        let json_mod = py.import("json")?;
        let result = json_mod.call_method1("loads", (&json_str,))?;
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// The compiled Rust extension module.
/// Python imports this as `amplifier_core._engine`.
#[pymodule]
fn _engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", "1.0.0")?;
    m.add("RUST_AVAILABLE", true)?;
    m.add_class::<PySession>()?;
    m.add_class::<PyHookRegistry>()?;
    m.add_class::<PyCancellationToken>()?;
    m.add_class::<PyCoordinator>()?;
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
        let _: fn() -> PySession = || {
            panic!("just checking type exists")
        };
    }

    /// Verify PyHookRegistry type exists and is constructable.
    #[test]
    fn py_hook_registry_type_exists() {
        let _: fn() -> PyHookRegistry = || {
            panic!("just checking type exists")
        };
    }

    /// Verify PyCancellationToken type exists and is constructable.
    #[test]
    fn py_cancellation_token_type_exists() {
        let _: fn() -> PyCancellationToken = || {
            panic!("just checking type exists")
        };
    }

    /// Verify PyCoordinator type exists and is constructable.
    #[test]
    fn py_coordinator_type_exists() {
        let _: fn() -> PyCoordinator = || {
            panic!("just checking type exists")
        };
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
}
