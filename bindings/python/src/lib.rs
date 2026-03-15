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
                let sub_dict = mp.get_item(mount_point)?.ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>(format!(
                        "Mount point sub-dict missing: {mount_point}"
                    ))
                })?;
                sub_dict.set_item(&resolved_name, &module)?;
            }
            _ => {}
        }

        // Return an awaitable that resolves to None (mount is async in Python)
        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) }),
        )
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
                let item = mp.get_item(mount_point)?.ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>(format!("Mount point missing: {mount_point}"))
                })?;
                Ok(item.unbind())
            }
            "providers" | "tools" | "agents" => {
                let sub_dict_any = mp.get_item(mount_point)?.ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>(format!("Mount point missing: {mount_point}"))
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

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) }),
        )
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
            wrap_future_as_coroutine(
                py,
                pyo3_async_runtimes::tokio::future_into_py(py, async move {
                    let results: Vec<Py<PyAny>> =
                        Python::try_attach(|py| -> PyResult<Vec<Py<PyAny>>> {
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
                        })
                        .unwrap_or(Ok(Vec::new()))?;
                    Ok(results)
                }),
            )
        }
    }

    // -----------------------------------------------------------------------
    // Task 2.8: request_cancel / reset_turn
    // -----------------------------------------------------------------------

    /// Request session cancellation.
    ///
    /// Matches Python `ModuleCoordinator.request_cancel(immediate=False)`.
    #[pyo3(signature = (immediate=false))]
    fn request_cancel<'py>(&self, py: Python<'py>, immediate: bool) -> PyResult<Bound<'py, PyAny>> {
        // Delegate to the PyCancellationToken
        let cancel = self.py_cancellation.clone_ref(py);
        wrap_future_as_coroutine(
            py,
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
            }),
        )
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
        // Set or clear the Rust-side approval provider based on whether value is None
        match Python::try_attach(|py| -> PyResult<()> {
            if value.bind(py).is_none() {
                self.inner.clear_approval_provider();
            } else {
                let bridge = Arc::new(PyApprovalProviderBridge {
                    py_obj: value.clone_ref(py),
                });
                self.inner.set_approval_provider(bridge);
            }
            Ok(())
        }) {
            Some(Ok(())) => {}
            Some(Err(e)) => {
                log::warn!("Failed to set approval provider bridge: {e}");
            }
            None => {
                log::warn!("Could not attach to Python runtime while setting approval provider");
            }
        }
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
        // Set or clear the Rust-side display service based on whether value is None
        match Python::try_attach(|py| -> PyResult<()> {
            if value.bind(py).is_none() {
                // No clear method exists; setting None just keeps Python-side ref
            } else {
                let bridge = Arc::new(PyDisplayServiceBridge {
                    py_obj: value.clone_ref(py),
                });
                self.inner.set_display_service(bridge);
            }
            Ok(())
        }) {
            Some(Ok(())) => {}
            Some(Err(e)) => {
                log::warn!("Failed to set display service bridge: {e}");
            }
            None => {
                log::warn!("Could not attach to Python runtime while setting display service");
            }
        }
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

    // -----------------------------------------------------------------------
    // Task 12: to_dict() — audit finding #1
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

// ---------------------------------------------------------------------------
// Module resolver bindings
// ---------------------------------------------------------------------------

