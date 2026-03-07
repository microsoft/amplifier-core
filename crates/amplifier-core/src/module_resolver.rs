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
    (
        "amplifier:modules/approval-provider",
        ModuleType::Approval,
    ),
    ("amplifier:modules/provider", ModuleType::Provider),
    (
        "amplifier:modules/orchestrator",
        ModuleType::Orchestrator,
    ),
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
    let component =
        wasmtime::component::Component::new(&engine, wasm_bytes).map_err(|e| {
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
        1 => Ok(matched.into_iter().next().unwrap().1),
        _ => Err(ModuleResolverError::AmbiguousWasmInterface {
            path: wasm_path.to_path_buf(),
            found: matched.into_iter().map(|(prefix, _)| prefix.to_string()).collect(),
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
    let doc: toml::Table = toml::from_str(content).map_err(|e| {
        ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: e.to_string(),
        }
    })?;

    let module_section = doc.get("module").and_then(|v| v.as_table()).ok_or_else(|| {
        ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: "missing [module] section".to_string(),
        }
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

    let module_type = parse_module_type(type_str).ok_or_else(|| {
        ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: format!("unknown module type: {type_str}"),
        }
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
            let wasm_path = module_path.join(wasm_filename);
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

/// Resolve a module from a filesystem path.
///
/// Inspects the directory at `path` and returns a `ModuleManifest`
/// describing the transport, module type, and artifact location.
pub fn resolve_module(_path: &Path) -> Result<ModuleManifest, ModuleResolverError> {
    todo!("Task 5 implements this")
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
            ModuleArtifact::WasmBytes { path: wasm_path, .. } => {
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
    #[test]
    fn detect_wasm_module_type_tool() {
        let bytes = fixture_bytes("echo-tool.wasm");
        let path = fixture_path("echo-tool.wasm");
        let engine = make_engine();
        let result = detect_wasm_module_type(&bytes, engine, &path).unwrap();
        assert_eq!(result, ModuleType::Tool);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_hook() {
        let bytes = fixture_bytes("deny-hook.wasm");
        let path = fixture_path("deny-hook.wasm");
        let engine = make_engine();
        let result = detect_wasm_module_type(&bytes, engine, &path).unwrap();
        assert_eq!(result, ModuleType::Hook);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_context() {
        let bytes = fixture_bytes("memory-context.wasm");
        let path = fixture_path("memory-context.wasm");
        let engine = make_engine();
        let result = detect_wasm_module_type(&bytes, engine, &path).unwrap();
        assert_eq!(result, ModuleType::Context);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_approval() {
        let bytes = fixture_bytes("auto-approve.wasm");
        let path = fixture_path("auto-approve.wasm");
        let engine = make_engine();
        let result = detect_wasm_module_type(&bytes, engine, &path).unwrap();
        assert_eq!(result, ModuleType::Approval);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_provider() {
        let bytes = fixture_bytes("echo-provider.wasm");
        let path = fixture_path("echo-provider.wasm");
        let engine = make_engine();
        let result = detect_wasm_module_type(&bytes, engine, &path).unwrap();
        assert_eq!(result, ModuleType::Provider);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_orchestrator() {
        let bytes = fixture_bytes("passthrough-orchestrator.wasm");
        let path = fixture_path("passthrough-orchestrator.wasm");
        let engine = make_engine();
        let result = detect_wasm_module_type(&bytes, engine, &path).unwrap();
        assert_eq!(result, ModuleType::Orchestrator);
    }

    #[test]
    fn scan_wasm_ignores_non_wasm_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), b"# readme").unwrap();
        std::fs::write(dir.path().join("lib.py"), b"pass").unwrap();

        let result = scan_for_wasm_file(dir.path());
        assert!(result.is_none(), "expected None when no .wasm files present");
    }
}
