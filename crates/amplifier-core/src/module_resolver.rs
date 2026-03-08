//! Cross-language module resolver.
//!
//! Given a filesystem path, inspects its contents and determines:
//! - What transport to use (Python, WASM, gRPC)
//! - What module type it is (Tool, Provider, Orchestrator, etc.)
//! - Where the loadable artifact is
//!
//! Detection order (first match wins):
//! 1. `amplifier.toml` (explicit override)
//! 2. `.wasm` files (auto-detect via Component Model metadata)
//! 3. Python package (`__init__.py` fallback)
//! 4. Error

use std::path::{Path, PathBuf};
#[cfg(feature = "wasm")]
use std::sync::Arc;

use crate::models::ModuleType;
use crate::transport::Transport;

/// Known WASM Component Model interface prefixes mapped to module types.
///
/// Export names in a WASM component include a version suffix (e.g., `@1.0.0`),
/// so we match using `starts_with` against these prefixes.
#[cfg(feature = "wasm")]
const KNOWN_INTERFACES: &[(&str, ModuleType)] = &[
    ("amplifier:modules/tool", ModuleType::Tool),
    ("amplifier:modules/hook-handler", ModuleType::Hook),
    ("amplifier:modules/context-manager", ModuleType::Context),
    ("amplifier:modules/approval-provider", ModuleType::Approval),
    ("amplifier:modules/provider", ModuleType::Provider),
    ("amplifier:modules/orchestrator", ModuleType::Orchestrator),
];

/// Detect the module type of a WASM component by inspecting its exports.
///
/// Loads the component using `wasmtime::component::Component::new`, iterates
/// over its exports, and matches export names against [`KNOWN_INTERFACES`].
///
/// Returns `Ok(ModuleType)` if exactly one known interface is found.
/// Returns `UnknownWasmInterface` if zero matches, `AmbiguousWasmInterface`
/// if more than one match.
#[cfg(feature = "wasm")]
pub fn detect_wasm_module_type(
    wasm_bytes: &[u8],
    engine: Arc<wasmtime::Engine>,
    wasm_path: &Path,
) -> Result<ModuleType, ModuleResolverError> {
    let component = wasmtime::component::Component::new(&engine, wasm_bytes).map_err(|e| {
        ModuleResolverError::WasmLoadError {
            path: wasm_path.to_path_buf(),
            reason: e.to_string(),
        }
    })?;

    let component_type = component.component_type();
    let mut matched: Vec<(&str, ModuleType)> = Vec::new();

    for (export_name, _) in component_type.exports(&engine) {
        for &(prefix, ref module_type) in KNOWN_INTERFACES {
            if export_name.starts_with(prefix) {
                matched.push((prefix, module_type.clone()));
            }
        }
    }

    match matched.len() {
        0 => Err(ModuleResolverError::UnknownWasmInterface {
            path: wasm_path.to_path_buf(),
        }),
        1 => Ok(matched.swap_remove(0).1),
        _ => Err(ModuleResolverError::AmbiguousWasmInterface {
            path: wasm_path.to_path_buf(),
            found: matched
                .into_iter()
                .map(|(prefix, _)| prefix.to_string())
                .collect(),
        }),
    }
}

/// Parse a module type string into a `ModuleType` variant.
///
/// Accepts lowercase strings: "orchestrator", "provider", "tool", "context",
/// "hook", "resolver", "approval". Returns `None` for unrecognized strings.
pub fn parse_module_type(s: &str) -> Option<ModuleType> {
    match s {
        "orchestrator" => Some(ModuleType::Orchestrator),
        "provider" => Some(ModuleType::Provider),
        "tool" => Some(ModuleType::Tool),
        "context" => Some(ModuleType::Context),
        "hook" => Some(ModuleType::Hook),
        "resolver" => Some(ModuleType::Resolver),
        "approval" => Some(ModuleType::Approval),
        _ => None,
    }
}