/// Resolve a module from a filesystem path.
///
/// Returns a dict with keys: "transport", "module_type", "artifact_type",
/// and artifact-specific keys ("artifact_path", "endpoint", "package_name").
#[pyfunction]
fn resolve_module(py: Python<'_>, path: String) -> PyResult<Py<PyDict>> {
    let manifest = amplifier_core::module_resolver::resolve_module(std::path::Path::new(&path))
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("{e}")))?;

    let dict = PyDict::new(py);
    let transport_str = match manifest.transport {
        amplifier_core::transport::Transport::Python => "python",
        amplifier_core::transport::Transport::Wasm => "wasm",
        amplifier_core::transport::Transport::Grpc => "grpc",
        amplifier_core::transport::Transport::Native => "native",
    };
    dict.set_item("transport", transport_str)?;

    let type_str = match manifest.module_type {
        amplifier_core::ModuleType::Tool => "tool",
        amplifier_core::ModuleType::Hook => "hook",
        amplifier_core::ModuleType::Context => "context",
        amplifier_core::ModuleType::Approval => "approval",
        amplifier_core::ModuleType::Provider => "provider",
        amplifier_core::ModuleType::Orchestrator => "orchestrator",
        amplifier_core::ModuleType::Resolver => "resolver",
    };
    dict.set_item("module_type", type_str)?;

    match &manifest.artifact {
        amplifier_core::module_resolver::ModuleArtifact::WasmPath(path) => {
            dict.set_item("artifact_type", "wasm")?;
            dict.set_item("artifact_path", path.to_string_lossy().as_ref())?;
        }
        amplifier_core::module_resolver::ModuleArtifact::WasmBytes { path, .. } => {
            dict.set_item("artifact_type", "wasm")?;
            dict.set_item("artifact_path", path.to_string_lossy().as_ref())?;
        }
        amplifier_core::module_resolver::ModuleArtifact::GrpcEndpoint(endpoint) => {
            dict.set_item("artifact_type", "grpc")?;
            dict.set_item("endpoint", endpoint.as_str())?;
        }
        amplifier_core::module_resolver::ModuleArtifact::PythonModule(name) => {
            dict.set_item("artifact_type", "python")?;
            dict.set_item("package_name", name.as_str())?;
        }
    }

    Ok(dict.unbind())
}

/// Load a WASM module from a resolved manifest path.
///
/// Returns a dict with "status" = "loaded" and "module_type" on success.
/// NOTE: This function loads into a throwaway test coordinator. For production
/// use, prefer `load_and_mount_wasm` which mounts into a real coordinator.
#[pyfunction]
fn load_wasm_from_path(py: Python<'_>, path: String) -> PyResult<Py<PyDict>> {
    let manifest = amplifier_core::module_resolver::resolve_module(std::path::Path::new(&path))
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("{e}")))?;

    if manifest.transport != amplifier_core::transport::Transport::Wasm {
        return Err(PyErr::new::<PyValueError, _>(format!(
            "load_wasm_from_path only handles WASM modules, got transport '{:?}'",
            manifest.transport
        )));
    }

    let engine = amplifier_core::wasm_engine::WasmEngine::new().map_err(|e| {
        PyErr::new::<PyRuntimeError, _>(format!("WASM engine creation failed: {e}"))
    })?;

    let coordinator = std::sync::Arc::new(amplifier_core::Coordinator::new_for_test());
    let loaded =
        amplifier_core::module_resolver::load_module(&manifest, engine.inner(), Some(coordinator))
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Module loading failed: {e}")))?;

    let dict = PyDict::new(py);
    dict.set_item("status", "loaded")?;
    dict.set_item("module_type", loaded.variant_name())?;
    Ok(dict.unbind())
}

// ---------------------------------------------------------------------------
// PyWasmTool — thin Python wrapper around a Rust Arc<dyn Tool>
// ---------------------------------------------------------------------------

/// Python-visible wrapper for a WASM-loaded tool module.
///
/// Bridges the Rust `Arc<dyn Tool>` trait object into Python's tool protocol,
/// so WASM tools can be mounted into a coordinator's `mount_points["tools"]`
/// dict alongside native Python tool modules.
///
/// Exposes: `name` (property), `get_spec()` (sync), `execute(input)` (async).
#[pyclass(name = "WasmTool")]
struct PyWasmTool {
    inner: Arc<dyn amplifier_core::traits::Tool>,
}

// Safety: Arc<dyn Tool> is Send+Sync (required by the Tool trait bound).
unsafe impl Send for PyWasmTool {}
unsafe impl Sync for PyWasmTool {}

#[pymethods]
impl PyWasmTool {
    /// The tool's unique name (e.g., "echo-tool").
    #[getter]
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// The tool's human-readable description.
    #[getter]
    fn description(&self) -> &str {
        self.inner.description()
    }

