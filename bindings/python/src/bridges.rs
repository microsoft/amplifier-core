// ---------------------------------------------------------------------------
// Trait bridges — adapt Python callables into Rust trait objects
// ---------------------------------------------------------------------------

use std::future::Future;
use std::pin::Pin;

use pyo3::prelude::*;
use serde_json::Value;

use amplifier_core::errors::{AmplifierError, HookError, SessionError};
use amplifier_core::models::{HookAction, HookResult};
use amplifier_core::traits::HookHandler;

use crate::helpers::try_model_dump;

// ---------------------------------------------------------------------------
// PyHookHandlerBridge — wraps a Python callable as a Rust HookHandler
// ---------------------------------------------------------------------------

/// Bridges a Python callable into the Rust [`HookHandler`] trait.
///
/// Stores a `Py<PyAny>` (the Python callable) and calls it via the GIL
/// when `handle()` is invoked. The callable should accept `(event, data)`
/// and return a dict (or None for a default continue result).
pub(crate) struct PyHookHandlerBridge {
    pub(crate) callable: Py<PyAny>,
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

        Box::pin(async move {
            // Clone the Py<PyAny> reference inside the GIL to safely use in this async block
            let callable = Python::try_attach(|py| Ok::<_, PyErr>(self.callable.clone_ref(py)))
                .ok_or_else(|| HookError::HandlerFailed {
                    message: "Failed to attach to Python runtime".to_string(),
                    handler_name: None,
                })?
                .map_err(|e| HookError::HandlerFailed {
                    message: format!("Failed to clone Python callable reference: {e}"),
                    handler_name: None,
                })?;

            // Step 1: Call the Python handler (inside GIL) — returns either a
            // sync result or a coroutine object, plus whether it's a coroutine.
            let (is_coro, py_result_or_coro) =
                Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
                    let json_mod = py.import("json")?;
                    let data_str = serde_json::to_string(&data).unwrap_or_else(|e| {
                        log::warn!(
                            "Failed to serialize hook data to JSON (using empty object): {e}"
                        );
                        "{}".to_string()
                    });
                    let py_data = json_mod.call_method1("loads", (&data_str,))?;

                    let call_result = callable.call(py, (&event, py_data), None)?;
                    let bound = call_result.bind(py);

                    // Check if the result is a coroutine (async handler)
                    let inspect = py.import("inspect")?;
                    let is_coro: bool = inspect.call_method1("iscoroutine", (bound,))?.extract()?;

                    Ok((is_coro, call_result))
                })
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
            //
            // Python hook handlers typically return Pydantic BaseModel instances
            // (e.g. amplifier_core.models.HookResult). stdlib json.dumps() cannot
            // serialize Pydantic models directly, so we first try model_dump() to
            // convert to a plain dict, then json.dumps() the dict. For non-Pydantic
            // return values (plain dicts, etc.) we fall back to json.dumps() directly.
            let result_json: String = Python::try_attach(|py| -> PyResult<String> {
                let bound = py_result.bind(py);
                if bound.is_none() {
                    return Ok("{}".to_string());
                }
                let json_mod = py.import("json")?;
                let serializable = try_model_dump(bound);
                let json_str: String = json_mod
                    .call_method1("dumps", (&serializable,))?
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

            let hook_result: HookResult = serde_json::from_str(&result_json).unwrap_or_else(|e| {
                log::error!(
                    "SECURITY: Hook handler returned unparseable result — failing closed (Deny): {e} — json: {result_json}"
                );
                HookResult {
                    action: HookAction::Deny,
                    reason: Some("Hook handler returned invalid response".to_string()),
                    ..Default::default()
                }
            });
            Ok(hook_result)
        })
    }
}

// ---------------------------------------------------------------------------
// PyApprovalProviderBridge — wraps a Python ApprovalSystem as a Rust ApprovalProvider
// ---------------------------------------------------------------------------