/// Parse an `amplifier.toml` file content into a `ModuleManifest`.
///
/// The TOML must have a `[module]` section with `transport` and `type` fields.
/// For gRPC transport, a `[grpc]` section with `endpoint` is required.
/// For WASM transport, optional `artifact` field specifies the wasm filename
/// (defaults to `module.wasm`). For Python/Native transport, derive package
/// name from directory name.
pub fn parse_amplifier_toml(
    content: &str,
    module_path: &Path,
) -> Result<ModuleManifest, ModuleResolverError> {
    let doc: toml::Table =
        toml::from_str(content).map_err(|e| ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let module_section = doc
        .get("module")
        .and_then(|v| v.as_table())
        .ok_or_else(|| ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: "missing [module] section".to_string(),
        })?;

    let transport_str = module_section
        .get("transport")
        .and_then(|v| v.as_str())
        .unwrap_or("python");
    let transport = Transport::from_str(transport_str);

    let type_str = module_section
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: "missing 'type' field in [module] section".to_string(),
        })?;

    let module_type =
        parse_module_type(type_str).ok_or_else(|| ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: format!("unknown module type: {type_str}"),
        })?;

    let artifact = match transport {
        Transport::Grpc => {
            let endpoint = doc
                .get("grpc")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("endpoint"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| ModuleResolverError::TomlParseError {
                    path: module_path.to_path_buf(),
                    reason: "gRPC transport requires [grpc] section with 'endpoint' field"
                        .to_string(),
                })?;
            ModuleArtifact::GrpcEndpoint(endpoint.to_string())
        }
        Transport::Wasm => {
            let wasm_filename = module_section
                .get("artifact")
                .and_then(|v| v.as_str())
                .unwrap_or("module.wasm");

            // H-03: Reject path separators — artifact must be a simple filename,
            // not a relative or absolute path that could escape the module directory.
            if wasm_filename.contains('/')
                || wasm_filename.contains('\\')
                || wasm_filename.starts_with('.')
            {
                return Err(ModuleResolverError::TomlParseError {
                    path: module_path.to_path_buf(),
                    reason: "artifact must be a simple filename, not a path".to_string(),
                });
            }

            let wasm_path = module_path.join(wasm_filename);

            // H-03: If the artifact already exists on disk, canonicalize both
            // paths and verify the resolved artifact stays inside module_path.
            // This catches symlink-based escapes that slip past the name check.
            if wasm_path.exists() {
                let canonical =
                    wasm_path
                        .canonicalize()
                        .map_err(|e| ModuleResolverError::TomlParseError {
                            path: module_path.to_path_buf(),
                            reason: format!("could not canonicalize artifact path: {e}"),
                        })?;
                let canonical_base = module_path.canonicalize().map_err(|e| {
                    ModuleResolverError::TomlParseError {
                        path: module_path.to_path_buf(),
                        reason: format!("could not canonicalize module path: {e}"),
                    }
                })?;
                if !canonical.starts_with(&canonical_base) {
                    return Err(ModuleResolverError::TomlParseError {
                        path: module_path.to_path_buf(),
                        reason: "artifact path escapes module directory".to_string(),
                    });
                }
            }

            ModuleArtifact::WasmBytes {
                bytes: Vec::new(), // bytes loaded later by the transport layer
                path: wasm_path,
            }
        }
        Transport::Python | Transport::Native => {
            let dir_name = module_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            ModuleArtifact::PythonModule(dir_name)
        }
    };

    Ok(ModuleManifest {
        transport,
        module_type,
        artifact,
    })
}

/// Describes a resolved module: what transport, what type, and where the artifact is.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleManifest {
    /// Transport to use for loading (Python, WASM, gRPC).
    pub transport: Transport,
    /// Module type (Tool, Provider, Orchestrator, etc.).
    pub module_type: ModuleType,
    /// Where the loadable artifact lives.
    pub artifact: ModuleArtifact,
}

/// The loadable artifact for a resolved module.
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleArtifact {
    /// Raw WASM component bytes, plus the path they were read from.
    WasmBytes { bytes: Vec<u8>, path: PathBuf },
    /// A gRPC endpoint URL (e.g., "http://localhost:50051").
    GrpcEndpoint(String),
    /// A Python package name (e.g., "amplifier_module_tool_bash").
    PythonModule(String),
}

/// Detect a Python package at the given directory path.
///
/// Checks two locations (first match wins):
/// 1. `dir/__init__.py` — the directory itself is a package; derive name from
///    the directory's file name, replacing dashes with underscores.
/// 2. `dir/<subdirectory>/__init__.py` — a nested package; iterate immediate
///    subdirectories looking for `__init__.py` and return the subdirectory name.
///
/// Returns the Python package name if found, or `None`.
pub fn detect_python_package(dir: &Path) -> Option<String> {
    // Check 1: dir itself has __init__.py
    if dir.join("__init__.py").is_file() {
        let name = dir.file_name()?.to_string_lossy().replace('-', "_");
        return Some(name);
    }

    // Check 2: a subdirectory has __init__.py
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() && path.join("__init__.py").is_file() {
            let name = path.file_name()?.to_string_lossy().to_string();
            return Some(name);
        }
    }

    None
}

