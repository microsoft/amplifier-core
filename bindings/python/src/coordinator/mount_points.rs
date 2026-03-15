//! Mount-point management for PyCoordinator.
//!
//! Contains all methods related to module storage, retrieval, turn tracking,
//! and system property access (approval, display, hooks, cancellation, loader).

use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::bridges::{PyApprovalProviderBridge, PyDisplayServiceBridge};
use crate::cancellation::PyCancellationToken;
use crate::helpers::wrap_future_as_coroutine;

use super::PyCoordinator;

#[pymethods]
impl PyCoordinator {
    // -----------------------------------------------------------------------
    // mount_points property
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
    // mount() and get()
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
            "providers" | "tools" => {
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
            "providers" | "tools" => {
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
    // unmount()
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
            "providers" | "tools" => {
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
    // loader, approval_system, display_system properties
    // -----------------------------------------------------------------------

    /// Module loader (Python object or None).
    #[getter]
    fn loader<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        // clone_ref on a Py<PyAny> holding Python None returns None —
        // the is_none() guard is redundant.
        self.loader_obj.clone_ref(py)
    }

    /// Set the module loader.
    #[setter]
    fn set_loader(&mut self, value: Py<PyAny>) {
        self.loader_obj = value;
    }

    /// Approval system (Python object or None).
    #[getter]
    fn approval_system<'py>(&self, py: Python<'py>) -> Py<PyAny> {
        // clone_ref on a Py<PyAny> holding Python None returns None —
        // the is_none() guard is redundant.
        self.approval_system_obj.clone_ref(py)
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
        // clone_ref on a Py<PyAny> holding Python None returns None —
        // the is_none() guard is redundant.
        self.display_system_obj.clone_ref(py)
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
    // hooks and cancellation properties
    // -----------------------------------------------------------------------

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
    // request_cancel / reset_turn
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
    // _current_turn_injections
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
    // injection_budget_per_turn / injection_size_limit
    // -----------------------------------------------------------------------

    /// Injection budget per turn from session config (policy).
    ///
    /// Returns int or None. Matches Python `ModuleCoordinator.injection_budget_per_turn`.
    #[getter]
    fn injection_budget_per_turn<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        self.get_injection_budget_per_turn(py)
    }

    /// Per-injection size limit from session config (policy).
    ///
    /// Returns int or None. Matches Python `ModuleCoordinator.injection_size_limit`.
    #[getter]
    fn injection_size_limit<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        self.get_injection_size_limit(py)
    }
}

// ---------------------------------------------------------------------------
// Crate-private accessors for injection policy values.
// The #[getter] methods above delegate to these so Rust callers (e.g.
// hook_dispatch.rs) have a single canonical implementation.
// ---------------------------------------------------------------------------

impl PyCoordinator {
    /// Injection budget per turn from session config. Returns `None` or an int.
    pub(crate) fn get_injection_budget_per_turn<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Py<PyAny>> {
        let config = self.config_dict.bind(py);
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

    /// Per-injection size limit from session config. Returns `None` or an int.
    pub(crate) fn get_injection_size_limit<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
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
}
