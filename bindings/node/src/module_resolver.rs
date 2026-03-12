// ---------------------------------------------------------------------------
// Module resolver bindings (Phase 4)
// ---------------------------------------------------------------------------

use napi::bindgen_prelude::*;

/// Result from resolving a module path.
#[napi(object)]
pub struct JsModuleManifest {
    /// How the module is loaded and invoked.
    ///
    /// Valid values (string literal union):
    /// `"python"` | `"wasm"` | `"grpc"` | `"native"`
    pub transport: String,

    /// Logical role the module plays inside the kernel.
    ///
    /// Valid values (string literal union):
    /// `"tool"` | `"hook"` | `"context"` | `"approval"` | `"provider"` | `"orchestrator"`
    pub module_type: String,

    /// Artifact format used to locate or load the module.
    ///
    /// Valid values (string literal union):
    /// `"wasm"` | `"grpc"` | `"python"`
    ///
    /// - `"wasm"` — `artifactPath` contains the `.wasm` component file path
    /// - `"grpc"` — `endpoint` contains the gRPC service URL
    /// - `"python"` — `packageName` contains the importable Python package name
    pub artifact_type: String,

    /// Path to WASM artifact (present when `artifactType` is `"wasm"`).
    pub artifact_path: Option<String>,

    /// gRPC service endpoint URL (present when `artifactType` is `"grpc"`).
    pub endpoint: Option<String>,

    /// Python package name for import (present when `artifactType` is `"python"`).
    pub package_name: Option<String>,
}

/// Resolve a module from a filesystem path.
///
/// Returns a JsModuleManifest describing the transport, module type, and artifact.
#[napi]
pub fn resolve_module(path: String) -> Result<JsModuleManifest> {
    let manifest = amplifier_core::module_resolver::resolve_module(std::path::Path::new(&path))
        .map_err(|e| Error::from_reason(format!("{e}")))?;

    let transport = match manifest.transport {
        amplifier_core::transport::Transport::Python => "python",
        amplifier_core::transport::Transport::Wasm => "wasm",
        amplifier_core::transport::Transport::Grpc => "grpc",
        amplifier_core::transport::Transport::Native => "native",
    };

    let module_type = match manifest.module_type {
        amplifier_core::models::ModuleType::Tool => "tool",
        amplifier_core::models::ModuleType::Hook => "hook",
        amplifier_core::models::ModuleType::Context => "context",
        amplifier_core::models::ModuleType::Approval => "approval",
        amplifier_core::models::ModuleType::Provider => "provider",
        amplifier_core::models::ModuleType::Orchestrator => "orchestrator",
        amplifier_core::models::ModuleType::Resolver => "resolver",
    };

    let (artifact_type, artifact_path, endpoint, package_name) = match &manifest.artifact {
        amplifier_core::module_resolver::ModuleArtifact::WasmPath(path) => {
            ("wasm", Some(path.to_string_lossy().to_string()), None, None)
        }
        amplifier_core::module_resolver::ModuleArtifact::WasmBytes { path, .. } => {
            ("wasm", Some(path.to_string_lossy().to_string()), None, None)
        }
        amplifier_core::module_resolver::ModuleArtifact::GrpcEndpoint(ep) => {
            ("grpc", None, Some(ep.clone()), None)
        }
        amplifier_core::module_resolver::ModuleArtifact::PythonModule(name) => {
            ("python", None, None, Some(name.clone()))
        }
    };

    Ok(JsModuleManifest {
        transport: transport.to_string(),
        module_type: module_type.to_string(),
        artifact_type: artifact_type.to_string(),
        artifact_path,
        endpoint,
        package_name,
    })
}

/// Load a WASM module from a path and return status info.
///
/// For WASM modules: loads the component and returns module type info.
/// For Python modules: returns an error (TS host can't load Python).
#[napi]
pub fn load_wasm_from_path(path: String) -> Result<String> {
    let manifest = amplifier_core::module_resolver::resolve_module(std::path::Path::new(&path))
        .map_err(|e| Error::from_reason(format!("{e}")))?;

    if manifest.transport == amplifier_core::transport::Transport::Python {
        return Err(Error::from_reason(
            "Python module detected — compile to WASM or run as gRPC sidecar. \
             TypeScript hosts cannot load Python modules.",
        ));
    }

    if manifest.transport != amplifier_core::transport::Transport::Wasm {
        return Err(Error::from_reason(format!(
            "load_wasm_from_path only handles WASM modules, got transport '{:?}'",
            manifest.transport
        )));
    }

    let engine = amplifier_core::wasm_engine::WasmEngine::new()
        .map_err(|e| Error::from_reason(format!("WASM engine creation failed: {e}")))?;

    let coordinator = std::sync::Arc::new(amplifier_core::Coordinator::new_for_test());
    let loaded =
        amplifier_core::module_resolver::load_module(&manifest, engine.inner(), Some(coordinator))
            .map_err(|e| Error::from_reason(format!("Module loading failed: {e}")))?;

    Ok(format!("loaded:{}", loaded.variant_name()))
}