/// Resolve a module from a filesystem path.
///
/// Inspects the directory at `path` and returns a `ModuleManifest`
/// describing the transport, module type, and artifact location.
///
/// Detection order (first match wins):
/// 1. `amplifier.toml` — explicit manifest
/// 2. `.wasm` file — auto-detected via Component Model metadata
/// 3. Python package (`__init__.py`) — fallback with `ModuleType::Tool`
/// 4. Error (`NoArtifactFound`)
pub fn resolve_module(path: &Path) -> Result<ModuleManifest, ModuleResolverError> {
    // Step 1: path must exist
    if !path.exists() {
        return Err(ModuleResolverError::PathNotFound {
            path: path.to_path_buf(),
        });
    }

    // Step 2: amplifier.toml takes priority
    let toml_path = path.join("amplifier.toml");
    if toml_path.is_file() {
        let content = std::fs::read_to_string(&toml_path).map_err(|e| ModuleResolverError::Io {
            path: toml_path.clone(),
            source: e,
        })?;
        return parse_amplifier_toml(&content, path);
    }

    // Step 3: .wasm file detection
    if let Some(wasm_path) = scan_for_wasm_file(path) {
        #[cfg(feature = "wasm")]
        {
            let bytes = std::fs::read(&wasm_path).map_err(|e| ModuleResolverError::Io {
                path: wasm_path.clone(),
                source: e,
            })?;
            let engine = crate::wasm_engine::WasmEngine::new()
                .map_err(|e| ModuleResolverError::WasmLoadError {
                    path: wasm_path.clone(),
                    reason: e.to_string(),
                })?
                .inner();
            let module_type = detect_wasm_module_type(&bytes, engine, &wasm_path)?;
            return Ok(ModuleManifest {
                transport: Transport::Wasm,
                module_type,
                artifact: ModuleArtifact::WasmBytes {
                    bytes,
                    path: wasm_path,
                },
            });
        }

        #[cfg(not(feature = "wasm"))]
        return Err(ModuleResolverError::WasmLoadError {
            path: wasm_path,
            reason: "WASM support not enabled".to_string(),
        });
    }

    // Step 4: Python package fallback
    if let Some(pkg_name) = detect_python_package(path) {
        return Ok(ModuleManifest {
            transport: Transport::Python,
            module_type: ModuleType::Tool,
            artifact: ModuleArtifact::PythonModule(pkg_name),
        });
    }

    // Step 5: nothing found
    Err(ModuleResolverError::NoArtifactFound {
        path: path.to_path_buf(),
    })
}

/// Errors from module resolution.
#[derive(Debug, thiserror::Error)]
pub enum ModuleResolverError {
    /// The path does not exist or is not a directory.
    #[error("module path does not exist: {path}")]
    PathNotFound { path: PathBuf },

    /// No loadable artifact found at the path.
    #[error("could not detect module transport at {path}. Expected: .wasm file, amplifier.toml, or Python package (__init__.py).")]
    NoArtifactFound { path: PathBuf },

    /// WASM component does not export any known Amplifier module interface.
    #[error("WASM component at {path} does not export any known Amplifier module interface. Known interfaces: amplifier:modules/tool, amplifier:modules/hook-handler, amplifier:modules/context-manager, amplifier:modules/approval-provider, amplifier:modules/provider, amplifier:modules/orchestrator")]
    UnknownWasmInterface { path: PathBuf },

    /// WASM component exports multiple Amplifier interfaces (ambiguous).
    #[error("WASM component at {path} exports multiple Amplifier module interfaces ({}). A component should implement exactly one module type.", found.join(", "))]
    AmbiguousWasmInterface { path: PathBuf, found: Vec<String> },

    /// Failed to parse `amplifier.toml`.
    #[error("failed to parse amplifier.toml at {path}: {reason}")]
    TomlParseError { path: PathBuf, reason: String },

    /// Failed to read or compile a WASM file.
    #[error("failed to load WASM component at {path}: {reason}")]
    WasmLoadError { path: PathBuf, reason: String },

