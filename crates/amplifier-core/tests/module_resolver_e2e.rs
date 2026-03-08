//! E2E integration tests for the full module resolver pipeline.
//!
//! Tests the complete resolve → detect type → load → execute pipeline
//! for all supported module types.
//!
//! Run with: cargo test -p amplifier-core --features wasm --test module_resolver_e2e

#![cfg(feature = "wasm")]

use std::path::Path;
use std::sync::Arc;

use amplifier_core::models::ModuleType;
use amplifier_core::module_resolver::{
    load_module, resolve_module, LoadedModule, ModuleArtifact, ModuleResolverError,
};
use amplifier_core::transport::Transport;
use amplifier_core::wasm_engine::WasmEngine;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a pre-compiled .wasm fixture by name.
///
/// CARGO_MANIFEST_DIR = `.../crates/amplifier-core`; fixtures live two
/// levels up at the workspace root under `tests/fixtures/wasm/`.
fn fixture(name: &str) -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../tests/fixtures/wasm").join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture '{}' not found at {}: {}", name, path.display(), e))
}

/// Create a shared wasmtime Engine with Component Model enabled.
fn make_engine() -> Arc<wasmtime::Engine> {
    WasmEngine::new()
        .expect("WasmEngine::new() should succeed")
        .inner()
}

/// Create a temp directory containing the named fixture file.
fn dir_with_wasm(fixture_name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp dir");
    let bytes = fixture(fixture_name);
    std::fs::write(dir.path().join(fixture_name), &bytes).expect("write fixture");
    dir
}

// ---------------------------------------------------------------------------
// Resolve + detect type for each of the 6 WASM module types
// ---------------------------------------------------------------------------