    /// Return the tool specification as a Python dict.
    ///
    /// The spec contains `name`, `description`, and `input_schema` (JSON Schema).
    fn get_spec(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let spec = self.inner.get_spec();
        let json_str = serde_json::to_string(&spec).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize ToolSpec: {e}"))
        })?;
        let json_mod = py.import("json")?;
        let dict = json_mod.call_method1("loads", (&json_str,))?;
        Ok(dict.unbind())
    }

    /// Execute the tool with JSON input and return the result.
    ///
    /// Async method — returns a coroutine that resolves to a dict with
    /// `success` (bool), `output` (any), and optional `error` (str).
    fn execute<'py>(
        &self,
        py: Python<'py>,
        input: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        // Convert Python input to serde_json::Value
        let json_mod = py.import("json")?;
        let serializable = try_model_dump(&input);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str)
            .map_err(|e| PyErr::new::<PyValueError, _>(format!("Invalid JSON input: {e}")))?;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let result = inner.execute(value).await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Tool execution failed: {e}"))
                })?;

                // Convert ToolResult to Python dict
                let result_json = serde_json::to_string(&result).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize ToolResult: {e}"))
                })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let dict = json_mod.call_method1("loads", (&result_json,))?;
                    Ok(dict.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    fn __repr__(&self) -> String {
        format!("<WasmTool '{}'>", self.inner.name())
    }
}

// ---------------------------------------------------------------------------
// PyWasmProvider — thin Python wrapper around a Rust Arc<dyn Provider>
// ---------------------------------------------------------------------------

/// Thin Python wrapper around a Rust `Arc<dyn Provider>` loaded from WASM.
///
/// Bridges the Rust `Arc<dyn Provider>` trait object into Python's provider
/// protocol, so WASM providers can be mounted into a coordinator's
/// `mount_points["providers"]` dict alongside native Python provider modules.
///
/// Implements the Python Provider protocol: `name`, `get_info`, `list_models`,
/// `complete`, `parse_tool_calls`. Created automatically by
/// `load_and_mount_wasm()` when a WASM provider module is detected.
#[pyclass(name = "WasmProvider")]
struct PyWasmProvider {
    inner: Arc<dyn amplifier_core::traits::Provider>,
}

// Safety: Arc<dyn Provider> is Send+Sync (required by the Provider trait bound).
unsafe impl Send for PyWasmProvider {}
unsafe impl Sync for PyWasmProvider {}

#[pymethods]
impl PyWasmProvider {
    /// The provider's unique name (e.g., "openai").
    #[getter]
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// Return provider metadata as a Python dict.
    ///
    /// Serialises `ProviderInfo` through a JSON round-trip so the caller
    /// receives a plain Python dict with all fields.
    fn get_info(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let info = self.inner.get_info();
        let json_str = serde_json::to_string(&info).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize ProviderInfo: {e}"))
        })?;
        let json_mod = py.import("json")?;
        let dict = json_mod.call_method1("loads", (&json_str,))?;
        Ok(dict.unbind())
    }

    /// List models available from this provider.
    ///
    /// Async method — returns a coroutine that resolves to a list of dicts,
    /// each representing a `ModelInfo`.
    fn list_models<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let models = inner.list_models().await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("list_models failed: {e}"))
                })?;

                let json_str = serde_json::to_string(&models).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize model list: {e}"))
                })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let list = json_mod.call_method1("loads", (&json_str,))?;
                    Ok(list.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    /// Generate a completion from a chat request.
    ///
    /// Async method — takes a request (dict or Pydantic model), serialises it
    /// to a Rust `ChatRequest`, calls the inner provider, and returns the
    /// `ChatResponse` as a Python dict.
    fn complete<'py>(
        &self,
        py: Python<'py>,
        request: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        // Convert Python request to serde_json::Value
        let json_mod = py.import("json")?;
        let serializable = try_model_dump(&request);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let chat_request: amplifier_core::messages::ChatRequest = serde_json::from_str(&json_str)
            .map_err(|e| {
            PyErr::new::<PyValueError, _>(format!("Invalid ChatRequest JSON: {e}"))
        })?;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let response = inner.complete(chat_request).await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Provider complete failed: {e}"))
                })?;

                let result_json = serde_json::to_string(&response).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!(
                        "Failed to serialize ChatResponse: {e}"
                    ))
                })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let dict = json_mod.call_method1("loads", (&result_json,))?;
                    Ok(dict.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    /// Extract tool calls from a provider response.
    ///
    /// Sync method — takes a response (dict or Pydantic model), deserialises
    /// it as `ChatResponse`, calls `parse_tool_calls`, and returns a list of
    /// dicts representing `ToolCall` structs.
    fn parse_tool_calls(&self, py: Python<'_>, response: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let json_mod = py.import("json")?;
        let serializable = try_model_dump(&response);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let chat_response: amplifier_core::messages::ChatResponse = serde_json::from_str(&json_str)
            .map_err(|e| {
                PyErr::new::<PyValueError, _>(format!("Invalid ChatResponse JSON: {e}"))
            })?;

        let tool_calls = self.inner.parse_tool_calls(&chat_response);

        let result_json = serde_json::to_string(&tool_calls).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize tool calls: {e}"))
        })?;
        let list = json_mod.call_method1("loads", (&result_json,))?;
        Ok(list.unbind())
    }

    fn __repr__(&self) -> String {
        format!("<WasmProvider '{}'>", self.inner.name())
    }
}