    /// I/O error reading files.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// A fully-loaded module, ready for use.
///
/// Returned by [`load_module`] after dispatch to the appropriate transport bridge.
/// The `PythonDelegated` variant is a signal to the Python host that it should
/// load the module itself via importlib.
#[cfg(feature = "wasm")]
pub enum LoadedModule {
    /// A loaded tool module.
    Tool(Arc<dyn crate::traits::Tool>),
    /// A loaded hook handler module.
    Hook(Arc<dyn crate::traits::HookHandler>),
    /// A loaded context manager module.
    Context(Arc<dyn crate::traits::ContextManager>),
    /// A loaded approval provider module.
    Approval(Arc<dyn crate::traits::ApprovalProvider>),
    /// A loaded provider module.
    Provider(Arc<dyn crate::traits::Provider>),
    /// A loaded orchestrator module.
    Orchestrator(Arc<dyn crate::traits::Orchestrator>),
    /// Python/Native module — the Python host should load this via importlib.
    PythonDelegated {
        /// The Python package name to import.
        package_name: String,
    },
}

#[cfg(feature = "wasm")]
impl LoadedModule {
    /// Returns the variant name as a static string (for diagnostics).
    pub fn variant_name(&self) -> &'static str {
        match self {
            LoadedModule::Tool(_) => "Tool",
            LoadedModule::Hook(_) => "Hook",
            LoadedModule::Context(_) => "Context",
            LoadedModule::Approval(_) => "Approval",
            LoadedModule::Provider(_) => "Provider",
            LoadedModule::Orchestrator(_) => "Orchestrator",
            LoadedModule::PythonDelegated { .. } => "PythonDelegated",
        }
    }
}

/// Load a module artifact into a runtime type, dispatching on transport and module type.
///
/// For `Transport::Wasm`, reads bytes from the manifest artifact, then dispatches to
/// the appropriate `load_wasm_*` function based on `module_type`.
///
/// For `Transport::Python` or `Transport::Native`, returns
/// [`LoadedModule::PythonDelegated`] as a signal to the Python host to handle loading
/// itself via importlib.
///
/// For `Transport::Grpc`, returns an error — gRPC loading is async and must be done
/// directly with [`crate::transport::load_grpc_tool`] or
/// [`crate::transport::load_grpc_orchestrator`].
///
/// `coordinator` is required only for `ModuleType::Orchestrator` WASM modules.
#[cfg(feature = "wasm")]
pub fn load_module(
    manifest: &ModuleManifest,
    engine: Arc<wasmtime::Engine>,
    coordinator: Option<Arc<crate::coordinator::Coordinator>>,
) -> Result<LoadedModule, Box<dyn std::error::Error + Send + Sync>> {
    use crate::models::ModuleType;

    // Resolver modules are metadata-only — they cannot be loaded as runtime modules
    if manifest.module_type == ModuleType::Resolver {
        return Err("Resolver modules are not loadable as runtime modules".into());
    }

    match &manifest.transport {
        Transport::Python | Transport::Native => {
            let package_name = match &manifest.artifact {
                ModuleArtifact::PythonModule(name) => name.clone(),
                other => {
                    return Err(format!(
                        "expected PythonModule artifact for Python/Native transport, got {:?}",
                        other
                    )
                    .into())
                }
            };
            Ok(LoadedModule::PythonDelegated { package_name })
        }

        Transport::Wasm => {
            let bytes = match &manifest.artifact {
                ModuleArtifact::WasmBytes { bytes, .. } => bytes,
                other => {
                    return Err(format!(
                        "expected WasmBytes artifact for WASM transport, got {:?}",
                        other
                    )
                    .into())
                }
            };

            match &manifest.module_type {
                ModuleType::Tool => {
                    let tool = crate::transport::load_wasm_tool(bytes, engine)?;
                    Ok(LoadedModule::Tool(tool))
                }
                ModuleType::Hook => {
                    let hook = crate::transport::load_wasm_hook(bytes, engine)?;
                    Ok(LoadedModule::Hook(hook))
                }
                ModuleType::Context => {
                    let ctx = crate::transport::load_wasm_context(bytes, engine)?;
                    Ok(LoadedModule::Context(ctx))
                }
                ModuleType::Approval => {
                    let approval = crate::transport::load_wasm_approval(bytes, engine)?;
                    Ok(LoadedModule::Approval(approval))
                }
                ModuleType::Provider => {
                    let provider = crate::transport::load_wasm_provider(bytes, engine)?;
                    Ok(LoadedModule::Provider(provider))
                }
                ModuleType::Orchestrator => {
                    let coord = coordinator.ok_or(
                        "Orchestrator WASM module requires a Coordinator but none was provided",
                    )?;
                    let orch = crate::transport::load_wasm_orchestrator(bytes, engine, coord)?;
                    Ok(LoadedModule::Orchestrator(orch))
                }
                // Resolver is rejected by the early-return guard above; this arm is unreachable.
                ModuleType::Resolver => unreachable!(
                    "Resolver modules are rejected before transport dispatch"
                ),
            }
        }

        Transport::Grpc => Err(
            "gRPC module loading requires async runtime. Use load_grpc_tool() / load_grpc_orchestrator() directly.".into(),
        ),
    }
}

