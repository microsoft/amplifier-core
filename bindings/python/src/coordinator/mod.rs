// ---------------------------------------------------------------------------
// PyCoordinator — wraps amplifier_core::Coordinator
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value;

use crate::cancellation::PyCancellationToken;
use crate::helpers::{try_model_dump, wrap_future_as_coroutine};
use crate::hooks::PyHookRegistry;

mod capabilities;
mod hook_dispatch;
mod mount_points;

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
pub(crate) struct PyCoordinator {
    /// Rust kernel coordinator (for reset_turn, injection tracking, config).
    pub(crate) inner: Arc<amplifier_core::Coordinator>,
    /// Python-side mount_points dict matching ModuleCoordinator structure.
    pub(crate) mount_points: Py<PyDict>,
    /// Python HookRegistry — also stored in mount_points["hooks"].
    pub(crate) py_hooks: Py<PyAny>,
    /// Cancellation token.
    pub(crate) py_cancellation: Py<PyCancellationToken>,
    /// Session back-reference.
    pub(crate) session_ref: Py<PyAny>,
    /// Session ID (from session object).
    pub(crate) session_id: String,
    /// Parent ID (from session object).
    pub(crate) parent_id: Option<String>,
    /// Config dict (from session object).
    pub(crate) config_dict: Py<PyAny>,
    /// Capability registry.
    pub(crate) capabilities: Py<PyDict>,
    /// Cleanup callables.
    pub(crate) cleanup_fns: Py<PyList>,
    /// Contribution channels: channel -> list of {name, callback}.
    pub(crate) channels_dict: Py<PyDict>,
    /// Per-turn injection counter (Python-side, mirrors Rust kernel).
    pub(crate) current_turn_injections: usize,
    /// Approval system (Python object or None).
    pub(crate) approval_system_obj: Py<PyAny>,
    /// Display system (Python object or None).
    pub(crate) display_system_obj: Py<PyAny>,
    /// Module loader (Python object or None).
    pub(crate) loader_obj: Py<PyAny>,
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
                    if p.is_none() {
                        None
                    } else {
                        Some(p.extract()?)
                    }
                };
                let cfg = sess.getattr("config")?;
                let rc: HashMap<String, Value> = {
                    let json_mod = py.import("json")?;
                    let serializable = try_model_dump(&cfg);
                    let json_str: String = json_mod
                        .call_method1("dumps", (&serializable,))?
                        .extract()?;
                    serde_json::from_str(&json_str).unwrap_or_else(|e| {
                        log::warn!("Failed to parse session config as JSON object (using empty config): {e}");
                        HashMap::new()
                    })
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
    // session_id, parent_id, session properties
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

    /// Update the session back-reference after construction.
    ///
    /// Called by `PySession::initialize()` to replace the `SimpleNamespace`
    /// placeholder (set during `PySession::new()`) with the real session object.
    /// This closes the chicken-and-egg circle: `new()` can't pass `self` to the
    /// coordinator because `self` doesn't exist yet, so `initialize()` patches
    /// it here once both objects are fully constructed.
    fn _set_session(&mut self, session: Bound<'_, PyAny>) {
        self.session_ref = session.unbind();
    }

    // -----------------------------------------------------------------------
    // _cleanup_fns / cleanup
    // -----------------------------------------------------------------------

    /// Read-only access to the cleanup functions list.
    ///
    /// Used by PySession::cleanup() to iterate cleanup callables directly.
    #[getter]
    fn _cleanup_fns<'py>(&self, py: Python<'py>) -> Bound<'py, PyList> {
        self.cleanup_fns.bind(py).clone()
    }

    /// Call all registered cleanup functions in reverse order.
    ///
    /// Matches Python `ModuleCoordinator.cleanup()`.
    /// Errors in individual cleanup functions are logged but don't stop execution.
    /// Uses `into_future` for async cleanup functions (same pattern as PySession::cleanup).
    fn cleanup<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let fns = self.cleanup_fns.clone_ref(py);
        let inspect = py.import("inspect")?;

        // Pre-check iscoroutinefunction while holding the GIL, matching
        // Python main's pattern of checking BEFORE calling.
        let list = fns.bind(py);
        let len = list.len();
        let mut callables: Vec<(Py<PyAny>, bool)> = Vec::with_capacity(len);
        for i in 0..len {
            let item = list.get_item(i)?;
            if item.is_none() || !item.is_callable() {
                continue;
            }
            let is_async: bool = inspect
                .call_method1("iscoroutinefunction", (&item,))?
                .extract()?;
            callables.push((item.unbind(), is_async));
        }

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Execute in reverse order
                for (callable, is_async) in callables.iter().rev() {
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
                                    Ok(None)
                                }
                            });

                        match call_outcome {
                            Some(Ok(Some(coro_py))) => {
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
                Ok(())
            }),
        )
    }

    // -----------------------------------------------------------------------
    // config property
    // -----------------------------------------------------------------------

    /// Session configuration as a Python dict.
    #[getter]
    fn config<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        self.config_dict.clone_ref(py)
    }

    // -----------------------------------------------------------------------
    // to_dict() — audit finding #1
    // -----------------------------------------------------------------------

    /// Return a plain Python dict exposing Rust-managed coordinator state.
    ///
    /// Addresses production audit finding: `vars(coordinator)` returns only
    /// the Python `__dict__`, missing all Rust-managed state. This method
    /// provides a reliable introspection surface.
    ///
    /// Returns dict with keys:
    /// - `tools` (list of str): mounted tool names
    /// - `providers` (list of str): mounted provider names
    /// - `has_orchestrator` (bool): whether an orchestrator is mounted
    /// - `has_context` (bool): whether a context manager is mounted
    /// - `capabilities` (list of str): registered capability names
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);

        // tools: list of mounted tool names from mount_points["tools"]
        let mp = self.mount_points.bind(py);
        let tools_dict = mp
            .get_item("tools")?
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("mount_points missing 'tools'"))?;
        let tools_keys: Vec<String> = tools_dict
            .cast::<PyDict>()?
            .keys()
            .iter()
            .map(|k| k.extract::<String>().unwrap_or_default())
            .collect();
        dict.set_item("tools", PyList::new(py, &tools_keys)?)?;

        // providers: list of mounted provider names from mount_points["providers"]
        let providers_dict = mp
            .get_item("providers")?
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("mount_points missing 'providers'"))?;
        let provider_keys: Vec<String> = providers_dict
            .cast::<PyDict>()?
            .keys()
            .iter()
            .map(|k| k.extract::<String>().unwrap_or_default())
            .collect();
        dict.set_item("providers", PyList::new(py, &provider_keys)?)?;

        // has_orchestrator: whether orchestrator mount point is not None
        let orch = mp.get_item("orchestrator")?.ok_or_else(|| {
            PyErr::new::<PyRuntimeError, _>("mount_points missing 'orchestrator'")
        })?;
        dict.set_item("has_orchestrator", !orch.is_none())?;

        // has_context: whether context mount point is not None
        let ctx = mp
            .get_item("context")?
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("mount_points missing 'context'"))?;
        dict.set_item("has_context", !ctx.is_none())?;

        // capabilities: list of registered capability names
        let caps = self.capabilities.bind(py);
        let cap_keys: Vec<String> = caps
            .keys()
            .iter()
            .map(|k| k.extract::<String>().unwrap_or_default())
            .collect();
        dict.set_item("capabilities", PyList::new(py, &cap_keys)?)?;

        // has_approval_provider: whether a Rust-side approval provider is mounted
        dict.set_item("has_approval_provider", self.inner.has_approval_provider())?;

        // has_display_service: whether a Rust-side display service is mounted
        dict.set_item("has_display_service", self.inner.has_display_service())?;

        Ok(dict)
    }
}