// ---------------------------------------------------------------------------
// PyWasmHook — thin Python wrapper around a Rust Arc<dyn HookHandler>
// ---------------------------------------------------------------------------

/// Thin Python wrapper around a Rust `Arc<dyn HookHandler>` loaded from WASM.
///
/// Bridges the Rust `Arc<dyn HookHandler>` trait object into Python,
/// so WASM hook modules can be used from the Python session.
///
/// Implements the Python hook protocol: `handle(event, data)` (async).
/// Created automatically by `load_and_mount_wasm()` when a WASM hook
/// module is detected.
#[pyclass(name = "WasmHook")]
struct PyWasmHook {
    inner: Arc<dyn amplifier_core::traits::HookHandler>,
}

// Safety: Arc<dyn HookHandler> is Send+Sync (required by the HookHandler trait bound).
unsafe impl Send for PyWasmHook {}
unsafe impl Sync for PyWasmHook {}

#[pymethods]
impl PyWasmHook {
    /// Handle a hook event.
    ///
    /// Async method — takes an event name and data (dict or Pydantic model),
    /// serialises through JSON, calls the inner handler, and returns the
    /// `HookResult` as a Python dict.
    fn handle<'py>(
        &self,
        py: Python<'py>,
        event: String,
        data: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        let json_mod = py.import("json")?;
        let serializable = try_model_dump(&data);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str).map_err(|e| {
            PyErr::new::<PyValueError, _>(format!("Invalid JSON for hook data: {e}"))
        })?;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let result = inner.handle(&event, value).await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Hook handle failed: {e}"))
                })?;

                let result_json = serde_json::to_string(&result).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize HookResult: {e}"))
                })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let dict = json_mod.call_method1("loads", (&result_json,))?;
                    Ok(dict.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    fn __repr__(&self) -> String {
        "<WasmHook>".to_string()
    }
}

// ---------------------------------------------------------------------------
// PyWasmContext — thin Python wrapper around a Rust Arc<dyn ContextManager>
// ---------------------------------------------------------------------------

/// Thin Python wrapper around a Rust `Arc<dyn ContextManager>` loaded from WASM.
///
/// Bridges the Rust `Arc<dyn ContextManager>` trait object into Python's
/// context protocol, so WASM context modules can be mounted into a
/// coordinator's `mount_points["context"]` slot.
///
/// Implements the Python context protocol: `add_message`, `get_messages`,
/// `get_messages_for_request`, `set_messages`, `clear`. Created automatically
/// by `load_and_mount_wasm()` when a WASM context module is detected.
#[pyclass(name = "WasmContext")]
struct PyWasmContext {
    inner: Arc<dyn amplifier_core::traits::ContextManager>,
}

// Safety: Arc<dyn ContextManager> is Send+Sync (required by the ContextManager trait bound).
unsafe impl Send for PyWasmContext {}
unsafe impl Sync for PyWasmContext {}