/// Scan a directory for the first `.wasm` file.
///
/// Reads the directory entries at `dir`, returning the path to the first
/// file with a `.wasm` extension, or `None` if no such file exists.
pub fn scan_for_wasm_file(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "wasm" {
                    return Some(path);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_manifest_can_be_constructed() {
        let manifest = ModuleManifest {
            transport: Transport::Wasm,
            module_type: ModuleType::Tool,
            artifact: ModuleArtifact::WasmBytes {
                bytes: vec![0, 1, 2],
                path: PathBuf::from("/tmp/echo-tool.wasm"),
            },
        };
        assert_eq!(
            manifest,
            ModuleManifest {
                transport: Transport::Wasm,
                module_type: ModuleType::Tool,
                artifact: ModuleArtifact::WasmBytes {
                    bytes: vec![0, 1, 2],
                    path: PathBuf::from("/tmp/echo-tool.wasm"),
                },
            }
        );
    }

    #[test]
    fn module_artifact_grpc_variant() {
        let artifact = ModuleArtifact::GrpcEndpoint("http://localhost:50051".into());
        match artifact {
            ModuleArtifact::GrpcEndpoint(endpoint) => {
                assert_eq!(endpoint, "http://localhost:50051");
            }
            _ => panic!("expected GrpcEndpoint variant"),
        }
    }

    #[test]
    fn module_artifact_python_variant() {
        let artifact = ModuleArtifact::PythonModule("amplifier_module_tool_bash".into());
        match artifact {
            ModuleArtifact::PythonModule(name) => {
                assert_eq!(name, "amplifier_module_tool_bash");
            }
            _ => panic!("expected PythonModule variant"),
        }
    }

    #[test]
    fn module_resolver_error_ambiguous_displays_found_interfaces() {
        let err = ModuleResolverError::AmbiguousWasmInterface {
            path: PathBuf::from("/tmp/multi.wasm"),
            found: vec![
                "amplifier:modules/tool".into(),
                "amplifier:modules/hook-handler".into(),
            ],
        };
        let msg = format!("{err}");
        assert!(msg.contains("/tmp/multi.wasm"));
        assert!(msg.contains("amplifier:modules/tool, amplifier:modules/hook-handler"));
    }

    #[test]
    fn module_manifest_supports_equality() {
        let a = ModuleManifest {
            transport: Transport::Wasm,
            module_type: ModuleType::Tool,
            artifact: ModuleArtifact::GrpcEndpoint("http://localhost:50051".into()),
        };
        let b = ModuleManifest {
            transport: Transport::Wasm,
            module_type: ModuleType::Tool,
            artifact: ModuleArtifact::GrpcEndpoint("http://localhost:50051".into()),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn module_resolver_error_displays_correctly() {
        let err = ModuleResolverError::NoArtifactFound {
            path: PathBuf::from("/tmp/empty"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("/tmp/empty"));
        assert!(msg.contains(".wasm"));
        assert!(msg.contains("amplifier.toml"));
        assert!(msg.contains("__init__.py"));
    }

    // --- parse_amplifier_toml tests ---

    #[test]
    fn parse_toml_grpc_transport() {
        let toml_content = r#"
[module]
transport = "grpc"
type = "tool"

[grpc]
endpoint = "http://localhost:50051"
"#;
        let path = Path::new("/modules/my-tool");
        let manifest = parse_amplifier_toml(toml_content, path).unwrap();
        assert_eq!(manifest.transport, Transport::Grpc);
        assert_eq!(manifest.module_type, ModuleType::Tool);
        assert_eq!(
            manifest.artifact,
            ModuleArtifact::GrpcEndpoint("http://localhost:50051".into())
        );
    }

    #[test]
    fn parse_toml_wasm_transport() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "hook"
artifact = "my-hook.wasm"
"#;
        let path = Path::new("/modules/my-hook");
        let manifest = parse_amplifier_toml(toml_content, path).unwrap();
        assert_eq!(manifest.transport, Transport::Wasm);
        assert_eq!(manifest.module_type, ModuleType::Hook);
        match &manifest.artifact {
            ModuleArtifact::WasmBytes {
                path: wasm_path, ..
            } => {
                assert_eq!(wasm_path, &PathBuf::from("/modules/my-hook/my-hook.wasm"));
            }
            other => panic!("expected WasmBytes, got {other:?}"),
        }
    }

    #[test]
    fn parse_toml_python_transport() {
        let toml_content = r#"
[module]
transport = "python"
type = "provider"
"#;
        let path = Path::new("/modules/my-provider");
        let manifest = parse_amplifier_toml(toml_content, path).unwrap();
        assert_eq!(manifest.transport, Transport::Python);
        assert_eq!(manifest.module_type, ModuleType::Provider);
        assert_eq!(
            manifest.artifact,
            ModuleArtifact::PythonModule("my-provider".into())
        );
    }

    #[test]
    fn parse_toml_grpc_missing_endpoint_errors() {
        let toml_content = r#"
[module]
transport = "grpc"
type = "tool"
"#;
        let path = Path::new("/modules/my-tool");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("endpoint"));
    }

    #[test]
    fn parse_toml_missing_type_errors() {
        let toml_content = r#"
[module]
transport = "grpc"
"#;
        let path = Path::new("/modules/my-tool");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("type"));
    }

    #[test]
    fn parse_toml_unknown_module_type_errors() {
        let toml_content = r#"
[module]
transport = "grpc"
type = "foobar"

[grpc]
endpoint = "http://localhost:50051"
"#;
        let path = Path::new("/modules/my-tool");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("unknown module type: foobar"));
    }

    #[test]
    fn parse_toml_missing_module_section_errors() {
        let toml_content = r#"
[grpc]
endpoint = "http://localhost:50051"
"#;
        let path = Path::new("/modules/my-tool");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("module"));
    }

    // --- scan_for_wasm_file tests ---

    #[test]
    fn scan_wasm_finds_wasm_file() {
        let dir = tempfile::tempdir().unwrap();
        let wasm_path = dir.path().join("echo-tool.wasm");
        std::fs::write(&wasm_path, b"fake wasm").unwrap();

        let result = scan_for_wasm_file(dir.path());
        assert!(result.is_some(), "expected to find a .wasm file");
        assert_eq!(result.unwrap(), wasm_path);
    }

    #[test]
    fn scan_wasm_returns_none_for_empty_dir() {
        let dir = tempfile::tempdir().unwrap();

        let result = scan_for_wasm_file(dir.path());
        assert!(result.is_none(), "expected None for empty directory");
    }

    #[cfg(feature = "wasm")]
    fn fixture_path(name: &str) -> std::path::PathBuf {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../tests/fixtures/wasm").join(name)
    }

    #[cfg(feature = "wasm")]
    fn fixture_bytes(name: &str) -> Vec<u8> {
        let path = fixture_path(name);
        std::fs::read(&path)
            .unwrap_or_else(|e| panic!("fixture {name} not found at {}: {e}", path.display()))
    }

    #[cfg(feature = "wasm")]
    fn make_engine() -> std::sync::Arc<wasmtime::Engine> {
        crate::wasm_engine::WasmEngine::new().unwrap().inner()
    }

    #[cfg(feature = "wasm")]
    fn assert_detects(fixture: &str, expected: ModuleType) {
        let bytes = fixture_bytes(fixture);
        let path = fixture_path(fixture);
        let engine = make_engine();
        let result = detect_wasm_module_type(&bytes, engine, &path).unwrap();
        assert_eq!(result, expected);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_tool() {
        assert_detects("echo-tool.wasm", ModuleType::Tool);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_hook() {
        assert_detects("deny-hook.wasm", ModuleType::Hook);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_context() {
        assert_detects("memory-context.wasm", ModuleType::Context);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_approval() {
        assert_detects("auto-approve.wasm", ModuleType::Approval);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_provider() {
        assert_detects("echo-provider.wasm", ModuleType::Provider);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_orchestrator() {
        assert_detects("passthrough-orchestrator.wasm", ModuleType::Orchestrator);
    }

    #[test]
    fn scan_wasm_ignores_non_wasm_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), b"# readme").unwrap();
        std::fs::write(dir.path().join("lib.py"), b"pass").unwrap();

        let result = scan_for_wasm_file(dir.path());
        assert!(
            result.is_none(),
            "expected None when no .wasm files present"
        );
    }

    // --- detect_python_package tests ---

    #[test]
    fn detect_python_package_with_init_py() {
        // Directory itself is a Python package (has __init__.py at top level).
        // Name derived from directory name with dashes replaced by underscores.
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir.path().join("amplifier-module-tool-bash");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("__init__.py"), b"").unwrap();

        let result = detect_python_package(&pkg_dir);
        assert_eq!(result, Some("amplifier_module_tool_bash".to_string()));
    }

    #[test]
    fn detect_python_package_with_nested_package() {
        // Directory contains a subdirectory that is a Python package.
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir.path().join("my-module");
        let nested = pkg_dir.join("amplifier_module_tool_bash");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("__init__.py"), b"").unwrap();

        let result = detect_python_package(&pkg_dir);
        assert_eq!(result, Some("amplifier_module_tool_bash".to_string()));
    }

    #[test]
    fn detect_python_package_empty_dir() {
        let dir = tempfile::tempdir().unwrap();

        let result = detect_python_package(dir.path());
        assert_eq!(result, None);
    }

    #[test]
    fn detect_python_package_no_init_py() {
        // Directory has files but no __init__.py anywhere.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), b"# readme").unwrap();
        std::fs::write(dir.path().join("main.py"), b"print('hello')").unwrap();

        let result = detect_python_package(dir.path());
        assert_eq!(result, None);
    }

    // --- resolve_module tests ---

    #[test]
    fn resolve_module_with_amplifier_toml() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let toml_content = r#"
[module]
transport = "grpc"
type = "tool"

[grpc]
endpoint = "http://localhost:9999"
"#;
        std::fs::write(dir.path().join("amplifier.toml"), toml_content).expect("write toml");
        // Also add a .wasm file to prove TOML takes priority
        std::fs::write(dir.path().join("echo-tool.wasm"), b"fake").expect("write wasm");

        let manifest = resolve_module(dir.path()).expect("should resolve");
        assert_eq!(manifest.transport, Transport::Grpc);
        assert_eq!(manifest.module_type, ModuleType::Tool);
        match manifest.artifact {
            ModuleArtifact::GrpcEndpoint(ref ep) => assert_eq!(ep, "http://localhost:9999"),
            _ => panic!("expected GrpcEndpoint"),
        }
    }

    #[test]
    fn resolve_module_with_python_package() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("__init__.py"), b"# package").expect("write");

        let manifest = resolve_module(dir.path()).expect("should resolve");
        assert_eq!(manifest.transport, Transport::Python);
    }

    #[test]
    fn resolve_module_empty_dir_errors() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let result = resolve_module(dir.path());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("could not detect"));
    }

    #[test]
    fn resolve_module_nonexistent_path_errors() {
        let result = resolve_module(Path::new("/tmp/nonexistent-module-path-xyz"));
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("does not exist"));
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn resolve_module_with_real_wasm_fixture() {
        // Create a temp dir and copy a real fixture into it
        let dir = tempfile::tempdir().expect("create temp dir");
        let wasm_bytes = fixture_bytes("echo-tool.wasm");
        std::fs::write(dir.path().join("echo-tool.wasm"), &wasm_bytes).expect("write wasm");

        let manifest = resolve_module(dir.path()).expect("should resolve");
        assert_eq!(manifest.transport, Transport::Wasm);
        assert_eq!(manifest.module_type, ModuleType::Tool);
        match &manifest.artifact {
            ModuleArtifact::WasmBytes { bytes, path } => {
                assert!(!bytes.is_empty());
                assert!(path.to_string_lossy().contains("echo-tool.wasm"));
            }
            _ => panic!("expected WasmBytes"),
        }
    }

    #[cfg(feature = "wasm")]
    #[tokio::test]
    async fn load_module_wasm_tool() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let wasm_bytes = fixture_bytes("echo-tool.wasm");
        std::fs::write(dir.path().join("echo-tool.wasm"), &wasm_bytes).expect("write wasm");

        let manifest = resolve_module(dir.path()).expect("should resolve");
        let engine = make_engine();
        let coordinator = std::sync::Arc::new(crate::coordinator::Coordinator::new_for_test());
        let result = load_module(&manifest, engine, Some(coordinator));
        assert!(result.is_ok());
        match result.unwrap() {
            LoadedModule::Tool(tool) => assert_eq!(tool.name(), "echo-tool"),
            other => panic!("expected Tool, got {:?}", other.variant_name()),
        }
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_module_python_returns_signal() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("__init__.py"), b"# package").expect("write");

        let manifest = resolve_module(dir.path()).expect("should resolve");
        let engine = make_engine();
        let result = load_module(&manifest, engine, None);
        assert!(result.is_ok());
        match result.unwrap() {
            LoadedModule::PythonDelegated { package_name } => {
                assert!(!package_name.is_empty());
            }
            other => panic!("expected PythonDelegated, got {:?}", other.variant_name()),
        }
    }

    /// Helper: resolve a fixture source directory via its amplifier.toml manifest and
    /// assert the expected transport and module type.  Mirrors the `assert_detects` helper
    /// used for the WASM auto-detection path.
    fn assert_resolves_toml(fixture_dir: &str, expected: ModuleType) {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let fixture_src = manifest_dir.join(format!("../../tests/fixtures/wasm/src/{fixture_dir}"));
        assert!(
            fixture_src.exists(),
            "fixture source dir should exist: {}",
            fixture_src.display()
        );

        let manifest = resolve_module(&fixture_src).expect("should resolve via amplifier.toml");
        assert_eq!(manifest.transport, Transport::Wasm);
        assert_eq!(manifest.module_type, expected);
    }

    #[test]
    fn resolve_fixture_via_amplifier_toml() {
        assert_resolves_toml("echo-tool", ModuleType::Tool);
    }

    #[test]
    fn resolve_fixture_via_amplifier_toml_hook() {
        assert_resolves_toml("deny-hook", ModuleType::Hook);
    }

    #[test]
    fn resolve_fixture_via_amplifier_toml_context() {
        assert_resolves_toml("memory-context", ModuleType::Context);
    }

    #[test]
    fn resolve_fixture_via_amplifier_toml_approval() {
        assert_resolves_toml("auto-approve", ModuleType::Approval);
    }

    #[test]
    fn resolve_fixture_via_amplifier_toml_provider() {
        assert_resolves_toml("echo-provider", ModuleType::Provider);
    }

    #[test]
    fn resolve_fixture_via_amplifier_toml_orchestrator() {
        assert_resolves_toml("passthrough-orchestrator", ModuleType::Orchestrator);
    }

    // --- path traversal tests (H-03) ---

    #[test]
    fn parse_toml_wasm_artifact_path_with_slashes_rejected() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "../../etc/passwd"
