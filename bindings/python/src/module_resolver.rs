// ---------------------------------------------------------------------------
// Module resolver bindings — resolve_module, load_wasm_from_path
// ---------------------------------------------------------------------------

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Resolve a module from a filesystem path.
///
/// Returns a dict with keys: "transport", "module_type", "artifact_type",
/// and artifact-specific keys ("artifact_path", "endpoint", "package_name").
#[pyfunction]
pub(crate) fn resolve_module(py: Python<'_>, path: String) -> PyResult<Py<PyDict>> {
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
pub(crate) fn load_wasm_from_path(py: Python<'_>, path: String) -> PyResult<Py<PyDict>> {
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