#[pymethods]
impl PyWasmContext {
    /// Append a message to the context history.
    ///
    /// Async method — takes a message (dict or Pydantic model), serialises
    /// through JSON, and calls the inner context manager.
    fn add_message<'py>(
        &self,
        py: Python<'py>,
        message: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        let json_mod = py.import("json")?;
        let serializable = try_model_dump(&message);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let value: Value = serde_json::from_str(&json_str)
            .map_err(|e| PyErr::new::<PyValueError, _>(format!("Invalid JSON for message: {e}")))?;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                inner.add_message(value).await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("add_message failed: {e}"))
                })?;
                Python::try_attach(|py| -> PyResult<Py<PyAny>> { Ok(py.None()) }).ok_or_else(
                    || PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime"),
                )?
            }),
        )
    }

    /// Get all messages (raw, uncompacted).
    ///
    /// Async method — returns a coroutine that resolves to a list of dicts.
    fn get_messages<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let messages = inner.get_messages().await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("get_messages failed: {e}"))
                })?;

                let json_str = serde_json::to_string(&messages).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize messages: {e}"))
                })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let list = json_mod.call_method1("loads", (&json_str,))?;
                    Ok(list.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    /// Get messages ready for an LLM request, compacted if necessary.
    ///
    /// Async method — takes an optional request dict (currently ignores
    /// token_budget and provider for WASM context managers), and returns
    /// a list of message dicts.
    fn get_messages_for_request<'py>(
        &self,
        py: Python<'py>,
        _request: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // WASM context managers don't receive provider/budget yet —
                // pass None for both parameters.
                let messages = inner
                    .get_messages_for_request(None, None)
                    .await
                    .map_err(|e| {
                        PyErr::new::<PyRuntimeError, _>(format!(
                            "get_messages_for_request failed: {e}"
                        ))
                    })?;

                let json_str = serde_json::to_string(&messages).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("Failed to serialize messages: {e}"))
                })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let list = json_mod.call_method1("loads", (&json_str,))?;
                    Ok(list.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    /// Replace the entire message list.
    ///
    /// Async method — takes a list of message dicts, serialises through JSON,
    /// and calls the inner context manager.
    fn set_messages<'py>(
        &self,
        py: Python<'py>,
        messages: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        let json_mod = py.import("json")?;
        let json_str: String = json_mod.call_method1("dumps", (&messages,))?.extract()?;
        let values: Vec<Value> = serde_json::from_str(&json_str).map_err(|e| {
            PyErr::new::<PyValueError, _>(format!("Invalid JSON for messages: {e}"))
        })?;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                inner.set_messages(values).await.map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!("set_messages failed: {e}"))
                })?;
                Python::try_attach(|py| -> PyResult<Py<PyAny>> { Ok(py.None()) }).ok_or_else(
                    || PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime"),
                )?
            }),
        )
    }

    /// Clear all messages from context.
    ///
    /// Async method — returns a coroutine that resolves to None.
    fn clear<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                inner
                    .clear()
                    .await
                    .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("clear failed: {e}")))?;
                Python::try_attach(|py| -> PyResult<Py<PyAny>> { Ok(py.None()) }).ok_or_else(
                    || PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime"),
                )?
            }),
        )
    }

    fn __repr__(&self) -> String {
        "<WasmContext>".to_string()
    }
}

// ---------------------------------------------------------------------------
// PyWasmOrchestrator — thin Python wrapper around a Rust Arc<dyn Orchestrator>
// ---------------------------------------------------------------------------

/// Thin Python wrapper around a Rust `Arc<dyn Orchestrator>` loaded from WASM.
///
/// Bridges the Rust `Arc<dyn Orchestrator>` trait object into Python's
/// orchestrator protocol, so WASM orchestrator modules can be mounted
/// into a coordinator's `mount_points["orchestrator"]` slot.
///
/// Implements the Python orchestrator protocol: `execute(prompt, ...)` (async).
/// Created automatically by `load_and_mount_wasm()` when a WASM orchestrator
/// module is detected.
#[pyclass(name = "WasmOrchestrator")]
struct PyWasmOrchestrator {
    inner: Arc<dyn amplifier_core::traits::Orchestrator>,
}

// Safety: Arc<dyn Orchestrator> is Send+Sync (required by the Orchestrator trait bound).
unsafe impl Send for PyWasmOrchestrator {}
unsafe impl Sync for PyWasmOrchestrator {}