/// Bridges a Python `ApprovalSystem` object into the Rust [`ApprovalProvider`] trait.
///
/// The Python `ApprovalSystem` protocol has:
///   `request_approval(prompt, options, timeout, default) -> str`
///
/// The Rust `ApprovalProvider` trait has:
///   `request_approval(ApprovalRequest) -> Result<ApprovalResponse, AmplifierError>`
///
/// This bridge adapts between the two interfaces.
pub(crate) struct PyApprovalProviderBridge {
    pub(crate) py_obj: Py<PyAny>,
}

// Safety: Py<PyAny> is Send+Sync (PyO3 handles GIL acquisition).
unsafe impl Send for PyApprovalProviderBridge {}
unsafe impl Sync for PyApprovalProviderBridge {}

impl amplifier_core::traits::ApprovalProvider for PyApprovalProviderBridge {
    fn request_approval(
        &self,
        request: amplifier_core::models::ApprovalRequest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        amplifier_core::models::ApprovalResponse,
                        amplifier_core::errors::AmplifierError,
                    >,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            // Clone the Py<PyAny> reference inside the GIL to safely use in this async block
            let py_obj = Python::try_attach(|py| Ok::<_, PyErr>(self.py_obj.clone_ref(py)))
                .ok_or_else(|| {
                    AmplifierError::Session(SessionError::Other {
                        message: "Failed to attach to Python runtime".to_string(),
                    })
                })?
                .map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!("Failed to clone Python object reference: {e}"),
                    })
                })?;

            // Step 1: Build Python call args from the ApprovalRequest
            let (is_coro, py_result_or_coro) =
                Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
                    // Adapt Rust ApprovalRequest to Python ApprovalSystem.request_approval() args:
                    //   prompt: str, options: list[str], timeout: float, default: str
                    let prompt = format!("{}: {}", request.tool_name, request.action);
                    let options = vec!["approve", "deny"];
                    let timeout = request.timeout.unwrap_or(300.0);
                    let default = "deny";

                    let call_result = py_obj.call_method(
                        py,
                        "request_approval",
                        (prompt, options, timeout, default),
                        None,
                    )?;
                    let bound = call_result.bind(py);

                    let inspect = py.import("inspect")?;
                    let is_coro: bool = inspect.call_method1("iscoroutine", (bound,))?.extract()?;

                    Ok((is_coro, call_result))
                })
                .ok_or_else(|| {
                    AmplifierError::Session(SessionError::Other {
                        message: "Failed to attach to Python runtime".to_string(),
                    })
                })?
                .map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!("Python approval call error: {e}"),
                    })
                })?;

            // Step 2: Await if coroutine
            let py_result: Py<PyAny> = if is_coro {
                let future = Python::try_attach(|py| {
                    pyo3_async_runtimes::tokio::into_future(py_result_or_coro.into_bound(py))
                })
                .ok_or_else(|| {
                    AmplifierError::Session(SessionError::Other {
                        message: "Failed to attach for coroutine conversion".to_string(),
                    })
                })?
                .map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!("Coroutine conversion error: {e}"),
                    })
                })?;
                future.await.map_err(|e| {
                    AmplifierError::Session(SessionError::Other {
                        message: format!("Async approval error: {e}"),
                    })
                })?
            } else {
                py_result_or_coro
            };

            // Step 3: Parse result string → ApprovalResponse
            let approved = Python::try_attach(|py| -> PyResult<bool> {
                let result_str: String = py_result.extract(py)?;
                Ok(result_str.to_lowercase().contains("approve"))
            })
            .ok_or_else(|| {
                AmplifierError::Session(SessionError::Other {
                    message: "Failed to attach to parse approval result".to_string(),
                })
            })?
            .map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("Failed to parse approval result: {e}"),
                })
            })?;

            Ok(amplifier_core::models::ApprovalResponse {
                approved,
                reason: None,
                remember: false,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// PyDisplayServiceBridge — wraps a Python DisplaySystem as a Rust DisplayService
// ---------------------------------------------------------------------------

/// Bridges a Python `DisplaySystem` object into the Rust [`DisplayService`] trait.
///
/// The Python `DisplaySystem` protocol has:
///   `show_message(message, level, source)`
///
/// The Rust `DisplayService` trait has:
///   `show_message(&self, message: &str, level: &str, source: &str) -> Pin<Box<...>>`
///
/// Display is fire-and-forget — errors are logged but do not propagate.
pub(crate) struct PyDisplayServiceBridge {
    pub(crate) py_obj: Py<PyAny>,
}

// Safety: Py<PyAny> is Send+Sync (PyO3 handles GIL acquisition).
unsafe impl Send for PyDisplayServiceBridge {}
unsafe impl Sync for PyDisplayServiceBridge {}

impl amplifier_core::traits::DisplayService for PyDisplayServiceBridge {
    fn show_message(
        &self,
        message: &str,
        level: &str,
        source: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), AmplifierError>> + Send + '_>> {
        let message = message.to_string();
        let level = level.to_string();
        let source = source.to_string();
        let py_obj = Python::try_attach(|py| self.py_obj.clone_ref(py));

        Box::pin(async move {
            let py_obj = py_obj.ok_or_else(|| {
                AmplifierError::Session(SessionError::Other {
                    message: "Failed to attach to Python runtime for display (clone)".to_string(),
                })
            })?;

            Python::try_attach(|py| -> PyResult<()> {
                py_obj.call_method(py, "show_message", (&message, &level, &source), None)?;
                Ok(())
            })
            .ok_or_else(|| {
                AmplifierError::Session(SessionError::Other {
                    message: "Failed to attach to Python runtime for display (call)".to_string(),
                })
            })?
            .map_err(|e| {
                AmplifierError::Session(SessionError::Other {
                    message: format!("Python display call error: {e}"),
                })
            })
        })
    }
}

// ---------------------------------------------------------------------------
// PyContextManagerBridge — wraps a Python context manager as a Rust-callable bridge
// ---------------------------------------------------------------------------

/// Bridges a Python context manager object (with `add_message`) into Rust.
///
/// The Python object must implement:
///   `add_message(message_dict)` — either sync or async
///
/// This bridge handles both sync and async Python `add_message` implementations.
pub(crate) struct PyContextManagerBridge {
    pub(crate) py_obj: Py<PyAny>,
}

// Safety: Py<PyAny> is Send+Sync (PyO3 handles GIL acquisition).
unsafe impl Send for PyContextManagerBridge {}
unsafe impl Sync for PyContextManagerBridge {}

impl PyContextManagerBridge {
    /// Call `add_message` on the wrapped Python object.
    ///
    /// Handles both sync and async Python implementations transparently.
    pub(crate) async fn add_message(&self, message: Py<PyAny>) -> Result<(), PyErr> {
        // Step 1: Call add_message on py_obj inside the GIL.
        // Check if the result is a coroutine (async implementation).
        let inner: PyResult<(bool, Py<PyAny>)> =
            Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
                let call_result = self
                    .py_obj
                    .call_method(py, "add_message", (message,), None)?;
                let bound = call_result.bind(py);

                // Check if the result is a coroutine (async handler)
                let inspect = py.import("inspect")?;
                let is_coro: bool = inspect.call_method1("iscoroutine", (bound,))?.extract()?;

                Ok((is_coro, call_result))
            })
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Failed to attach to Python runtime",
                )
            })?;

        let (is_coro, py_result_or_coro) = inner?;

        // Step 2: If it's a coroutine, await it outside the GIL via into_future.
        // This drives the coroutine on the caller's event loop.
        if is_coro {
            let inner_fut: PyResult<_> = Python::try_attach(|py| {
                pyo3_async_runtimes::tokio::into_future(py_result_or_coro.into_bound(py))
            })
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Failed to attach to Python runtime for coroutine conversion",
                )
            })?;

            let future = inner_fut?;
            let _ = future.await?;
        }

        Ok(())
    }
}
