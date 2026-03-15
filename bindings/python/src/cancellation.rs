// ---------------------------------------------------------------------------
// PyCancellationToken — wraps amplifier_core::CancellationToken
// ---------------------------------------------------------------------------

use std::collections::HashSet;
use std::sync::Arc;

use pyo3::prelude::*;

use crate::helpers::wrap_future_as_coroutine;

/// Python-visible cancellation token wrapper.
///
/// Provides cooperative cancellation for Python consumers.
#[pyclass(name = "RustCancellationToken")]
pub(crate) struct PyCancellationToken {
    pub(crate) inner: amplifier_core::CancellationToken,
    /// Python-side cancel callbacks (stored separately from Rust inner
    /// because `trigger_callbacks` must run within pyo3 task-local context,
    /// not inside `tokio::task::spawn` which loses those locals).
    py_callbacks: Arc<std::sync::Mutex<Vec<Py<PyAny>>>>,
}

#[pymethods]
impl PyCancellationToken {
    /// Create a new cancellation token in the `None` state.
    #[new]
    pub(crate) fn new() -> Self {
        Self {
            inner: amplifier_core::CancellationToken::new(),
            py_callbacks: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Request graceful cancellation (waits for current tools to complete).
    fn request_cancellation(&self) {
        self.inner.request_graceful();
    }

    /// Whether any cancellation has been requested.
    #[getter]
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// Current cancellation state as a string (`"none"`, `"graceful"`, `"immediate"`).
    #[getter]
    fn state(&self) -> String {
        format!("{:?}", self.inner.state()).to_lowercase()
    }

    // -- New properties (Milestone: complete CancellationToken bindings) --

    /// Whether graceful cancellation has been requested.
    #[getter]
    fn is_graceful(&self) -> bool {
        self.inner.is_graceful()
    }

    /// Whether immediate cancellation has been requested.
    #[getter]
    fn is_immediate(&self) -> bool {
        self.inner.is_immediate()
    }

    /// Currently running tool call IDs (snapshot).
    #[getter]
    fn running_tools(&self) -> HashSet<String> {
        self.inner.running_tools()
    }

    /// Names of currently running tools (for display).
    #[getter]
    fn running_tool_names(&self) -> Vec<String> {
        self.inner.running_tool_names()
    }

    // -- New methods --

    /// Request graceful cancellation. Returns true if state changed.
    fn request_graceful(&self) -> bool {
        self.inner.request_graceful()
    }

    /// Request immediate cancellation. Returns true if state changed.
    fn request_immediate(&self) -> bool {
        self.inner.request_immediate()
    }

    /// Reset cancellation state. Called when starting a new turn.
    fn reset(&self) {
        self.inner.reset()
    }

    /// Register a tool as starting execution.
    fn register_tool_start(&self, tool_call_id: &str, tool_name: &str) {
        self.inner.register_tool_start(tool_call_id, tool_name)
    }

    /// Register a tool as completed.
    fn register_tool_complete(&self, tool_call_id: &str) {
        self.inner.register_tool_complete(tool_call_id)
    }

    /// Register a child token for cancellation propagation.
    fn register_child(&self, child: &PyCancellationToken) {
        self.inner.register_child(child.inner.clone())
    }

    /// Unregister a child token.
    fn unregister_child(&self, child: &PyCancellationToken) {
        self.inner.unregister_child(&child.inner)
    }

    /// Register a Python callable as a cancellation callback.
    ///
    /// The callable should be an async function `() -> None` (or a sync
    /// function — both are supported). It will be called when
    /// `trigger_callbacks()` is invoked.
    fn on_cancel(&self, callback: Py<PyAny>) {
        self.py_callbacks.lock().unwrap().push(callback);
    }

    /// Trigger all registered cancellation callbacks.
    ///
    /// Async method — returns an awaitable. Errors in individual callbacks
    /// are logged but do not prevent subsequent callbacks from executing.
    ///
    /// NOTE: We drive Python callbacks directly here (rather than delegating
    /// to `self.inner.trigger_callbacks()`) because the Rust inner method
    /// uses `tokio::task::spawn` which creates a new task that lacks the
    /// pyo3-async-runtimes task locals needed by `into_future`.
    fn trigger_callbacks<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let callbacks = self.py_callbacks.clone();
        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Snapshot callbacks (clone refs under GIL, then release lock)
                let cbs: Vec<Py<PyAny>> = {
                    let guard = callbacks.lock().unwrap();
                    Python::try_attach(|py| {
                        Ok::<_, PyErr>(guard.iter().map(|cb| cb.clone_ref(py)).collect())
                    })
                    .unwrap_or(Ok(Vec::new()))
                    .unwrap_or_default()
                };

                for cb in cbs {
                    // Call the callback and check if it returns a coroutine
                    let call_result: Option<PyResult<(bool, Py<PyAny>)>> =
                        Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
                            let result = cb.call0(py)?;
                            let bound = result.bind(py);
                            let inspect = py.import("inspect")?;
                            let is_coro: bool =
                                inspect.call_method1("iscoroutine", (bound,))?.extract()?;
                            Ok((is_coro, result))
                        });

                    match call_result {
                        Some(Ok((true, coro_py))) => {
                            // Await the coroutine via into_future (task locals available here)
                            let future_result = Python::try_attach(|py| {
                                pyo3_async_runtimes::tokio::into_future(coro_py.into_bound(py))
                            });
                            if let Some(Ok(future)) = future_result {
                                let _ = future.await; // Best-effort; errors logged not propagated
                            }
                        }
                        Some(Ok((false, _))) => {
                            // Sync callback completed successfully
                        }
                        Some(Err(e)) => {
                            log::error!("Error in cancellation callback: {e}");
                        }
                        None => {
                            // Failed to attach to Python runtime — skip
                        }
                    }
                }
                Ok(())
            }),
        )
    }
}