#[pymethods]
impl PyWasmOrchestrator {
    /// Execute the WASM orchestrator with a prompt.
    ///
    /// # Why all 6 parameters are accepted
    ///
    /// `_session_exec.run_orchestrator()` always passes all 6 keyword arguments
    /// (`prompt`, `context`, `providers`, `tools`, `hooks`, `coordinator`) to
    /// every orchestrator — Python and WASM alike.  If this method's signature
    /// did not accept them, Python would raise `TypeError: execute() got an
    /// unexpected keyword argument …` at call time.
    ///
    /// # Why 5 parameters are discarded
    ///
    /// WASM guests cannot receive arbitrary Python objects across the sandbox
    /// boundary.  Instead, they access kernel services (context, providers,
    /// tools, hooks, coordinator) via **`kernel-service` host imports** defined
    /// in the WIT interface.  The Python-side objects are therefore accepted
    /// here solely for signature compatibility and then dropped.
    ///
    /// # Future enhancement
    ///
    /// Forward relevant session state (e.g. context messages, tool manifests)
    /// to WASM guests by plumbing them through the `kernel-service` host
    /// imports, so that WASM orchestrators can interact with the same kernel
    /// services available to Python orchestrators.
    #[pyo3(signature = (prompt, context=None, providers=None, tools=None, hooks=None, coordinator=None))]
    #[allow(clippy::too_many_arguments)]
    fn execute<'py>(
        &self,
        py: Python<'py>,
        prompt: String,
        context: Option<Bound<'py, PyAny>>,
        providers: Option<Bound<'py, PyAny>>,
        tools: Option<Bound<'py, PyAny>>,
        hooks: Option<Bound<'py, PyAny>>,
        coordinator: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        // Protocol conformance: these params are required by the unified dispatch
        // path in `_session_exec.run_orchestrator()` which always passes all 6
        // keyword arguments.  WASM guests access kernel services (context,
        // providers, tools, hooks, coordinator) via host imports defined in the
        // WIT `kernel-service` interface, not via Python parameters.
        let _ = (context, providers, tools, hooks, coordinator);

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Provide minimal defaults for the required trait parameters.
                // WASM orchestrators currently only use `prompt`.
                let empty_context: Arc<dyn amplifier_core::traits::ContextManager> =
                    Arc::new(NullContextManager);
                let empty_providers: HashMap<String, Arc<dyn amplifier_core::traits::Provider>> =
                    HashMap::new();
                let empty_tools: HashMap<String, Arc<dyn amplifier_core::traits::Tool>> =
                    HashMap::new();
                let null_hooks = Value::Null;
                let null_coordinator = Value::Null;

                let result = inner
                    .execute(
                        prompt,
                        empty_context,
                        empty_providers,
                        empty_tools,
                        null_hooks,
                        null_coordinator,
                    )
                    .await
                    .map_err(|e| {
                        PyErr::new::<PyRuntimeError, _>(format!("Orchestrator execute failed: {e}"))
                    })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    Ok(result.into_pyobject(py)?.into_any().unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    fn __repr__(&self) -> String {
        "<WasmOrchestrator>".to_string()
    }
}

/// Minimal no-op context manager used as a placeholder when calling WASM
/// orchestrators that don't actually use the context parameter.
struct NullContextManager;

impl amplifier_core::traits::ContextManager for NullContextManager {
    fn add_message(
        &self,
        _message: Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), amplifier_core::ContextError>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }

