// ---------------------------------------------------------------------------
// PyUnregisterFn + PyHookRegistry — wraps amplifier_core::HookRegistry
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

use crate::bridges::PyHookHandlerBridge;
use crate::helpers::{try_model_dump, wrap_future_as_coroutine};

// ---------------------------------------------------------------------------
// PyUnregisterFn — callable returned by PyHookRegistry.register()
// ---------------------------------------------------------------------------

/// Python-callable returned by `RustHookRegistry.register()`.
///
/// When called, removes the handler from the hook registry.
/// This matches the Python `HookRegistry.register()` contract which returns
/// a callable that unregisters the handler when invoked.
#[pyclass(name = "RustUnregisterFn")]
pub(crate) struct PyUnregisterFn {
    #[allow(clippy::type_complexity)]
    unregister_fns: Arc<std::sync::Mutex<HashMap<String, Box<dyn Fn() + Send + Sync>>>>,
    name: String,
}

#[pymethods]
impl PyUnregisterFn {
    fn __call__(&self) -> PyResult<()> {
        if let Ok(mut fns) = self.unregister_fns.lock() {
            if let Some(unreg) = fns.remove(&self.name) {
                unreg();
            }
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!("<unregister '{}'>", self.name)
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
pub(crate) struct PyHookRegistry {
    pub(crate) inner: Arc<amplifier_core::HookRegistry>,
    /// Stored unregister closures keyed by handler name.
    #[allow(clippy::type_complexity)]
    unregister_fns: Arc<std::sync::Mutex<HashMap<String, Box<dyn Fn() + Send + Sync>>>>,
}

#[pymethods]
impl PyHookRegistry {
    /// Create a new empty hook registry.
    #[new]
    pub(crate) fn new() -> Self {
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
    /// Register a hook handler.
    ///
    /// Matches Python `HookRegistry.register(event, handler, priority=0, name=None)`.
    /// The handler and name argument order matches the Python API so that
    /// module code like `registry.register(event, handler, name="my-hook")` works.
    #[pyo3(signature = (event, handler, priority = 0, name = None))]
    fn register(
        &self,
        py: Python<'_>,
        event: &str,
        handler: Py<PyAny>,
        priority: i32,
        name: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        let handler_name =
            name.unwrap_or_else(|| format!("_auto_{event}_{}", uuid::Uuid::new_v4()));
        let bridge = Arc::new(PyHookHandlerBridge { callable: handler });
        let unregister_fn =
            self.inner
                .register(event, bridge, priority, Some(handler_name.clone()));

        self.unregister_fns
            .lock()
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Lock poisoned: {e}")))?
            .insert(handler_name.clone(), unregister_fn);

        // Return a callable that unregisters this handler when invoked.
        // Matches the Python HookRegistry.register() contract.
        let callable = Py::new(
            py,
            PyUnregisterFn {
                unregister_fns: self.unregister_fns.clone(),
                name: handler_name,
            },
        )?;
        Ok(callable.into_any())
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
        let serializable = try_model_dump(&data);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}")))?;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let result = inner.emit(&event, value).await;
                // Convert HookResult to a JSON string, then parse it back as a
                // Python HookResult object so callers can access .action, .data, etc.
                let result_json = serde_json::to_string(&result).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize hook result to JSON (using empty object): {e}");
                    "{}".to_string()
                });
                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let dict = json_mod.call_method1("loads", (&result_json,))?;
                    // Create a proper HookResult from the dict
                    let models = py.import("amplifier_core.models")?;
                    let hook_result_cls = models.getattr("HookResult")?;
                    let obj = hook_result_cls.call_method1("model_validate", (&dict,))?;
                    Ok(obj.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        "Failed to attach to Python runtime",
                    )
                })?
            }),
        )
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
                let json_str: String = json_mod.call_method1("dumps", (dict,))?.extract()?;
                serde_json::from_str(&json_str)
                    .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}")))?
            }
            None => serde_json::json!({}),
        };
        self.inner.set_default_fields(value);
        Ok(())
    }

    /// Alias for `register()` -- backward compatibility with Python HookRegistry.
    #[pyo3(signature = (event, handler, priority = 0, name = None))]
    fn on(
        &self,
        py: Python<'_>,
        event: &str,
        handler: Py<PyAny>,
        priority: i32,
        name: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        self.register(py, event, handler, priority, name)
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
        let serializable = try_model_dump(&data);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}")))?;
        let timeout_dur = std::time::Duration::from_secs_f64(timeout);

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let results = inner.emit_and_collect(&event, value, timeout_dur).await;
                // Convert each HashMap<String, Value> to a JSON string.
                // Returns Vec<String> which becomes a Python list of strings.
                let json_strings: Vec<String> = results
                    .iter()
                    .map(|r| serde_json::to_string(r).unwrap_or_else(|e| {
                        log::warn!("Failed to serialize emit_and_collect result to JSON (using empty object): {e}");
                        "{}".to_string()
                    }))
                    .collect();
                Ok(json_strings)
            }),
        )
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
