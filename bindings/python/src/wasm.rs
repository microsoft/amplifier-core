// ---------------------------------------------------------------------------
// WASM module wrappers — PyWasmTool, PyWasmProvider, PyWasmHook,
// PyWasmContext, PyWasmOrchestrator, PyWasmApproval, load_and_mount_wasm
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

use crate::coordinator::PyCoordinator;
use crate::helpers::{try_model_dump, wrap_future_as_coroutine};

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
pub(crate) struct PyWasmTool {
    pub(crate) inner: Arc<dyn amplifier_core::traits::Tool>,
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
pub(crate) struct PyWasmProvider {
    pub(crate) inner: Arc<dyn amplifier_core::traits::Provider>,
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
pub(crate) struct PyWasmHook {
    pub(crate) inner: Arc<dyn amplifier_core::traits::HookHandler>,
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
pub(crate) struct PyWasmContext {
    pub(crate) inner: Arc<dyn amplifier_core::traits::ContextManager>,
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
pub(crate) struct PyWasmOrchestrator {
    pub(crate) inner: Arc<dyn amplifier_core::traits::Orchestrator>,
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
pub(crate) struct NullContextManager;

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
pub(crate) struct PyWasmApproval {
    pub(crate) inner: Arc<dyn amplifier_core::traits::ApprovalProvider>,
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
pub(crate) fn load_and_mount_wasm(
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