    fn get_messages_for_request(
        &self,
        _token_budget: Option<i64>,
        _provider: Option<Arc<dyn amplifier_core::traits::Provider>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, amplifier_core::ContextError>> + Send + '_>>
    {
        Box::pin(async { Ok(vec![]) })
    }

    fn get_messages(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, amplifier_core::ContextError>> + Send + '_>>
    {
        Box::pin(async { Ok(vec![]) })
    }

    fn set_messages(
        &self,
        _messages: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), amplifier_core::ContextError>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }

    fn clear(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<(), amplifier_core::ContextError>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

// ---------------------------------------------------------------------------
// PyWasmApproval — thin Python wrapper around a Rust Arc<dyn ApprovalProvider>
// ---------------------------------------------------------------------------

/// Thin Python wrapper around a Rust `Arc<dyn ApprovalProvider>` loaded from WASM.
///
/// Bridges the Rust `Arc<dyn ApprovalProvider>` trait object into Python,
/// so WASM approval modules can be used from the Python session.
///
/// Implements the Python approval protocol: `request_approval(request)` (async).
/// Created automatically by `load_and_mount_wasm()` when a WASM approval
/// module is detected.
#[pyclass(name = "WasmApproval")]
struct PyWasmApproval {
    inner: Arc<dyn amplifier_core::traits::ApprovalProvider>,
}

// Safety: Arc<dyn ApprovalProvider> is Send+Sync (required by the ApprovalProvider trait bound).
unsafe impl Send for PyWasmApproval {}
unsafe impl Sync for PyWasmApproval {}

#[pymethods]
impl PyWasmApproval {
    /// Request approval for an action.
    ///
    /// Async method — takes a request (dict or Pydantic model), deserialises
    /// it as `ApprovalRequest`, calls the inner approval provider, and returns
    /// the `ApprovalResponse` as a Python dict.
    fn request_approval<'py>(
        &self,
        py: Python<'py>,
        request: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        let json_mod = py.import("json")?;
        let serializable = try_model_dump(&request);
        let json_str: String = json_mod
            .call_method1("dumps", (&serializable,))?
            .extract()?;
        let approval_request: amplifier_core::models::ApprovalRequest =
            serde_json::from_str(&json_str).map_err(|e| {
                PyErr::new::<PyValueError, _>(format!("Invalid ApprovalRequest JSON: {e}"))
            })?;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let response = inner
                    .request_approval(approval_request)
                    .await
                    .map_err(|e| {
                        PyErr::new::<PyRuntimeError, _>(format!("request_approval failed: {e}"))
                    })?;

                let result_json = serde_json::to_string(&response).map_err(|e| {
                    PyErr::new::<PyRuntimeError, _>(format!(
                        "Failed to serialize ApprovalResponse: {e}"
                    ))
                })?;

                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                    let json_mod = py.import("json")?;
                    let dict = json_mod.call_method1("loads", (&result_json,))?;
                    Ok(dict.unbind())
                })
                .ok_or_else(|| {
                    PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime")
                })?
            }),
        )
    }

    fn __repr__(&self) -> String {
        "<WasmApproval>".to_string()
    }
}

// ---------------------------------------------------------------------------
// load_and_mount_wasm — load WASM module and mount into a real coordinator
// ---------------------------------------------------------------------------