/// Resolve echo-tool.wasm and verify Transport::Wasm + ModuleType::Tool.
#[test]
fn resolve_wasm_tool() {
    let dir = dir_with_wasm("echo-tool.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Tool);
}

/// Resolve deny-hook.wasm and verify Transport::Wasm + ModuleType::Hook.
#[test]
fn resolve_wasm_hook() {
    let dir = dir_with_wasm("deny-hook.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Hook);
}

/// Resolve memory-context.wasm and verify Transport::Wasm + ModuleType::Context.
#[test]
fn resolve_wasm_context() {
    let dir = dir_with_wasm("memory-context.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Context);
}

/// Resolve auto-approve.wasm and verify Transport::Wasm + ModuleType::Approval.
#[test]
fn resolve_wasm_approval() {
    let dir = dir_with_wasm("auto-approve.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Approval);
}

/// Resolve echo-provider.wasm and verify Transport::Wasm + ModuleType::Provider.
#[test]
fn resolve_wasm_provider() {
    let dir = dir_with_wasm("echo-provider.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Provider);
}

/// Resolve passthrough-orchestrator.wasm and verify Transport::Wasm + ModuleType::Orchestrator.
#[test]
fn resolve_wasm_orchestrator() {
    let dir = dir_with_wasm("passthrough-orchestrator.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Orchestrator);
}

// ---------------------------------------------------------------------------
// Python package detection
// ---------------------------------------------------------------------------

/// Resolve a directory containing __init__.py — expects Python transport with
/// ModuleType::Tool (default) and a PythonModule artifact.
#[test]
fn resolve_python_package() {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(dir.path().join("__init__.py"), b"# package").expect("write __init__.py");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Python);
    assert_eq!(manifest.module_type, ModuleType::Tool);
    match &manifest.artifact {
        ModuleArtifact::PythonModule(name) => {
            assert!(!name.is_empty(), "package name should be non-empty");
        }
        other => panic!("expected PythonModule artifact, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// amplifier.toml gRPC detection
// ---------------------------------------------------------------------------

/// Resolve a directory with amplifier.toml (gRPC transport) — expects the
/// endpoint from the TOML to be captured in the manifest.
#[test]
fn resolve_amplifier_toml_grpc() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let toml_content = r#"
[module]
transport = "grpc"
type = "tool"

[grpc]
endpoint = "http://localhost:50051"
"#;
    std::fs::write(dir.path().join("amplifier.toml"), toml_content).expect("write amplifier.toml");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Grpc);
    assert_eq!(manifest.module_type, ModuleType::Tool);
    match &manifest.artifact {
        ModuleArtifact::GrpcEndpoint(endpoint) => {
            assert_eq!(endpoint, "http://localhost:50051");
        }
        other => panic!("expected GrpcEndpoint artifact, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Priority: amplifier.toml overrides WASM auto-detection
// ---------------------------------------------------------------------------

/// When both echo-tool.wasm AND amplifier.toml are present, the TOML wins.
/// Transport must be Grpc (from the TOML), not Wasm (from the .wasm file).
#[test]
fn resolve_amplifier_toml_overrides_auto() {
    let dir = tempfile::tempdir().expect("create temp dir");

    // Write a real WASM component that would otherwise be auto-detected as Tool.
    let wasm_bytes = fixture("echo-tool.wasm");
    std::fs::write(dir.path().join("echo-tool.wasm"), &wasm_bytes).expect("write wasm");

    // Write an amplifier.toml pointing to gRPC — it should override the .wasm.
    let toml_content = r#"
[module]
transport = "grpc"
type = "tool"

[grpc]
endpoint = "http://localhost:50051"
"#;
    std::fs::write(dir.path().join("amplifier.toml"), toml_content).expect("write amplifier.toml");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(
        manifest.transport,
        Transport::Grpc,
        "amplifier.toml must override WASM auto-detection"
    );
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

/// An empty directory produces a resolution error mentioning "could not detect".
#[test]
fn resolve_empty_dir_errors() {
    let dir = tempfile::tempdir().expect("create temp dir");

    // Annotate with the error type so the ModuleResolverError import is used.
    let result: Result<_, ModuleResolverError> = resolve_module(dir.path());

    assert!(result.is_err(), "empty dir should produce an error");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("could not detect"),
        "error should mention 'could not detect', got: {err_msg}"
    );
}

/// A path that does not exist produces a resolution error mentioning "does not exist".
#[test]
fn resolve_nonexistent_path_errors() {
    let result = resolve_module(Path::new("/nonexistent/path-xyz-resolver-e2e-999"));

    assert!(result.is_err(), "nonexistent path should produce an error");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("does not exist"),
        "error should mention 'does not exist', got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// Full pipeline: resolve → load → execute
// ---------------------------------------------------------------------------

/// Full pipeline for the echo-tool:
/// resolve echo-tool.wasm → load → execute JSON input → verify roundtrip.
///
/// The echo-tool fixture echoes back its input verbatim.
#[tokio::test]
async fn load_module_wasm_tool_e2e() {
    let dir = dir_with_wasm("echo-tool.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");

    let engine = make_engine();
    let coordinator = Arc::new(amplifier_core::Coordinator::new_for_test());
    let loaded = load_module(&manifest, engine, Some(coordinator)).expect("should load");

    match loaded {
        LoadedModule::Tool(tool) => {
            assert_eq!(tool.name(), "echo-tool");
            let input = serde_json::json!({"message": "hello from resolver", "count": 7});
            let result = tool
                .execute(input.clone())
                .await
                .expect("execute should succeed");
            assert!(result.success);
            assert_eq!(result.output, Some(input));
        }
        other => panic!("expected Tool, got {}", other.variant_name()),
    }
}

/// Full pipeline for deny-hook:
/// resolve deny-hook.wasm → load → verify the variant is LoadedModule::Hook.
#[tokio::test]
async fn load_module_wasm_hook_e2e() {
    let dir = dir_with_wasm("deny-hook.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");

    let engine = make_engine();
    let coordinator = Arc::new(amplifier_core::Coordinator::new_for_test());
    let loaded = load_module(&manifest, engine, Some(coordinator)).expect("should load");

    match loaded {
        LoadedModule::Hook(_) => {
            // Verified: the resolver correctly identified and loaded a Hook module.
        }
        other => panic!("expected Hook, got {}", other.variant_name()),
    }
}

/// Full pipeline for a Python package:
/// resolve dir with __init__.py → load → verify LoadedModule::PythonDelegated
/// with a non-empty package_name (the Python host should load it via importlib).
#[test]
fn load_module_python_returns_delegated() {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(dir.path().join("__init__.py"), b"# package").expect("write __init__.py");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    let engine = make_engine();
    let loaded = load_module(&manifest, engine, None).expect("should load");

    match loaded {
        LoadedModule::PythonDelegated { package_name } => {
            assert!(
                !package_name.is_empty(),
                "package_name should be non-empty, got empty string"
            );
        }
        other => panic!("expected PythonDelegated, got {}", other.variant_name()),
    }
}