"#;
        let path = Path::new("/tmp/test-module");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(
            result.is_err(),
            "expected error for artifact with path separators"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("simple filename"),
            "error should mention 'simple filename': {msg}"
        );
    }

    #[test]
    fn parse_toml_wasm_artifact_dotdot_relative_rejected() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "../sibling/module.wasm"
"#;
        let path = Path::new("/tmp/test-module");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(
            result.is_err(),
            "expected error for dotdot relative artifact path"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("simple filename"),
            "error should mention 'simple filename': {msg}"
        );
    }

    #[test]
    fn parse_toml_wasm_artifact_hidden_dot_file_rejected() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = ".hidden.wasm"
"#;
        let path = Path::new("/tmp/test-module");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(
            result.is_err(),
            "expected error for artifact starting with '.'"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("simple filename"),
            "error should mention 'simple filename': {msg}"
        );
    }

    #[test]
    fn parse_toml_wasm_artifact_simple_filename_accepted() {
        // A well-formed artifact = "module.wasm" must be accepted.
        // Uses a non-existent path — canonicalization is skipped when the
        // file is absent (path is resolved at load-time, not parse-time).
        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "module.wasm"
"#;
        let path = Path::new("/tmp/test-module");
        let result = parse_amplifier_toml(toml_content, path);
        assert!(
            result.is_ok(),
            "expected success for simple filename, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn parse_toml_wasm_artifact_symlink_escape_rejected() {
        // An artifact that is a simple filename but resolves via symlink to a
        // path outside the module directory must be rejected (confinement check).
        use std::os::unix::fs::symlink;

        let base = tempfile::tempdir().unwrap();
        let module_dir = base.path().join("my-module");
        std::fs::create_dir(&module_dir).unwrap();

        // Create a "sensitive" wasm file one level above the module dir.
        let sensitive = base.path().join("sensitive.wasm");
        std::fs::write(&sensitive, b"sensitive data").unwrap();

        // Symlink inside the module dir → points outside.
        symlink(&sensitive, module_dir.join("evil.wasm")).unwrap();

        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "evil.wasm"
"#;
        let result = parse_amplifier_toml(toml_content, &module_dir);
        assert!(
            result.is_err(),
            "expected error when artifact symlink escapes module directory"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("escapes module directory"),
            "error should mention 'escapes module directory': {msg}"
        );
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_module_resolver_type_errors() {
        let manifest = ModuleManifest {
            transport: Transport::Python,
            module_type: ModuleType::Resolver,
            artifact: ModuleArtifact::PythonModule("some_resolver".into()),
        };
        let engine = make_engine();
        let result = load_module(&manifest, engine, None);
        assert!(result.is_err());
    }
}
