//! Capability registration and contribution channels for PyCoordinator.
//!
//! Contains methods for inter-module communication: capability registry,
//! cleanup function registration, and contribution channel management.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::helpers::wrap_future_as_coroutine;

use super::PyCoordinator;

#[pymethods]
impl PyCoordinator {
    // -----------------------------------------------------------------------
    // register_capability / get_capability
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
    // register_cleanup
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // register_contributor / collect_contributions / channels
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
        let list_any = channels.get_item(channel)?.ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Channel list missing after insert: {}",
                channel
            ))
        })?;
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
            // Fallback: sync-only collection via Rust.
            // This path is taken when the package is installed without the Python
            // helper module (e.g. partial install, missing __init__ re-export).
            // Async contributor callbacks will NOT be awaited in this path.
            log::warn!(
                "amplifier_core._collect_helper not available — \
                 collect_contributions using sync-only fallback for channel '{}'. \
                 Async contributor callbacks will be skipped.",
                channel
            );
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

    /// Contribution channels dict.
    #[getter]
    fn channels<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        self.channels_dict.bind(py).clone()
    }
}