/// Load a WASM module from a filesystem path and mount it into a coordinator.
///
/// Unlike `load_wasm_from_path` (which loads into a throwaway test coordinator),
/// this function mounts the loaded module directly into the given coordinator's
/// Python-visible `mount_points` dict, making it available for orchestrator use.
///
/// Currently supports mounting:
/// - **tool** modules → `mount_points["tools"][name]` as a `WasmTool` wrapper
/// - Other module types are loaded and validated, returning their info for
///   Python-side mounting (hooks are registered differently, etc.)
///
/// Returns a dict with:
/// - `"status"`: `"mounted"` if mounted, `"loaded"` if loaded but not auto-mounted
/// - `"module_type"`: the detected module type string
/// - `"name"`: the module name (for tool modules)
///
/// # Errors
///
/// Returns `ValueError` if the path doesn't contain a WASM module.
/// Returns `RuntimeError` if engine creation or module loading fails.
#[pyfunction]
fn load_and_mount_wasm(
    py: Python<'_>,
    coordinator: &PyCoordinator,
    path: String,
) -> PyResult<Py<PyDict>> {
    let manifest = amplifier_core::module_resolver::resolve_module(std::path::Path::new(&path))
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("{e}")))?;

    if manifest.transport != amplifier_core::transport::Transport::Wasm {
        return Err(PyErr::new::<PyValueError, _>(format!(
            "load_and_mount_wasm only handles WASM modules, got transport '{:?}'",
            manifest.transport
        )));
    }

    let engine = amplifier_core::wasm_engine::WasmEngine::new().map_err(|e| {
        PyErr::new::<PyRuntimeError, _>(format!("WASM engine creation failed: {e}"))
    })?;

    // Use the real coordinator's inner Arc<Coordinator> for orchestrator modules
    let rust_coordinator = coordinator.inner.clone();
    let loaded = amplifier_core::module_resolver::load_module(
        &manifest,
        engine.inner(),
        Some(rust_coordinator),
    )
    .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Module loading failed: {e}")))?;

    let dict = PyDict::new(py);
    dict.set_item("module_type", loaded.variant_name())?;

    match loaded {
        amplifier_core::module_resolver::LoadedModule::Tool(tool) => {
            let tool_name = tool.name().to_string();
            // Wrap in PyWasmTool and mount into coordinator's mount_points["tools"]
            let wrapper = Py::new(py, PyWasmTool { inner: tool })?;
            let mp = coordinator.mount_points.bind(py);
            let tools_any = mp
                .get_item("tools")?
                .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("mount_points missing 'tools'"))?;
            let tools_dict = tools_any.cast::<PyDict>()?;
            tools_dict.set_item(&tool_name, &wrapper)?;
            dict.set_item("status", "mounted")?;
            dict.set_item("name", &tool_name)?;
        }
        amplifier_core::module_resolver::LoadedModule::PythonDelegated { package_name } => {
            // Signal to caller: this is a Python module, handle via importlib
            dict.set_item("status", "delegate_to_python")?;
            dict.set_item("package_name", package_name)?;
        }
        amplifier_core::module_resolver::LoadedModule::Provider(provider) => {
            let provider_name = provider.name().to_string();
            // Wrap in PyWasmProvider and mount into coordinator's mount_points["providers"]
            let wrapper = Py::new(py, PyWasmProvider { inner: provider })?;
            let mp = coordinator.mount_points.bind(py);
            let providers_any = mp.get_item("providers")?.ok_or_else(|| {
                PyErr::new::<PyRuntimeError, _>("mount_points missing 'providers'")
            })?;
            let providers_dict = providers_any.cast::<PyDict>()?;
            providers_dict.set_item(&provider_name, &wrapper)?;
            dict.set_item("status", "mounted")?;
            dict.set_item("name", &provider_name)?;
        }
        amplifier_core::module_resolver::LoadedModule::Hook(hook) => {
            // Register the WASM hook with the coordinator's Rust-side hook
            // registry so it participates in `emit()` dispatch.
            //
            // Ask the module which events it wants to subscribe to via the
            // `HookHandler::get_subscriptions` trait method.  WASM modules
            // compiled with the current WIT return their declared subscriptions;
            // old modules without `get-subscriptions` fall back to a wildcard
            // subscription inside `WasmHookBridge::get_subscriptions()`.
            //
            // NOTE: `GrpcHookBridge` uses the trait default (wildcard) here.
            // Its async `get_subscriptions` RPC with UNIMPLEMENTED fallback is
            // invoked through a separate async registration path for gRPC hooks.
            let config = serde_json::json!({});
            let subscriptions_result: Vec<(String, i32, String)> = hook.get_subscriptions(&config);

            let hooks_registry = coordinator.inner.hooks_shared();
            for (event, priority, name) in &subscriptions_result {
                let _ = hooks_registry.register(event, hook.clone(), *priority, Some(name.clone()));
            }

            dict.set_item("status", "mounted")?;
            dict.set_item("subscriptions_count", subscriptions_result.len())?;
        }
        amplifier_core::module_resolver::LoadedModule::Context(context) => {
            // Wrap in PyWasmContext and mount into coordinator's mount_points["context"]
            let wrapper = Py::new(py, PyWasmContext { inner: context })?;
            let mp = coordinator.mount_points.bind(py);
            mp.set_item("context", &wrapper)?;
            dict.set_item("status", "mounted")?;
        }
        amplifier_core::module_resolver::LoadedModule::Orchestrator(orchestrator) => {
            // Wrap in PyWasmOrchestrator and mount into coordinator's mount_points["orchestrator"]
            log::warn!(
                "WASM orchestrator mounted — context/providers/tools/hooks/coordinator \
                 are not forwarded to WASM guests in this version. \
                 The WASM guest accesses kernel services via host imports instead."
            );
            let wrapper = Py::new(
                py,
                PyWasmOrchestrator {
                    inner: orchestrator,
                },
            )?;
            let mp = coordinator.mount_points.bind(py);
            mp.set_item("orchestrator", &wrapper)?;
            dict.set_item("status", "mounted")?;
        }
        amplifier_core::module_resolver::LoadedModule::Approval(approval) => {
            // Wrap in PyWasmApproval — returned to caller for use
            let wrapper = Py::new(py, PyWasmApproval { inner: approval })?;
            dict.set_item("status", "loaded")?;
            dict.set_item("wrapper", wrapper)?;
        }
    }

    Ok(dict.unbind())
}

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
