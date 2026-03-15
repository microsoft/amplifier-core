// ---------------------------------------------------------------------------
// PySession — wraps amplifier_core::Session
// ---------------------------------------------------------------------------

use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

use crate::helpers::wrap_future_as_coroutine;
use crate::hooks::PyHookRegistry;

// ---------------------------------------------------------------------------
// PySession — wraps amplifier_core::Session (Milestone 3)
// ---------------------------------------------------------------------------

/// Python-visible session wrapper.
///
/// Hybrid approach: the Session creates and owns a `PyCoordinator` internally.
/// `initialize()` delegates to a Python helper (`_session_init.py`) that calls
/// the Python loader to load modules from config.
/// `execute(prompt)` delegates to a Python helper (`_session_exec.py`) that
/// calls the orchestrator.
/// `cleanup()` runs the coordinator's cleanup functions.
///
/// Matches the Python `AmplifierSession` constructor signature:
/// ```python
/// __init__(self, config, loader=None, session_id=None, parent_id=None,
///          approval_system=None, display_system=None, is_resumed=False)
/// ```
#[pyclass(name = "RustSession")]
pub(crate) struct PySession {
    /// Rust kernel session (for session_id, parent_id, initialized flag).
    inner: Arc<tokio::sync::Mutex<amplifier_core::Session>>,
    /// The PyCoordinator instance owned by this session.
    coordinator: Py<PyAny>,
    /// Original config dict (Python dict).
    config: Py<PyDict>,
    /// Whether this is a resumed session.
    is_resumed: bool,
    /// Cached session_id (avoids locking inner for every access).
    cached_session_id: String,
    /// Cached parent_id.
    cached_parent_id: Option<String>,
}

