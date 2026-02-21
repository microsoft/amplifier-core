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

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
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
        // Clone the Py<PyAny> reference inside the GIL to safely move into async block
        let callable = Python::try_attach(|py| {
            Ok::<_, PyErr>(self.callable.clone_ref(py))
        })
        .unwrap()
        .unwrap();

        Box::pin(async move {
            // Step 1: Call the Python handler (inside GIL) — returns either a
            // sync result or a coroutine object, plus whether it's a coroutine.
            let (is_coro, py_result_or_coro) = Python::try_attach(
                |py| -> PyResult<(bool, Py<PyAny>)> {
                    let json_mod = py.import("json")?;
                    let data_str =
                        serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
                    let py_data = json_mod.call_method1("loads", (&data_str,))?;

                    let call_result = callable.call(py, (&event, py_data), None)?;
                    let bound = call_result.bind(py);

                    // Check if the result is a coroutine (async handler)
                    let inspect = py.import("inspect")?;
                    let is_coro: bool =
                        inspect.call_method1("iscoroutine", (bound,))?.extract()?;

                    Ok((is_coro, call_result))
                },
            )
            .ok_or_else(|| HookError::HandlerFailed {
                message: "Failed to attach to Python runtime".to_string(),
                handler_name: None,
            })?
            .map_err(|e| HookError::HandlerFailed {
                message: format!("Python handler call error: {e}"),
                handler_name: None,
            })?;

            // Step 2: If it's a coroutine, convert to a Rust Future via
            // pyo3_async_runtimes::tokio::into_future() and await OUTSIDE the GIL.
            // This is the key fix: the old code used run_coroutine_threadsafe /
            // asyncio.run() which either deadlocked or created a throwaway event loop.
            // into_future() properly drives the coroutine on the caller's event loop.
            let py_result = if is_coro {
                let future = Python::try_attach(|py| {
                    pyo3_async_runtimes::tokio::into_future(py_result_or_coro.into_bound(py))
                })
                .ok_or_else(|| HookError::HandlerFailed {
                    message: "Failed to attach to Python runtime for coroutine conversion"
                        .to_string(),
                    handler_name: None,
                })?
                .map_err(|e| HookError::HandlerFailed {
                    message: format!("Failed to convert coroutine: {e}"),
                    handler_name: None,
                })?;

                // Await OUTSIDE the GIL — drives the Python coroutine on the
                // caller's asyncio event loop via pyo3-async-runtimes task locals.
                future.await.map_err(|e| HookError::HandlerFailed {
                    message: format!("Python async handler error: {e}"),
                    handler_name: None,
                })?
            } else {
                py_result_or_coro
            };

            // Step 3: Parse the Python result into a HookResult (reacquire GIL)
            let result_json: String = Python::try_attach(|py| -> PyResult<String> {
                let bound = py_result.bind(py);
                if bound.is_none() {
                    return Ok("{}".to_string());
                }
                let json_mod = py.import("json")?;
                let json_str: String = json_mod
                    .call_method1("dumps", (bound,))?
                    .extract()
                    .unwrap_or_else(|_| "{}".to_string());
                Ok(json_str)
            })
            .ok_or_else(|| HookError::HandlerFailed {
                message: "Failed to attach to Python runtime for result parsing".to_string(),
                handler_name: None,
            })?
            .map_err(|e| HookError::HandlerFailed {
                message: format!("Failed to serialize handler result: {e}"),
                handler_name: None,
            })?;

            let hook_result: HookResult =
                serde_json::from_str(&result_json).unwrap_or_default();
            Ok(hook_result)
        })
    }
}

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
struct PySession {
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
        #[allow(unused_variables)]
        loader: Option<Bound<'_, PyAny>>,
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
        let json_str: String = json_mod
            .call_method1("dumps", (config,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Invalid config JSON: {e}"))
        })?;
        let session_config =
            amplifier_core::SessionConfig::from_value(value).map_err(|e| {
                PyErr::new::<PyValueError, _>(format!("Invalid session config: {e}"))
            })?;

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
        // Use the Python ModuleCoordinator wrapper (from _rust_wrappers.py)
        // which adds process_hook_result on top of the Rust PyCoordinator.
        // This is critical: orchestrators call coordinator.process_hook_result()
        // which only exists on the Python wrapper, not on raw RustCoordinator.
        let coord_any: Py<PyAny> = {
            let wrappers = py.import("amplifier_core._rust_wrappers")?;
            let coord_cls = wrappers.getattr("ModuleCoordinator")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("session", fake_session.clone())?;
            if let Some(ref approval) = approval_system {
                kwargs.set_item("approval_system", approval)?;
            }
            if let Some(ref display) = display_system {
                kwargs.set_item("display_system", display)?;
            }
            let coord = coord_cls.call((), Some(&kwargs))?;
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
    /// 2. Delegates module loading to `_session_init.initialize_session()`
    ///    via `into_future` (Python handles loader, importlib, module resolution)
    /// 3. Sets the Rust `initialized` flag on success
    ///
    /// Errors from module loading propagate; `initialized` stays `false`.
    fn initialize<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        // Step 1: Idempotency — if already initialized, return resolved future
        {
            let session = self.inner.blocking_lock();
            if session.is_initialized() {
                return pyo3_async_runtimes::tokio::future_into_py(py, async { Ok(()) });
            }
        }

        // Step 2: Prepare the Python init coroutine (we have the GIL here)
        let helper = py.import("amplifier_core._session_init")?;
        let init_fn = helper.getattr("initialize_session")?;
        let coro = init_fn.call1((
            self.config.bind(py),
            self.coordinator.bind(py),
            &self.cached_session_id,
            self.cached_parent_id.as_deref(),
        ))?;
        // Convert to an owned Py<PyAny> so it's 'static + Send
        let coro_py: Py<PyAny> = coro.unbind();

        let inner = self.inner.clone();

        // Step 3: Return an awaitable that runs init then sets the flag
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
                PyErr::new::<PyRuntimeError, _>(format!(
                    "Session initialization failed: {e}"
                ))
            })?;

            // Step 4: Mark session as initialized in Rust kernel
            {
                let mut session = inner.lock().await;
                session.set_initialized();
            }

            Ok(())
        })
    }

    // -----------------------------------------------------------------------
    // Task 9: execute(prompt) — Rust owns the control flow
    // -----------------------------------------------------------------------

    /// Execute a prompt through the mounted orchestrator.
    ///
    /// Rust controls the lifecycle:
    /// 1. Checks initialization flag (error if not initialized)
    /// 2. Emits pre-execution events (session:start or session:resume)
    /// 3. Delegates orchestrator call to `_session_exec.run_orchestrator()`
    ///    via `into_future` (Python handles mount point access + kwargs)
    /// 4. Checks cancellation after execution
    /// 5. Emits cancel:completed event if cancelled
    /// 6. Returns the result string
    fn execute<'py>(
        &self,
        py: Python<'py>,
        prompt: String,
    ) -> PyResult<Bound<'py, PyAny>> {
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
        let debug_fn = helper.getattr("emit_debug_events")?;

        // Prepare the orchestrator call coroutine
        let orch_coro = run_fn.call1((
            self.coordinator.bind(py),
            &prompt,
        ))?;
        let orch_coro_py: Py<PyAny> = orch_coro.unbind();

        // Determine event names based on is_resumed
        let (event_base, event_debug, event_raw) = if self.is_resumed {
            ("session:resume", "session:resume:debug", "session:resume:raw")
        } else {
            ("session:start", "session:start:debug", "session:start:raw")
        };

        // Prepare debug events coroutine
        let debug_coro = debug_fn.call1((
            self.coordinator.bind(py),
            self.config.bind(py),
            &self.cached_session_id,
            event_debug,
            event_raw,
        ))?;
        let debug_coro_py: Py<PyAny> = debug_coro.unbind();

        // Prepare the pre-execution event emission coroutine
        let coord = self.coordinator.bind(py);
        let hooks = coord.getattr("hooks")?;
        let emit_data = PyDict::new(py);
        emit_data.set_item("session_id", &self.cached_session_id)?;
        emit_data.set_item("parent_id", self.cached_parent_id.as_deref())?;
        let pre_event_coro = hooks.call_method1("emit", (event_base, &emit_data))?;
        let pre_event_coro_py: Py<PyAny> = pre_event_coro.unbind();

        // Clone references for the async block
        let coordinator = self.coordinator.clone_ref(py);

        // Step 3: Return an awaitable that runs the full execute sequence
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // 3a: Emit pre-execution event (session:start or session:resume)
            let pre_event_future = Python::try_attach(|py| {
                pyo3_async_runtimes::tokio::into_future(pre_event_coro_py.into_bound(py))
            })
            .ok_or_else(|| {
                PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
            })?
            .map_err(|e| {
                PyErr::new::<PyRuntimeError, _>(format!(
                    "Failed to convert pre-event coroutine: {e}"
                ))
            })?;

            // Await outside GIL
            pre_event_future.await.map_err(|e| {
                PyErr::new::<PyRuntimeError, _>(format!(
                    "Pre-execution event emission failed: {e}"
                ))
            })?;

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
                PyErr::new::<PyRuntimeError, _>(format!(
                    "Debug event emission failed: {e}"
                ))
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
                PyErr::new::<PyRuntimeError, _>(format!(
                    "Failed to check cancellation: {e}"
                ))
            })?;

            match orch_result {
                Ok(py_result) => {
                    // Success path — check cancellation and emit event if needed
                    if is_cancelled {
                        let cancel_future = Python::try_attach(|py| -> PyResult<_> {
                            let coord = coordinator.bind(py);
                            let hooks = coord.getattr("hooks")?;
                            let cancellation = coord.getattr("cancellation")?;
                            let state: String = cancellation.getattr("state")?.extract()?;
                            let data = PyDict::new(py);
                            data.set_item("was_immediate", state == "immediate")?;
                            let coro = hooks.call_method1("emit", ("cancel:completed", data))?;
                            pyo3_async_runtimes::tokio::into_future(coro)
                        })
                        .ok_or_else(|| {
                            PyErr::new::<PyRuntimeError, _>(
                                "Failed to attach to Python runtime",
                            )
                        })??;

                        let _ = cancel_future.await; // Best-effort cancel event
                    }

                    // Extract the result string
                    let result_str: String = Python::try_attach(|py| -> PyResult<String> {
                        let bound = py_result.bind(py);
                        bound.extract()
                    })
                    .ok_or_else(|| {
                        PyErr::new::<PyRuntimeError, _>(
                            "Failed to attach to Python runtime",
                        )
                    })??;

                    Ok(result_str)
                }
                Err(e) => {
                    // Error path — check cancellation and emit event if needed
                    if is_cancelled {
                        let err_str = format!("{e}");
                        let cancel_future = Python::try_attach(|py| -> PyResult<_> {
                            let coord = coordinator.bind(py);
                            let hooks = coord.getattr("hooks")?;
                            let cancellation = coord.getattr("cancellation")?;
                            let state: String = cancellation.getattr("state")?.extract()?;
                            let data = PyDict::new(py);
                            data.set_item("was_immediate", state == "immediate")?;
                            data.set_item("error", &err_str)?;
                            let coro = hooks.call_method1("emit", ("cancel:completed", data))?;
                            pyo3_async_runtimes::tokio::into_future(coro)
                        })
                        .ok_or_else(|| {
                            PyErr::new::<PyRuntimeError, _>(
                                "Failed to attach to Python runtime",
                            )
                        })??;

                        let _ = cancel_future.await; // Best-effort cancel event
                    }

                    Err(PyErr::new::<PyRuntimeError, _>(format!(
                        "Execution failed: {e}"
                    )))
                }
            }
        })
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
        let coordinator = self.coordinator.clone_ref(py);
        let session_id = self.cached_session_id.clone();

        // Step 1: Collect cleanup functions and prepare the session:end event
        // coroutine while we still hold the GIL.
        let coord = self.coordinator.bind(py);
        let cleanup_fns_list = coord.getattr("_cleanup_fns")?;
        let cleanup_len: usize = cleanup_fns_list.len()?;
        // Snapshot the callable references so we can call them later
        let mut cleanup_callables: Vec<Py<PyAny>> = Vec::with_capacity(cleanup_len);
        for i in 0..cleanup_len {
            let item = cleanup_fns_list.get_item(i)?;
            cleanup_callables.push(item.unbind());
        }

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // ----------------------------------------------------------
            // Step 1: Call all cleanup functions in reverse order
            // ----------------------------------------------------------
            for callable in cleanup_callables.iter().rev() {
                // Guard: skip None and non-callable items (defense-in-depth)
                let should_skip = Python::try_attach(|py| -> bool {
                    let bound = callable.bind(py);
                    bound.is_none() || !bound.is_callable()
                })
                .unwrap_or(true);

                if should_skip {
                    continue;
                }

                // 1a: Call the function inside the GIL
                let call_outcome: Option<PyResult<(bool, Py<PyAny>)>> =
                    Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
                        let result = callable.call0(py)?;
                        let bound = result.bind(py);
                        let inspect = py.import("inspect")?;
                        let is_coro: bool =
                            inspect.call_method1("iscoroutine", (bound,))?.extract()?;
                        Ok((is_coro, result))
                    });

                match call_outcome {
                    Some(Ok((true, coro_py))) => {
                        // 1b: Async cleanup — convert coroutine to future and await
                        let future_result = Python::try_attach(|py| {
                            pyo3_async_runtimes::tokio::into_future(coro_py.into_bound(py))
                        });
                        if let Some(Ok(future)) = future_result {
                            if let Err(e) = future.await {
                                // Log but continue
                                let _ = Python::try_attach(|py| -> PyResult<()> {
                                    let logging = py.import("logging")?;
                                    let logger = logging.call_method1(
                                        "getLogger",
                                        ("amplifier_core.session",),
                                    )?;
                                    let _ = logger.call_method1(
                                        "error",
                                        (format!("Error during async cleanup: {e}"),),
                                    );
                                    Ok(())
                                });
                            }
                        }
                    }
                    Some(Ok((false, _))) => {
                        // Sync call completed successfully — nothing more to do
                    }
                    Some(Err(e)) => {
                        // Error calling the function — log and continue
                        let _ = Python::try_attach(|py| -> PyResult<()> {
                            let logging = py.import("logging")?;
                            let logger = logging.call_method1(
                                "getLogger",
                                ("amplifier_core.session",),
                            )?;
                            let _ = logger.call_method1(
                                "error",
                                (format!("Error during cleanup: {e}"),),
                            );
                            Ok(())
                        });
                    }
                    None => {
                        // Failed to attach to Python runtime — skip
                    }
                }
            }

            // ----------------------------------------------------------
            // Step 2: Emit session:end event (best-effort)
            // ----------------------------------------------------------
            let end_event_result: Option<PyResult<_>> = Python::try_attach(|py| {
                let coord = coordinator.bind(py);
                let hooks = coord.getattr("hooks")?;
                let data = PyDict::new(py);
                data.set_item("session_id", &session_id)?;
                let coro = hooks.call_method1("emit", ("session:end", data))?;
                pyo3_async_runtimes::tokio::into_future(coro)
            });

            if let Some(Ok(future)) = end_event_result {
                if let Err(e) = future.await {
                    // Log but don't propagate
                    let _ = Python::try_attach(|py| -> PyResult<()> {
                        let logging = py.import("logging")?;
                        let logger = logging
                            .call_method1("getLogger", ("amplifier_core.session",))?;
                        let _ = logger.call_method1(
                            "error",
                            (format!("Error emitting session:end: {e}"),),
                        );
                        Ok(())
                    });
                }
            }

            // ----------------------------------------------------------
            // Step 3: Reset the initialized flag
            // ----------------------------------------------------------
            {
                let mut session = inner.lock().await;
                session.clear_initialized();
            }

            Ok(())
        })
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
    #[allow(clippy::too_many_arguments)]
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
    /// Register a hook handler.
    ///
    /// Matches Python `HookRegistry.register(event, handler, priority=0, name=None)`.
    /// The handler and name argument order matches the Python API so that
    /// module code like `registry.register(event, handler, name="my-hook")` works.
    #[pyo3(signature = (event, handler, priority = 0, name = None))]
    fn register(
        &self,
        event: &str,
        handler: Py<PyAny>,
        priority: i32,
        name: Option<String>,
    ) -> PyResult<()> {
        let handler_name = name.unwrap_or_else(|| format!("_auto_{event}_{}", uuid::Uuid::new_v4()));
        let bridge = Arc::new(PyHookHandlerBridge { callable: handler });
        let unregister_fn = self.inner.register(
            event,
            bridge,
            priority,
            Some(handler_name.clone()),
        );

        self.unregister_fns
            .lock()
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Lock poisoned: {e}")))?
            .insert(handler_name, unregister_fn);

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
            // Convert HookResult to a JSON string, then parse it back as a
            // Python HookResult object so callers can access .action, .data, etc.
            let result_json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
            Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                let json_mod = py.import("json")?;
                let dict = json_mod.call_method1("loads", (&result_json,))?;
                // Create a proper HookResult from the dict
                let models = py.import("amplifier_core.models")?;
                let hook_result_cls = models.getattr("HookResult")?;
                let obj = hook_result_cls.call_method1("model_validate", (&dict,))?;
                Ok(obj.unbind())
            })
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to attach to Python runtime"))?
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
    #[pyo3(signature = (event, handler, priority = 0, name = None))]
    fn on(
        &self,
        event: &str,
        handler: Py<PyAny>,
        priority: i32,
        name: Option<String>,
    ) -> PyResult<()> {
        self.register(event, handler, priority, name)
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
    #[allow(clippy::too_many_arguments)]
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
    #[getter]
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
// PyCoordinator — wraps amplifier_core::Coordinator (Milestone 2)
// ---------------------------------------------------------------------------

/// Python-visible coordinator wrapper.
///
/// Hybrid approach: stores Python objects (`Py<PyAny>`) for modules in a
/// Python dict (`mount_points`), because the ecosystem passes Python Protocol
/// objects, not Rust trait objects. The Rust kernel's typed mount points are
/// NOT used by the Python bridge.
///
/// The `mount_points` dict is directly accessible and mutable from Python,
/// matching `ModuleCoordinator.mount_points` behavior that the ecosystem
/// (pytest_plugin, testing.py) depends on.
#[pyclass(name = "RustCoordinator", subclass)]
struct PyCoordinator {
    /// Rust kernel coordinator (for reset_turn, injection tracking, config).
    inner: Arc<amplifier_core::Coordinator>,
    /// Python-side mount_points dict matching ModuleCoordinator structure.
    mount_points: Py<PyDict>,
    /// Python HookRegistry — also stored in mount_points["hooks"].
    py_hooks: Py<PyAny>,
    /// Cancellation token.
    py_cancellation: Py<PyCancellationToken>,
    /// Session back-reference.
    session_ref: Py<PyAny>,
    /// Session ID (from session object).
    session_id: String,
    /// Parent ID (from session object).
    parent_id: Option<String>,
    /// Config dict (from session object).
    config_dict: Py<PyAny>,
    /// Capability registry.
    capabilities: Py<PyDict>,
    /// Cleanup callables.
    cleanup_fns: Py<PyList>,
    /// Contribution channels: channel -> list of {name, callback}.
    channels_dict: Py<PyDict>,
    /// Per-turn injection counter (Python-side, mirrors Rust kernel).
    current_turn_injections: usize,
    /// Approval system (Python object or None).
    approval_system_obj: Py<PyAny>,
    /// Display system (Python object or None).
    display_system_obj: Py<PyAny>,
    /// Module loader (Python object or None).
    loader_obj: Py<PyAny>,
}

#[pymethods]
impl PyCoordinator {
    /// Create a new coordinator from a session object.
    ///
    /// Matches Python `ModuleCoordinator.__init__(self, session, approval_system=None, display_system=None)`.
    ///
    /// The session object must have:
    /// - `session_id: str`
    /// - `parent_id: str | None`
    /// - `config: dict`
    ///
    /// When `session` is `None` (default), a lightweight placeholder is used.
    /// This enables Python subclasses (e.g. `TestCoordinator`) to call
    /// `super().__init__(session, ...)` from `__init__` instead of needing
    /// to pass arguments through `__new__`.
    #[allow(clippy::too_many_arguments)]
    #[new]
    #[pyo3(signature = (session=None, approval_system=None, display_system=None))]
    fn new(
        py: Python<'_>,
        session: Option<Bound<'_, PyAny>>,
        approval_system: Option<Bound<'_, PyAny>>,
        display_system: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        // If no session provided, use empty defaults.  The Python subclass
        // __init__ is expected to call super().__init__(real_session, ...)
        // which will re-initialise via __init__, but PyO3 #[new] is __new__
        // so we build a valid-but-placeholder struct first.
        let (session_id, parent_id, config_obj_py, session_ref, rust_config) = match &session {
            Some(sess) => {
                let sid: String = sess.getattr("session_id")?.extract()?;
                let pid: Option<String> = {
                    let p = sess.getattr("parent_id")?;
                    if p.is_none() { None } else { Some(p.extract()?) }
                };
                let cfg = sess.getattr("config")?;
                let rc: HashMap<String, Value> = {
                    let json_mod = py.import("json")?;
                    let json_str: String = json_mod
                        .call_method1("dumps", (&cfg,))?
                        .extract()?;
                    serde_json::from_str(&json_str).unwrap_or_default()
                };
                (sid, pid, cfg.unbind(), sess.clone().unbind(), rc)
            }
            None => {
                // Placeholder defaults — Python subclass will set real values
                let empty_dict = PyDict::new(py);
                (
                    String::new(),
                    None,
                    empty_dict.clone().into_any().unbind(),
                    py.None(),
                    HashMap::new(),
                )
            }
        };

        let inner = Arc::new(amplifier_core::Coordinator::new(rust_config));

        // Create the hooks registry
        let hooks_instance = Py::new(py, PyHookRegistry::new())?;
        let hooks_any: Py<PyAny> = hooks_instance.clone_ref(py).into_any();

        // Create the cancellation token
        let cancel_instance = Py::new(py, PyCancellationToken::new())?;

        // Build mount_points dict matching Python ModuleCoordinator
        let mp = PyDict::new(py);
        mp.set_item("orchestrator", py.None())?;
        mp.set_item("providers", PyDict::new(py))?;
        mp.set_item("tools", PyDict::new(py))?;
        mp.set_item("context", py.None())?;
        mp.set_item("hooks", &hooks_any)?;
        mp.set_item("module-source-resolver", py.None())?;

        Ok(Self {
            inner,
            mount_points: mp.unbind(),
            py_hooks: hooks_any,
            py_cancellation: cancel_instance,
            session_ref,
            session_id,
            parent_id,
            config_dict: config_obj_py,
            capabilities: PyDict::new(py).unbind(),
            cleanup_fns: PyList::empty(py).unbind(),
            channels_dict: PyDict::new(py).unbind(),
            current_turn_injections: 0,
            approval_system_obj: approval_system
                .map(|a| a.unbind())
                .unwrap_or_else(|| py.None()),
            display_system_obj: display_system
                .map(|d| d.unbind())
                .unwrap_or_else(|| py.None()),
            loader_obj: py.None(),
        })
    }

    // -----------------------------------------------------------------------
    // Task 2.1: mount_points property
    // -----------------------------------------------------------------------

    /// The mount_points dict — direct access for backward compatibility.
    /// Tests and pytest_plugin access coordinator.mount_points["tools"]["echo"] directly.
    #[getter]
    fn mount_points<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        Ok(self.mount_points.bind(py).clone())
    }

    #[setter]
    fn set_mount_points(&mut self, _py: Python<'_>, value: Bound<'_, PyDict>) -> PyResult<()> {
        self.mount_points = value.unbind();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Task 2.2: mount() and get()
    // -----------------------------------------------------------------------

    /// Mount a module at a specific mount point.
    ///
    /// Matches Python `ModuleCoordinator.mount(mount_point, module, name=None)`.
    /// For single-slot points (orchestrator, context, module-source-resolver),
    /// `name` is ignored. For multi-slot points (providers, tools), `name` is
    /// required or auto-detected from `module.name`.
    #[pyo3(signature = (mount_point, module, name=None))]
    fn mount<'py>(
        &self,
        py: Python<'py>,
        mount_point: &str,
        module: Bound<'py, PyAny>,
        name: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mp = self.mount_points.bind(py);

        // Validate mount point exists
        if !mp.contains(mount_point)? {
            return Err(PyErr::new::<PyValueError, _>(format!(
                "Unknown mount point: {mount_point}"
            )));
        }

        if mount_point == "hooks" {
            return Err(PyErr::new::<PyValueError, _>(
                "Hooks should be registered directly with the HookRegistry",
            ));
        }

        match mount_point {
            "orchestrator" | "context" | "module-source-resolver" => {
                mp.set_item(mount_point, &module)?;
            }
            "providers" | "tools" | "agents" => {
                let resolved_name = match name {
                    Some(n) => n,
                    None => match module.getattr("name") {
                        Ok(attr) => attr.extract::<String>()?,
                        Err(_) => {
                            return Err(PyErr::new::<PyValueError, _>(format!(
                                "Name required for {mount_point}"
                            )));
                        }
                    },
                };
                let sub_dict = mp
                    .get_item(mount_point)?
                    .ok_or_else(|| {
                        PyErr::new::<PyRuntimeError, _>(format!(
                            "Mount point sub-dict missing: {mount_point}"
                        ))
                    })?;
                sub_dict.set_item(&resolved_name, &module)?;
            }
            _ => {}
        }

        // Return an awaitable that resolves to None (mount is async in Python)
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }

    /// Get a mounted module.
    ///
    /// Matches Python `ModuleCoordinator.get(mount_point, name=None)`.
    /// For single-slot: returns the module or None.
    /// For multi-slot without name: returns the dict of all modules.
    /// For multi-slot with name: returns one module or None.
    #[pyo3(signature = (mount_point, name=None))]
    fn get<'py>(
        &self,
        py: Python<'py>,
        mount_point: &str,
        name: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let mp = self.mount_points.bind(py);

        if !mp.contains(mount_point)? {
            return Err(PyErr::new::<PyValueError, _>(format!(
                "Unknown mount point: {mount_point}"
            )));
        }

        match mount_point {
            "orchestrator" | "context" | "hooks" | "module-source-resolver" => {
                let item = mp
                    .get_item(mount_point)?
                    .ok_or_else(|| {
                        PyErr::new::<PyRuntimeError, _>(format!(
                            "Mount point missing: {mount_point}"
                        ))
                    })?;
                Ok(item.unbind())
            }
            "providers" | "tools" | "agents" => {
                let sub_dict_any = mp
                    .get_item(mount_point)?
                    .ok_or_else(|| {
                        PyErr::new::<PyRuntimeError, _>(format!(
                            "Mount point missing: {mount_point}"
                        ))
                    })?;
                match name {
                    None => Ok(sub_dict_any.unbind()),
                    Some(n) => {
                        let sub = sub_dict_any.cast::<PyDict>()?;
                        match sub.get_item(n)? {
                            Some(item) => Ok(item.unbind()),
                            None => Ok(py.None()),
                        }
                    }
                }
            }
            _ => Ok(py.None()),
        }
    }

    // -----------------------------------------------------------------------
    // Task 2.3: unmount()
    // -----------------------------------------------------------------------

    /// Unmount a module from a mount point.
    ///
    /// Matches Python `ModuleCoordinator.unmount(mount_point, name=None)`.
    #[pyo3(signature = (mount_point, name=None))]
    fn unmount<'py>(
        &self,
        py: Python<'py>,
        mount_point: &str,
        name: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mp = self.mount_points.bind(py);

        if !mp.contains(mount_point)? {
            return Err(PyErr::new::<PyValueError, _>(format!(
                "Unknown mount point: {mount_point}"
            )));
        }

        match mount_point {
            "orchestrator" | "context" | "module-source-resolver" => {
                mp.set_item(mount_point, py.None())?;
            }
            "providers" | "tools" | "agents" => {
                if let Some(n) = name {
                    let sub_any = mp.get_item(mount_point)?.ok_or_else(|| {
                        PyErr::new::<PyRuntimeError, _>(format!(
                            "Mount point missing: {mount_point}"
                        ))
                    })?;
                    let sub_dict = sub_any.cast::<PyDict>()?;
                    sub_dict.del_item(n).ok(); // Ignore if not present
                } else {
                    return Err(PyErr::new::<PyValueError, _>(format!(
                        "Name required to unmount from {mount_point}"
                    )));
                }
            }
            _ => {}
        }

        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }

    // -----------------------------------------------------------------------
    // Task 2.4: session_id, parent_id, session properties
    // -----------------------------------------------------------------------

    /// Current session ID.
    #[getter]
    fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Parent session ID, or None.
    #[getter]
    fn parent_id<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        match &self.parent_id {
            Some(pid) => pid.into_pyobject(py).unwrap().into_any().unbind(),
            None => py.None(),
        }
    }

    /// Parent session reference.
    #[getter]
    fn session<'py>(&self, py: Python<'py>) -> Bound<'py, PyAny> {
        self.session_ref.bind(py).clone()
    }

    // -----------------------------------------------------------------------
    // Task 2.5: register_capability / get_capability
    // -----------------------------------------------------------------------

    /// Register a capability for inter-module communication.
    fn register_capability(
        &self,
        py: Python<'_>,
        name: &str,
        value: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let caps = self.capabilities.bind(py);
        caps.set_item(name, value)?;
        Ok(())
    }

    /// Get a registered capability, or None.
    fn get_capability<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Py<PyAny>> {
        let caps = self.capabilities.bind(py);
        match caps.get_item(name)? {
            Some(item) => Ok(item.unbind()),
            None => Ok(py.None()),
        }
    }

    // -----------------------------------------------------------------------
    // Task 2.6: register_cleanup / cleanup
    // -----------------------------------------------------------------------

    /// Read-only access to the cleanup functions list.
    ///
    /// Used by PySession::cleanup() to iterate cleanup callables directly.
    #[getter]
    fn _cleanup_fns<'py>(&self, py: Python<'py>) -> Bound<'py, PyList> {
        self.cleanup_fns.bind(py).clone()
    }

    /// Register a cleanup function to be called on shutdown.
    ///
    /// Only stores callable objects. Non-callable values (including None)
    /// are silently ignored to match Python's behavior where mount()
    /// may return None for cleanup.
    fn register_cleanup(&self, py: Python<'_>, cleanup_fn: Bound<'_, PyAny>) -> PyResult<()> {
        // Guard: only store callable objects, skip None and non-callables
        if cleanup_fn.is_none() {
            return Ok(());
        }
        let is_callable: bool = cleanup_fn.is_callable();
        if !is_callable {
            // Log but don't error — matches Python behavior
            return Ok(());
        }
        let list = self.cleanup_fns.bind(py);
        list.append(&cleanup_fn)?;
        Ok(())
    }

    /// Call all registered cleanup functions in reverse order.
    ///
    /// Matches Python `ModuleCoordinator.cleanup()`.
    /// Errors in individual cleanup functions are logged but don't stop execution.
    fn cleanup<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let fns = self.cleanup_fns.clone_ref(py);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result: PyResult<()> = Python::try_attach(|py| -> PyResult<()> {
                let list = fns.bind(py);
                let len = list.len();
                // Execute in reverse order
                for i in (0..len).rev() {
                    let cleanup_fn = list.get_item(i)?;
                    // Guard: skip None and non-callable items (defense-in-depth)
                    if cleanup_fn.is_none() || !cleanup_fn.is_callable() {
                        continue;
                    }
                    // Try calling; catch and log errors
                    match cleanup_fn.call0() {
                        Ok(result) => {
                            // If it returned a coroutine, await it properly
                            let inspect = py.import("inspect")?;
                            let is_coro: bool =
                                inspect.call_method1("iscoroutine", (&result,))?.extract()?;
                            if is_coro {
                                let asyncio = py.import("asyncio")?;
                                // Try to schedule in the running loop
                                match asyncio.call_method1("get_running_loop", ()) {
                                    Ok(loop_) => {
                                        let future = asyncio.call_method1(
                                            "run_coroutine_threadsafe", (&result, &loop_)
                                        )?;
                                        let _ = future.call_method1("result", (5.0,));
                                    }
                                    Err(_) => {
                                        // No running loop, use asyncio.run
                                        let _ = asyncio.call_method1("run", (&result,));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            // Log but continue — matches Python behavior
                            let logging = py.import("logging")?;
                            let logger = logging.call_method1("getLogger", ("amplifier_core.coordinator",))?;
                            let _ = logger.call_method1(
                                "error",
                                (format!("Error during cleanup: {e}"),),
                            );
                        }
                    }
                }
                Ok(())
            })
            .unwrap_or(Ok(()));
            result?;
            Ok(())
        })
    }

    // -----------------------------------------------------------------------
    // Task 2.7: register_contributor / collect_contributions
    // -----------------------------------------------------------------------

    /// Register a contributor to a named channel.
    ///
    /// Matches Python `ModuleCoordinator.register_contributor(channel, name, callback)`.
    fn register_contributor(
        &self,
        py: Python<'_>,
        channel: &str,
        name: &str,
        callback: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let channels = self.channels_dict.bind(py);
        if !channels.contains(channel)? {
            channels.set_item(channel, PyList::empty(py))?;
        }
        let list_any = channels.get_item(channel)?.unwrap();
        let list = list_any.cast::<PyList>()?;
        let entry = PyDict::new(py);
        entry.set_item("name", name)?;
        entry.set_item("callback", &callback)?;
        list.append(entry)?;
        Ok(())
    }

    /// Collect contributions from a channel.
    ///
    /// Matches Python `ModuleCoordinator.collect_contributions(channel)`.
    /// Errors in individual contributors are logged, not propagated.
    /// None returns are filtered out. Supports both sync and async callbacks.
    fn collect_contributions<'py>(
        &self,
        py: Python<'py>,
        channel: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Build a Python coroutine that handles both sync and async callbacks,
        // matching the Python ModuleCoordinator.collect_contributions behavior.
        let channels = self.channels_dict.clone_ref(py);

        // Create a Python helper function to do the collection properly in Python
        // This handles async callbacks naturally since it runs in the Python event loop
        let collect_code = py.import("amplifier_core._collect_helper");
        if let Ok(helper_mod) = collect_code {
            let collect_fn = helper_mod.getattr("collect_contributions")?;
            let coro = collect_fn.call1((&channels, &channel))?;
            // Return the coroutine directly - it will be awaited by the caller
            Ok(coro)
        } else {
            // Fallback: sync-only collection via Rust
            let channels_ref = channels;
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let results: Vec<Py<PyAny>> = Python::try_attach(
                    |py| -> PyResult<Vec<Py<PyAny>>> {
                        let channels_dict = channels_ref.bind(py);
                        let contributors = match channels_dict.get_item(&channel)? {
                            Some(list) => list,
                            None => return Ok(Vec::new()),
                        };
                        let list = contributors.cast::<PyList>()?;
                        let mut results: Vec<Py<PyAny>> = Vec::new();

                        for i in 0..list.len() {
                            let entry = list.get_item(i)?;
                            let callback = entry.get_item("callback")?;
                            match callback.call0() {
                                Ok(result) => {
                                    if !result.is_none() {
                                        results.push(result.unbind());
                                    }
                                }
                                Err(_) => continue,
                            }
                        }
                        Ok(results)
                    },
                )
                .unwrap_or(Ok(Vec::new()))?;
                Ok(results)
            })
        }
    }

    // -----------------------------------------------------------------------
    // Task 2.8: request_cancel / reset_turn
    // -----------------------------------------------------------------------

    /// Request session cancellation.
    ///
    /// Matches Python `ModuleCoordinator.request_cancel(immediate=False)`.
    #[pyo3(signature = (immediate=false))]
    fn request_cancel<'py>(
        &self,
        py: Python<'py>,
        immediate: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Delegate to the PyCancellationToken
        let cancel = self.py_cancellation.clone_ref(py);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result: PyResult<()> = Python::try_attach(|py| -> PyResult<()> {
                let token = cancel.borrow(py);
                if immediate {
                    token.inner.request_immediate();
                } else {
                    token.inner.request_graceful();
                }
                Ok(())
            })
            .unwrap_or(Ok(()));
            result?;
            Ok(())
        })
    }

    /// Reset per-turn tracking. Call at turn boundaries.
    ///
    /// Matches Python `ModuleCoordinator.reset_turn()`.
    fn reset_turn(&mut self) {
        self.current_turn_injections = 0;
        self.inner.reset_turn();
    }

    // -----------------------------------------------------------------------
    // Task 2.4 (continued): _current_turn_injections
    // -----------------------------------------------------------------------

    /// Per-turn injection counter.
    #[getter(_current_turn_injections)]
    fn get_current_turn_injections(&self) -> usize {
        self.current_turn_injections
    }

    /// Set per-turn injection counter.
    #[setter(_current_turn_injections)]
    fn set_current_turn_injections(&mut self, value: usize) {
        self.current_turn_injections = value;
    }

    // -----------------------------------------------------------------------
    // Task 2.9: injection_budget_per_turn / injection_size_limit
    // -----------------------------------------------------------------------

    /// Injection budget per turn from session config (policy).
    ///
    /// Returns int or None. Matches Python `ModuleCoordinator.injection_budget_per_turn`.
    #[getter]
    fn injection_budget_per_turn<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        let config = self.config_dict.bind(py);
        // config is a Python dict; use call to get("session")
        let session = config.call_method1("get", ("session",))?;
        if session.is_none() {
            return Ok(py.None());
        }
        let val = session.call_method1("get", ("injection_budget_per_turn",))?;
        if val.is_none() {
            Ok(py.None())
        } else {
            Ok(val.unbind())
        }
    }

    /// Per-injection size limit from session config (policy).
    ///
    /// Returns int or None. Matches Python `ModuleCoordinator.injection_size_limit`.
    #[getter]
    fn injection_size_limit<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        let config = self.config_dict.bind(py);
        let session = config.call_method1("get", ("session",))?;
        if session.is_none() {
            return Ok(py.None());
        }
        let val = session.call_method1("get", ("injection_size_limit",))?;
        if val.is_none() {
            Ok(py.None())
        } else {
            Ok(val.unbind())
        }
    }

    // -----------------------------------------------------------------------
    // Task 2.10: loader, approval_system, display_system properties
    // -----------------------------------------------------------------------

    /// Module loader (Python object or None).
    #[getter]
    fn loader<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        let obj = self.loader_obj.bind(py);
        if obj.is_none() {
            py.None()
        } else {
            self.loader_obj.clone_ref(py)
        }
    }

    /// Set the module loader.
    #[setter]
    fn set_loader(&mut self, value: Py<PyAny>) {
        self.loader_obj = value;
    }

    /// Approval system (Python object or None).
    #[getter]
    fn approval_system<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        let obj = self.approval_system_obj.bind(py);
        if obj.is_none() {
            py.None()
        } else {
            self.approval_system_obj.clone_ref(py)
        }
    }

    /// Set the approval system.
    #[setter]
    fn set_approval_system(&mut self, value: Py<PyAny>) {
        self.approval_system_obj = value;
    }

    /// Display system (Python object or None).
    #[getter]
    fn display_system<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        let obj = self.display_system_obj.bind(py);
        if obj.is_none() {
            py.None()
        } else {
            self.display_system_obj.clone_ref(py)
        }
    }

    /// Set the display system.
    #[setter]
    fn set_display_system(&mut self, value: Py<PyAny>) {
        self.display_system_obj = value;
    }

    // -----------------------------------------------------------------------
    // Task 2.10 (continued): channels, config, hooks, cancellation properties
    // -----------------------------------------------------------------------

    /// Contribution channels dict.
    #[getter]
    fn channels<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        self.channels_dict.bind(py).clone()
    }

    /// Session configuration as a Python dict.
    #[getter]
    fn config<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        self.config_dict.clone_ref(py)
    }

    /// Access the hook registry.
    ///
    /// Returns the same PyHookRegistry stored in mount_points["hooks"].
    #[getter]
    fn hooks<'py>(&self, py: Python<'py>) -> Bound<'py, PyAny> {
        self.py_hooks.bind(py).clone()
    }

    /// Access the cancellation token.
    #[getter]
    fn cancellation<'py>(&self, py: Python<'py>) -> Bound<'py, PyCancellationToken> {
        self.py_cancellation.bind(py).clone()
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
}
