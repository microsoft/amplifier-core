// ---------------------------------------------------------------------------
// PyUnregisterFn + PyHookRegistry — wraps amplifier_core::HookRegistry
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value;

use crate::bridges::PyHookHandlerBridge;
use crate::helpers::{json_dumps_safe, try_model_dump, wrap_future_as_coroutine};

struct Registration {
    id: uuid::Uuid,
    unregister: Box<dyn Fn() + Send + Sync>,
}

type Registrations = Arc<std::sync::Mutex<HashMap<String, Vec<Registration>>>>;

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
    unregister_fns: Registrations,
    name: String,
    registration_id: uuid::Uuid,
}

#[pymethods]
impl PyUnregisterFn {
    fn __call__(&self) -> PyResult<()> {
        let registration = {
            let mut fns = self
                .unregister_fns
                .lock()
                .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Lock poisoned: {e}")))?;
            let mut owned = None;
            if let Some(registrations) = fns.get_mut(&self.name) {
                if let Some(index) = registrations
                    .iter()
                    .position(|r| r.id == self.registration_id)
                {
                    owned = Some(registrations.remove(index));
                }
                if registrations.is_empty() {
                    fns.remove(&self.name);
                }
            }
            owned
        };
        if let Some(registration) = registration {
            (registration.unregister)();
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
    /// Names are labels, not identities: retain each registration's own closure.
    unregister_fns: Registrations,
}

impl PyHookRegistry {
    /// Wrap the coordinator's registry so every transport shares dispatch.
    pub(crate) fn from_shared(inner: Arc<amplifier_core::HookRegistry>) -> Self {
        Self {
            inner,
            unregister_fns: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

#[pymethods]
impl PyHookRegistry {
    /// Create a new empty hook registry.
    #[new]
    pub(crate) fn new() -> Self {
        Self::from_shared(Arc::new(amplifier_core::HookRegistry::new()))
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
        // A native callback can re-enter the shared registry from a Tokio
        // blocking thread, where task locals are unavailable. Keep the
        // registration loop as a fallback only; do not snapshot contextvars,
        // because the current emitting task's context must take precedence.
        let fallback_locals = pyo3_async_runtimes::tokio::get_current_locals(py)
            .ok()
            .map(|locals| pyo3_async_runtimes::TaskLocals::new(locals.event_loop(py)));
        let bridge = Arc::new(PyHookHandlerBridge {
            callable: handler,
            fallback_locals,
        });
        let unregister_fn =
            self.inner
                .register(event, bridge, priority, Some(handler_name.clone()));

        let registration_id = uuid::Uuid::new_v4();
        self.unregister_fns
            .lock()
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Lock poisoned: {e}")))?
            .entry(handler_name.clone())
            .or_default()
            .push(Registration {
                id: registration_id,
                unregister: unregister_fn,
            });

        // Return a callable that unregisters this handler when invoked.
        // Matches the Python HookRegistry.register() contract.
        let callable = Py::new(
            py,
            PyUnregisterFn {
                unregister_fns: self.unregister_fns.clone(),
                name: handler_name,
                registration_id,
            },
        )?;
        Ok(callable.into_any())
    }

    /// Emit an event and return the aggregated result as a JSON string.
    ///
    /// Calls all registered handlers for the event in priority order.
    ///
    /// For the LLM call event family this also stamps the correlation id
    /// (`request_id`) so `llm:request` and its matching terminal event can be
    /// paired by identity instead of by position. See
    /// [`crate::correlation`] for the policy and its scoping rules.
    fn emit<'py>(
        &self,
        py: Python<'py>,
        event: String,
        data: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        // Convert Python data to serde_json::Value
        let serializable = try_model_dump(&data);
        let json_str: String = json_dumps_safe(py, &serializable)?;
        let value: Value = serde_json::from_str(&json_str)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}")))?;
        // Stamp the correlation id before handlers see the event. Must happen
        // here, on the caller's Python stack, because the in-flight call is
        // scoped by contextvars -- the spawned future below runs off-thread
        // and no longer has the emitting task's context.
        let value = crate::correlation::stamp_request_id(py, &event, value)?;

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

    /// Unregister the most recent remaining registration with this name.
    /// Returned callables still own their exact registration independently.
    fn unregister(&self, name: &str) -> PyResult<()> {
        let registration = {
            let mut fns = self
                .unregister_fns
                .lock()
                .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Lock poisoned: {e}")))?;
            let latest = fns.get_mut(name).and_then(Vec::pop);
            if fns.get(name).is_some_and(Vec::is_empty) {
                fns.remove(name);
            }
            latest
        };
        if let Some(registration) = registration {
            (registration.unregister)();
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
                let json_str = json_dumps_safe(dict.py(), dict.as_any())?;
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
    /// Returns a Python `list[dict]`, where each dict is the `result.data`
    /// from one handler response. Each `HashMap<String, Value>` is serialized
    /// to JSON and parsed back into a Python dict via `json.loads()`.
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
        let serializable = try_model_dump(&data);
        let json_str: String = json_dumps_safe(py, &serializable)?;
        let value: Value = serde_json::from_str(&json_str)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Invalid JSON: {e}")))?;
        let timeout_dur = std::time::Duration::from_secs_f64(timeout);

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let results = inner.emit_and_collect(&event, value, timeout_dur).await;
                // Convert each HashMap<String, Value> to a Python dict.
                // Serializes each result to a JSON string then json.loads() to dict.
                // Returns Py<PyAny> (a Python list of dicts).
                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let list = PyList::empty(py);
                    for r in &results {
                        let json_str = serde_json::to_string(r).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize emit_and_collect result to JSON (using empty object): {e}");
                            "{}".to_string()
                        });
                        let dict = json_mod.call_method1("loads", (&json_str,))?;
                        list.append(dict)?;
                    }
                    Ok(list.into_any().unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        "Failed to attach to Python runtime",
                    )
                })?
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