#[pymethods]
impl PySession {
    /// Create a new session matching the Python AmplifierSession constructor.
    ///
    /// The dict must contain `session.orchestrator` and `session.context`.
    #[allow(clippy::too_many_arguments)]
    #[new]
    #[pyo3(signature = (config, loader=None, session_id=None, parent_id=None, approval_system=None, display_system=None, is_resumed=false))]
    fn new(
        py: Python<'_>,
        config: &Bound<'_, PyDict>,
        #[allow(unused_variables)] loader: Option<Bound<'_, PyAny>>,
        session_id: Option<String>,
        parent_id: Option<String>,
        approval_system: Option<Bound<'_, PyAny>>,
        display_system: Option<Bound<'_, PyAny>>,
        is_resumed: bool,
    ) -> PyResult<Self> {
        // ---- Validate config (matching Python AmplifierSession.__init__) ----
        // Python: if not config: raise ValueError("Configuration is required")
        if config.is_empty() {
            return Err(PyErr::new::<PyValueError, _>("Configuration is required"));
        }

        // Python: if not config.get("session", {}).get("orchestrator"):
        let session_section = config.get_item("session")?;
        let (has_orchestrator, has_context) = match &session_section {
            Some(s) => {
                let s_dict = s.cast::<PyDict>()?;
                let orch = s_dict.get_item("orchestrator")?;
                let ctx = s_dict.get_item("context")?;
                (
                    orch.is_some_and(|o| !o.is_none()),
                    ctx.is_some_and(|c| !c.is_none()),
                )
            }
            None => (false, false),
        };

        if !has_orchestrator {
            return Err(PyErr::new::<PyValueError, _>(
                "Configuration must specify session.orchestrator",
            ));
        }
        if !has_context {
            return Err(PyErr::new::<PyValueError, _>(
                "Configuration must specify session.context",
            ));
        }

        // ---- Build Rust kernel Session ----
        let json_mod = py.import("json")?;
        let json_str: String = json_mod.call_method1("dumps", (config,))?.extract()?;
        let value: Value = serde_json::from_str(&json_str)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Invalid config JSON: {e}")))?;
        let session_config = amplifier_core::SessionConfig::from_value(value)
            .map_err(|e| PyErr::new::<PyValueError, _>(format!("Invalid session config: {e}")))?;

        let session = if is_resumed {
            let sid = session_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            amplifier_core::Session::new_resumed(session_config, sid, parent_id.clone())
        } else {
            amplifier_core::Session::new(session_config, session_id.clone(), parent_id.clone())
        };

        let actual_session_id = session.session_id().to_string();
        let actual_parent_id = session.parent_id().map(|s| s.to_string());

        // ---- Create a "fake session" Python object for coordinator construction ----
        // The PyCoordinator::new() expects a Python object with .session_id,
        // .parent_id, .config attributes. We create a simple namespace object.
        let types_mod = py.import("types")?;
        let ns_cls = types_mod.getattr("SimpleNamespace")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("session_id", &actual_session_id)?;
        kwargs.set_item("parent_id", actual_parent_id.as_deref())?;
        kwargs.set_item("config", config)?;
        let fake_session = ns_cls.call((), Some(&kwargs))?;

        // ---- Create the coordinator ----
        // RustCoordinator now has process_hook_result() and cleanup()
        // with fatal-exception logic built in — no Python wrapper needed.
        let coord_any: Py<PyAny> = {
            let engine = py.import("amplifier_core._engine")?;
            let coord_cls = engine.getattr("RustCoordinator")?;
            let coord_kwargs = PyDict::new(py);
            coord_kwargs.set_item("session", fake_session.clone())?;
            if let Some(ref approval) = approval_system {
                coord_kwargs.set_item("approval_system", approval)?;
            }
            if let Some(ref display) = display_system {
                coord_kwargs.set_item("display_system", display)?;
            }
            let coord = coord_cls.call((), Some(&coord_kwargs))?;
            coord.unbind()
        };

        // ---- Set default fields on the hook registry ----
        // Python: self.coordinator.hooks.set_default_fields(session_id=..., parent_id=...)
        {
            let coord_bound = coord_any.bind(py);
            let hooks = coord_bound.getattr("hooks")?;
            let defaults_dict = PyDict::new(py);
            defaults_dict.set_item("session_id", &actual_session_id)?;
            defaults_dict.set_item("parent_id", actual_parent_id.as_deref())?;
            hooks.call_method("set_default_fields", (), Some(&defaults_dict))?;
        }

        // ---- Patch the coordinator's session back-reference to point to
        //      the *real* PySession once it's constructed. We'll do this
        //      via a post-construction step below using the SimpleNamespace
        //      placeholder for now. The coordinator.session will be the
        //      SimpleNamespace, but coordinator.session_id is correct. ----

        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(session)),
            coordinator: coord_any,
            config: config.clone().unbind(),
            is_resumed,
            cached_session_id: actual_session_id,
            cached_parent_id: actual_parent_id,
        })
    }

    // -----------------------------------------------------------------------
    // Task 3.1: session_id, parent_id (cached — no lock needed)
    // -----------------------------------------------------------------------

    /// The session ID (UUID string).
    #[getter]
    fn session_id(&self) -> &str {
        &self.cached_session_id
    }

    /// The parent session ID, if any.
    #[getter]
    fn parent_id<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        match &self.cached_parent_id {
            Some(pid) => pid.into_pyobject(py).unwrap().into_any().unbind(),
            None => py.None(),
        }
    }

    // -----------------------------------------------------------------------
    // Task 3.2: coordinator, config, is_resumed properties
    // -----------------------------------------------------------------------

    /// The coordinator owned by this session.
    #[getter]
    fn coordinator<'py>(&self, py: Python<'py>) -> Bound<'py, PyAny> {
        self.coordinator.bind(py).clone()
    }

    /// The original config dict.
    #[getter]
    fn config<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        self.config.bind(py).clone()
    }

    /// Whether this is a resumed session.
    #[getter]
    fn is_resumed(&self) -> bool {
        self.is_resumed
    }

    /// Whether the session has been initialized.
    #[getter]
    fn initialized(&self) -> PyResult<bool> {
        let session = self.inner.blocking_lock();
        Ok(session.is_initialized())
    }

    // -----------------------------------------------------------------------
    // Task 3.3 / Task 8: initialize() — Rust owns the control flow
    // -----------------------------------------------------------------------

    /// Initialize the session by loading modules from config.
    ///
    /// Rust controls the lifecycle:
    /// 1. Idempotency guard (already initialized → no-op)
    /// 2. Patches the coordinator's `session_ref` to point to the real
    ///    `RustSession` object (replacing the `SimpleNamespace` placeholder
    ///    created in `new()` — necessary because `self` doesn't exist yet
    ///    during `__new__`).
    /// 3. Delegates module loading to `_session_init.initialize_session()`
    ///    via `into_future` (Python handles loader, importlib, module resolution)
    /// 4. Sets the Rust `initialized` flag on success
    ///
    /// Errors from module loading propagate; `initialized` stays `false`.
    fn initialize<'py>(
        slf: &Bound<'py, PySession>,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Step 1: Idempotency — if already initialized, return resolved future
        {
            let this = slf.borrow();
            let session = this.inner.blocking_lock();
            if session.is_initialized() {
                return wrap_future_as_coroutine(
                    py,
                    pyo3_async_runtimes::tokio::future_into_py(py, async { Ok(()) }),
                );
            }
        }

        // Step 2: Extract what we need before entering the async block
        let (coro_py, inner) = {
            let this = slf.borrow();
            let helper = py.import("amplifier_core._session_init")?;
            let init_fn = helper.getattr("initialize_session")?;
            let coro = init_fn.call1((
                this.config.bind(py),
                this.coordinator.bind(py),
                this.cached_session_id.as_str(),
                this.cached_parent_id.as_deref(),
            ))?;
            // Convert to an owned Py<PyAny> so it's 'static + Send
            let coro_py: Py<PyAny> = coro.unbind();
            let inner = this.inner.clone();
            (coro_py, inner)
        };

        // Step 3: Patch the coordinator's session back-reference to point to
        //         the real PySession, replacing the SimpleNamespace placeholder
        //         created in new().  Must happen while we have the GIL and a
        //         Python reference to `slf` (before future_into_py).
        {
            let coord = slf.borrow().coordinator.clone_ref(py);
            coord
                .bind(py)
                .call_method1("_set_session", (slf.as_any(),))?;
        }

        // Step 4: Return an awaitable that runs init then sets the flag
        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Convert the Python coroutine to a Rust future (needs GIL + task locals)
                let future = Python::try_attach(|py| {
                    pyo3_async_runtimes::tokio::into_future(coro_py.into_bound(py))
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
                .map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!(
                        "Failed to convert init coroutine: {e}"
                    ))
                })?;

                // Await the Python module loading (outside GIL)
                future.await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Session initialization failed: {e}"))
                })?;

                // Step 5: Mark session as initialized in Rust kernel
                {
                    let session = inner.lock().await;
                    session.set_initialized();
                }

                Ok(())
            }),
        )
    }

    // -----------------------------------------------------------------------
    // Task 9: execute(prompt) — Rust owns the control flow
    // -----------------------------------------------------------------------

    /// Execute a prompt through the mounted orchestrator.
    ///
    /// Rust controls the lifecycle:
    /// 1. Checks initialization flag (error if not initialized)
    /// 2. Emits pre-execution events (session:start or session:resume)
    ///    with optional `raw` field when session.raw=true
    /// 3. Delegates orchestrator call to `_session_exec.run_orchestrator()`
    ///    via `into_future` (Python handles mount point access + kwargs)
    /// 4. Checks cancellation after execution
    /// 5. Emits cancel:completed event if cancelled
    /// 6. Returns the result string
    fn execute<'py>(&self, py: Python<'py>, prompt: String) -> PyResult<Bound<'py, PyAny>> {
        // Step 1: Check initialized — fail fast before any async work
        {
            let session = self.inner.blocking_lock();
            if !session.is_initialized() {
                return Err(PyErr::new::<PyRuntimeError, _>(
                    "Session not initialized. Call initialize() first.",
                ));
            }
        }

        // Step 2: Prepare the Python orchestrator coroutine (we have the GIL here)
        let helper = py.import("amplifier_core._session_exec")?;
        let run_fn = helper.getattr("run_orchestrator")?;
        let raw_fn = helper.getattr("emit_raw_field_if_configured")?;

        // Prepare the orchestrator call coroutine
        let orch_coro = run_fn.call1((self.coordinator.bind(py), &prompt))?;
        let orch_coro_py: Py<PyAny> = orch_coro.unbind();

        // Determine event name based on is_resumed
        let event_base = if self.is_resumed {
            "session:resume"
        } else {
            "session:start"
        };

        // Prepare raw-field emission coroutine (no-op if session.raw=false)
        let raw_coro = raw_fn.call1((
            self.coordinator.bind(py),
            self.config.bind(py),
            &self.cached_session_id,
            event_base,
        ))?;
        let debug_coro_py: Py<PyAny> = raw_coro.unbind();

        // Get the inner HookRegistry for direct Rust emit (avoids PyO3 Future/coroutine mismatch:
        // calling a #[pymethods] fn that uses future_into_py returns a Future object, but
        // into_future() expects a native Python coroutine — they are different awaitables).
        let hooks_inner: Arc<amplifier_core::HookRegistry> = {
            let coord = self.coordinator.bind(py);
            let hooks = coord.getattr("hooks")?;
            let hook_registry = hooks.extract::<PyRef<PyHookRegistry>>()?;
            hook_registry.inner.clone()
        };
        let pre_event_data = serde_json::json!({
            "session_id": self.cached_session_id,
            "parent_id": self.cached_parent_id,
        });

        // Clone references for the async block
        let coordinator = self.coordinator.clone_ref(py);

        // Step 3: Return an awaitable that runs the full execute sequence
        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // 3a: Emit pre-execution event (session:start or session:resume)
                // Call inner Rust emit directly — avoids the Future/coroutine mismatch that
                // occurs when going through the Python PyO3 bridge (future_into_py returns
                // a Future object, but into_future() expects a native coroutine).
                hooks_inner.emit(event_base, pre_event_data).await;

                // 3b: Emit debug events (delegates to Python for redact_secrets/truncate_values)
                let debug_future = Python::try_attach(|py| {
                    pyo3_async_runtimes::tokio::into_future(debug_coro_py.into_bound(py))
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
                .map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!(
                        "Failed to convert debug event coroutine: {e}"
                    ))
                })?;

                debug_future.await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Debug event emission failed: {e}"))
                })?;

                // 3c: Call the Python orchestrator (mount point access + orchestrator.execute())
                let orch_future = Python::try_attach(|py| {
                    pyo3_async_runtimes::tokio::into_future(orch_coro_py.into_bound(py))
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
                .map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!(
                        "Failed to convert orchestrator coroutine: {e}"
                    ))
                })?;

                // Await orchestrator execution outside GIL
                let orch_result = orch_future.await;

                // 3d: Check cancellation and emit cancel:completed if needed
                let is_cancelled = Python::try_attach(|py| -> PyResult<bool> {
                    let coord = coordinator.bind(py);
                    let cancellation = coord.getattr("cancellation")?;
                    cancellation.getattr("is_cancelled")?.extract()
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
                .map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Failed to check cancellation: {e}"))
                })?;

                match orch_result {
                    Ok(py_result) => {
                        // Success path — check cancellation and emit event if needed
                        if is_cancelled {
                            // Get cancellation state and emit directly via Rust — avoids
                            // Future/coroutine mismatch when going through the Python bridge.
                            let cancel_data = Python::try_attach(|py| -> PyResult<_> {
                                let coord = coordinator.bind(py);
                                let cancellation = coord.getattr("cancellation")?;
                                let state: String = cancellation.getattr("state")?.extract()?;
                                Ok(serde_json::json!({ "was_immediate": state == "immediate" }))
                            })
                            .ok_or_else(|| {
                                PyErr::new::<PyRuntimeError, _>(
                                    "Failed to attach to Python runtime",
                                )
                            })??;

                            let _ = hooks_inner.emit("cancel:completed", cancel_data).await;
                            // Best-effort
                        }

                        // Extract the result string
                        let result_str: String = Python::try_attach(|py| -> PyResult<String> {
                            let bound = py_result.bind(py);
                            bound.extract()
                        })
                        .ok_or_else(|| {
                            PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                        })??;

                        Ok(result_str)
                    }
                    Err(e) => {
                        // Error path — check cancellation and emit event if needed
                        if is_cancelled {
                            let err_str = format!("{e}");
                            // Get cancellation state and emit directly via Rust — avoids
                            // Future/coroutine mismatch when going through the Python bridge.
                            let cancel_data = Python::try_attach(|py| -> PyResult<_> {
                                let coord = coordinator.bind(py);
                                let cancellation = coord.getattr("cancellation")?;
                                let state: String = cancellation.getattr("state")?.extract()?;
                                Ok(serde_json::json!({
                                    "was_immediate": state == "immediate",
                                    "error": err_str,
                                }))
                            })
                            .ok_or_else(|| {
                                PyErr::new::<PyRuntimeError, _>(
                                    "Failed to attach to Python runtime",
                                )
                            })??;

                            let _ = hooks_inner.emit("cancel:completed", cancel_data).await;
                            // Best-effort
                        }

                        Err(PyErr::new::<PyRuntimeError, _>(format!(
                            "Execution failed: {e}"
                        )))
                    }
                }
            }),
        )
    }

    // -----------------------------------------------------------------------
    // Task 10: cleanup() — Rust owns the full cleanup lifecycle
    // -----------------------------------------------------------------------

    /// Clean up session resources.
    ///
    /// Rust controls the full cleanup lifecycle:
    /// 1. Call all registered cleanup functions (reverse order, error-tolerant)
    /// 2. Emit `session:end` event via hooks
    /// 3. Reset the initialized flag
    ///
    /// Errors in cleanup functions and event emission are logged but never
    /// propagate — cleanup must always complete.
    fn cleanup<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        // Grab references we need inside the async block
        let session_id = self.cached_session_id.clone();

        // Extract inner HookRegistry for direct Rust emit in the async block
        // (avoids Future/coroutine mismatch when calling through the Python bridge).
        let hooks_inner_for_end: Arc<amplifier_core::HookRegistry> = {
            let coord = self.coordinator.bind(py);
            let hooks = coord.getattr("hooks")?;
            let hook_registry = hooks.extract::<PyRef<PyHookRegistry>>()?;
            hook_registry.inner.clone()
        };

        // Step 1: Collect cleanup functions while we still hold the GIL.
        // Also pre-check iscoroutinefunction so we know how to call each one.
        let coord = self.coordinator.bind(py);
        let cleanup_fns_list = coord.getattr("_cleanup_fns")?;
        let cleanup_len: usize = cleanup_fns_list.len()?;
        let inspect = py.import("inspect")?;

        // Snapshot callable references with their async-ness pre-determined.
        // This matches Python main's pattern of checking iscoroutinefunction
        // BEFORE calling, rather than calling first and checking the result.
        let mut cleanup_callables: Vec<(Py<PyAny>, bool)> = Vec::with_capacity(cleanup_len);
        for i in 0..cleanup_len {
            let item = cleanup_fns_list.get_item(i)?;
            // Guard: skip None and non-callable items (defense-in-depth)
            if item.is_none() || !item.is_callable() {
                continue;
            }
            let is_async: bool = inspect
                .call_method1("iscoroutinefunction", (&item,))?
                .extract()?;
            cleanup_callables.push((item.unbind(), is_async));
        }

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // ----------------------------------------------------------
                // Step 1: Call all cleanup functions in reverse order
                // Matches Python main's coordinator.cleanup() pattern:
                //   if callable(fn):
                //     if iscoroutinefunction(fn): await fn()
                //     else: result = fn(); if iscoroutine(result): await result
                // ----------------------------------------------------------
                for (callable, is_async) in cleanup_callables.iter().rev() {
                    if *is_async {
                        // Async cleanup: call to get coroutine, then await via into_future
                        let coro_result: Option<PyResult<Py<PyAny>>> =
                            Python::try_attach(|py| callable.call0(py));

                        if let Some(Ok(coro_py)) = coro_result {
                            let future_result = Python::try_attach(|py| {
                                pyo3_async_runtimes::tokio::into_future(coro_py.into_bound(py))
                            });
                            if let Some(Ok(future)) = future_result {
                                if let Err(e) = future.await {
                                    log::error!("Error during cleanup: {e}");
                                }
                            }
                        } else if let Some(Err(e)) = coro_result {
                            log::error!("Error during cleanup: {e}");
                        }
                    } else {
                        // Sync cleanup: call and check if result is a coroutine
                        let call_outcome: Option<PyResult<Option<Py<PyAny>>>> =
                            Python::try_attach(|py| -> PyResult<Option<Py<PyAny>>> {
                                let result = callable.call0(py)?;
                                let bound = result.bind(py);
                                let inspect = py.import("inspect")?;
                                let is_coro: bool =
                                    inspect.call_method1("iscoroutine", (bound,))?.extract()?;
                                if is_coro {
                                    Ok(Some(result))
                                } else {
                                    Ok(None) // Sync completed
                                }
                            });

                        match call_outcome {
                            Some(Ok(Some(coro_py))) => {
                                // Sync function returned a coroutine — await it
                                let future_result = Python::try_attach(|py| {
                                    pyo3_async_runtimes::tokio::into_future(coro_py.into_bound(py))
                                });
                                if let Some(Ok(future)) = future_result {
                                    if let Err(e) = future.await {
                                        log::error!("Error during cleanup: {e}");
                                    }
                                }
                            }
                            Some(Ok(None)) => {
                                // Sync call completed successfully
                            }
                            Some(Err(e)) => {
                                log::error!("Error during cleanup: {e}");
                            }
                            None => {
                                // Failed to attach to Python runtime — skip
                            }
                        }
                    }
                }

                // ----------------------------------------------------------
                // Step 2: Emit session:end event (best-effort)
                // Direct Rust emit — avoids Future/coroutine mismatch when going
                // through the Python PyO3 bridge (future_into_py returns a Future,
                // but into_future() expects a native coroutine).
                // ----------------------------------------------------------
                let end_data = serde_json::json!({ "session_id": session_id });
                hooks_inner_for_end.emit("session:end", end_data).await;

                // ----------------------------------------------------------
                // Step 3: Reset the initialized flag
                // ----------------------------------------------------------
                {
                    let session = inner.lock().await;
                    session.clear_initialized();
                }

                Ok(())
            }),
        )
    }

    // -----------------------------------------------------------------------
    // Task 3.6: async context manager support
    // -----------------------------------------------------------------------

    /// Async context manager entry: initializes the session and returns self.
    fn __aenter__<'py>(slf: Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        // Create a Python wrapper coroutine that initializes then returns self
        let helper = py.import("amplifier_core._session_init")?;
        let aenter_fn = helper.getattr("_session_aenter")?;
        let coro = aenter_fn.call1((&slf,))?;
        Ok(coro)
    }

    /// Async context manager exit: runs cleanup.
    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        _exc_type: &Bound<'py, PyAny>,
        _exc_val: &Bound<'py, PyAny>,
        _exc_tb: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.cleanup(py)
    }
}
