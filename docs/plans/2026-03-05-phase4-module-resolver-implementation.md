# Phase 4: Cross-Language Module Resolver — Implementation Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Given a resolved filesystem path to a module, automatically detect its transport (Python, WASM, gRPC) and module type (Tool, Provider, Orchestrator, etc.), then load it through the correct bridge — making transport invisible to developers.

**Architecture:** A new `module_resolver.rs` in the Rust kernel inspects a directory path and returns a `ModuleManifest` describing what was found (transport, module type, artifact). Detection runs in priority order: `amplifier.toml` (explicit override) → `.wasm` files (auto-detect via Component Model metadata) → Python package (`__init__.py` fallback) → error. A convenience `load_module()` function dispatches the manifest to the correct `load_wasm_*` / `load_grpc_*` function from `transport.rs`. Both functions are exposed to Python (PyO3) and TypeScript (Napi-RS), and `loader_dispatch.py` is updated to call them instead of raising `NotImplementedError`.

**Tech Stack:** Rust (amplifier-core), wasmtime Component Model inspection, TOML parsing (`toml` crate), PyO3 bindings, Napi-RS bindings, Python (loader_dispatch.py)

**Design doc:** `docs/plans/2026-03-05-phase4-module-resolver-design.md`

**Branch:** `dev/cross-language-sdk` (Phase 3 already merged — all 6 WASM bridges work, test fixtures exist)

---

## Codebase Orientation (Read This First)

You are working in `/home/bkrabach/dev/rust-devrust-core/amplifier-core/` on branch `dev/cross-language-sdk`.

**Key existing files you'll interact with:**

| File | What's in it |
|---|---|
| `crates/amplifier-core/src/transport.rs` | `Transport` enum (`Python`, `Grpc`, `Native`, `Wasm`), `load_wasm_tool()`, `load_wasm_hook()`, `load_wasm_context()`, `load_wasm_approval()`, `load_wasm_provider()`, `load_wasm_orchestrator()`, `load_grpc_tool()`, `load_grpc_orchestrator()`. Each `load_wasm_*` takes `(&[u8], Arc<wasmtime::Engine>)` and returns `Result<Arc<dyn Trait>>`. |
| `crates/amplifier-core/src/models.rs` | `ModuleType` enum with variants: `Orchestrator`, `Provider`, `Tool`, `Context`, `Hook`, `Resolver`. Uses `#[serde(rename_all = "snake_case")]`. |
| `crates/amplifier-core/src/lib.rs` | Public module declarations and re-exports. You'll add `pub mod module_resolver;` here. |
| `crates/amplifier-core/src/wasm_engine.rs` | `WasmEngine` wrapper holding `Arc<wasmtime::Engine>`. Call `WasmEngine::new()?.inner()` to get the engine. |
| `crates/amplifier-core/src/traits.rs` | The 6 module traits: `Tool`, `Provider`, `Orchestrator`, `ContextManager`, `HookHandler`, `ApprovalProvider`. |
| `crates/amplifier-core/src/coordinator.rs` | `Coordinator` struct with `new_for_test()` and typed mount points. `load_wasm_orchestrator()` requires `Arc<Coordinator>`. |
| `crates/amplifier-core/Cargo.toml` | Dependencies. `wasmtime` and `wasmtime-wasi` are behind `features = ["wasm"]`. You'll add `toml` crate here. |
| `bindings/python/src/lib.rs` | PyO3 bindings. Has `PySession`, `PyCoordinator`, `PyHookRegistry`, `PyCancellationToken`. You'll add `resolve_module()` and `load_module()` functions. |
| `bindings/python/Cargo.toml` | PyO3 crate dependencies. You'll add `amplifier-core` wasm feature here. |
| `bindings/node/src/lib.rs` | Napi-RS bindings. Has `JsCoordinator`, `JsAmplifierSession`, etc. You'll add `resolveModule()` and `loadModule()`. |
| `bindings/node/Cargo.toml` | Napi-RS crate dependencies. You'll add `amplifier-core` wasm feature here. |
| `python/amplifier_core/loader_dispatch.py` | Current Python transport routing. Has `load_module()` with `NotImplementedError` for WASM and native transports. |
| `tests/fixtures/wasm/` | Pre-compiled `.wasm` fixtures: `echo-tool.wasm`, `deny-hook.wasm`, `memory-context.wasm`, `auto-approve.wasm`, `echo-provider.wasm`, `passthrough-orchestrator.wasm`. |
| `wit/amplifier-modules.wit` | WIT definitions. Package `amplifier:modules@1.0.0`. Interface names: `tool`, `hook-handler`, `context-manager`, `approval-provider`, `provider`, `orchestrator`. |

**Test commands:**
```bash
# Unit tests (Rust, with WASM feature)
cargo test -p amplifier-core --features wasm

# Specific test
cargo test -p amplifier-core --features wasm -- test_name_here

# Clippy
cargo clippy -p amplifier-core --features wasm -- -D warnings

# Integration tests only
cargo test -p amplifier-core --features wasm --test wasm_e2e
```

**Fixture helper pattern** (copy this for tests):
```rust
fn fixture(name: &str) -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../tests/fixtures/wasm").join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture '{}' not found at {}: {}", name, path.display(), e))
}
```

**INTERFACE_NAME constants in existing bridges** (the resolver must match these):
- `wasm_tool.rs`: `"amplifier:modules/tool@1.0.0"`
- `wasm_hook.rs`: `"amplifier:modules/hook-handler@1.0.0"`
- `wasm_context.rs`: `"amplifier:modules/context-manager@1.0.0"` (verify — check the file)
- `wasm_approval.rs`: `"amplifier:modules/approval-provider@1.0.0"` (verify — check the file)
- `wasm_provider.rs`: `"amplifier:modules/provider@1.0.0"` (verify — check the file)
- `wasm_orchestrator.rs`: `"amplifier:modules/orchestrator@1.0.0"` (verify — check the file)

---

## Task 0: Define `ModuleManifest`, `ModuleArtifact` Types and Create `module_resolver.rs` Skeleton

**Files:**
- Create: `crates/amplifier-core/src/module_resolver.rs`
- Modify: `crates/amplifier-core/src/lib.rs`

### Step 1: Write the failing test

Add the file `crates/amplifier-core/src/module_resolver.rs` with **only** the test module at the bottom — no implementation yet. This test verifies the types exist and can be constructed:

```rust
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

use crate::models::ModuleType;
use crate::transport::Transport;

/// Describes a resolved module: what transport, what type, and where the artifact is.
#[derive(Debug, Clone)]
pub struct ModuleManifest {
    /// Transport to use for loading (Python, WASM, gRPC).
    pub transport: Transport,
    /// Module type (Tool, Provider, Orchestrator, etc.).
    pub module_type: ModuleType,
    /// Where the loadable artifact lives.
    pub artifact: ModuleArtifact,
}

/// The loadable artifact for a resolved module.
#[derive(Debug, Clone)]
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
pub fn resolve_module(path: &Path) -> Result<ModuleManifest, ModuleResolverError> {
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
    #[error("WASM component at {path} exports multiple Amplifier module interfaces ({found:?}). A component should implement exactly one module type.")]
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
        assert_eq!(manifest.transport, Transport::Wasm);
        assert_eq!(manifest.module_type, ModuleType::Tool);
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
}
```

### Step 2: Register the module in `lib.rs`

Open `crates/amplifier-core/src/lib.rs`. Add `pub mod module_resolver;` after the `transport` line. The module list currently looks like:

```rust
pub mod transport;
#[cfg(feature = "wasm")]
pub mod wasm_engine;
```

Add the new line so it becomes:

```rust
pub mod module_resolver;
pub mod transport;
#[cfg(feature = "wasm")]
pub mod wasm_engine;
```

(Keep alphabetical order with other modules.)

### Step 3: Run test to verify it passes

```bash
cd /home/bkrabach/dev/rust-devrust-core/amplifier-core
cargo test -p amplifier-core --features wasm -- module_resolver -v
```

**Expected:** 4 tests pass: `module_manifest_can_be_constructed`, `module_artifact_grpc_variant`, `module_artifact_python_variant`, `module_resolver_error_displays_correctly`.

### Step 4: Run clippy

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
```

**Expected:** No errors. (The `todo!()` macro in `resolve_module` is fine — clippy doesn't flag it.)

### Step 5: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs crates/amplifier-core/src/lib.rs
git commit -m "feat(resolver): add ModuleManifest, ModuleArtifact types and module_resolver skeleton"
```

---

## Task 1: `amplifier.toml` Reader

**Files:**
- Modify: `crates/amplifier-core/Cargo.toml` (add `toml` dependency)
- Modify: `crates/amplifier-core/src/module_resolver.rs` (add TOML parsing function)

### Step 1: Add the `toml` crate dependency

Open `crates/amplifier-core/Cargo.toml`. In the `[dependencies]` section, add:

```toml
toml = "0.8"
```

Add it alphabetically — after the `tokio-stream` line and before the `wasmtime` line.

### Step 2: Write the failing test

Add these tests to the `mod tests` block in `module_resolver.rs`:

```rust
    #[test]
    fn parse_toml_grpc_transport() {
        let toml_content = r#"
[module]
transport = "grpc"
type = "tool"

[grpc]
endpoint = "http://localhost:50051"
"#;
        let manifest = parse_amplifier_toml(toml_content, Path::new("/tmp/my-module"))
            .expect("should parse valid TOML");
        assert_eq!(manifest.transport, Transport::Grpc);
        assert_eq!(manifest.module_type, ModuleType::Tool);
        match manifest.artifact {
            ModuleArtifact::GrpcEndpoint(ref ep) => assert_eq!(ep, "http://localhost:50051"),
            _ => panic!("expected GrpcEndpoint"),
        }
    }

    #[test]
    fn parse_toml_wasm_transport() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "provider"
artifact = "my-provider.wasm"
"#;
        let manifest = parse_amplifier_toml(toml_content, Path::new("/tmp/my-module"))
            .expect("should parse valid TOML");
        assert_eq!(manifest.transport, Transport::Wasm);
        assert_eq!(manifest.module_type, ModuleType::Provider);
    }

    #[test]
    fn parse_toml_python_transport() {
        let toml_content = r#"
[module]
transport = "python"
type = "hook"
"#;
        let manifest = parse_amplifier_toml(toml_content, Path::new("/tmp/my-module"))
            .expect("should parse valid TOML");
        assert_eq!(manifest.transport, Transport::Python);
        assert_eq!(manifest.module_type, ModuleType::Hook);
    }

    #[test]
    fn parse_toml_grpc_missing_endpoint_errors() {
        let toml_content = r#"
[module]
transport = "grpc"
type = "tool"
"#;
        let result = parse_amplifier_toml(toml_content, Path::new("/tmp/my-module"));
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("endpoint"));
    }

    #[test]
    fn parse_toml_missing_type_errors() {
        let toml_content = r#"
[module]
transport = "grpc"
"#;
        let result = parse_amplifier_toml(toml_content, Path::new("/tmp/my-module"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_toml_missing_module_section_errors() {
        let toml_content = r#"
name = "something"
"#;
        let result = parse_amplifier_toml(toml_content, Path::new("/tmp/my-module"));
        assert!(result.is_err());
    }
```

### Step 3: Run test to verify they fail

```bash
cargo test -p amplifier-core --features wasm -- parse_toml -v
```

**Expected:** FAIL — `parse_amplifier_toml` doesn't exist yet.

### Step 4: Write the implementation

Add this function above the `resolve_module` function in `module_resolver.rs` (after the `ModuleResolverError` enum):

```rust
/// Parse an `amplifier.toml` content string into a `ModuleManifest`.
///
/// The TOML must have a `[module]` section with `transport` and `type` fields.
/// For gRPC transport, a `[grpc]` section with `endpoint` is required.
pub(crate) fn parse_amplifier_toml(
    content: &str,
    module_path: &Path,
) -> Result<ModuleManifest, ModuleResolverError> {
    let table: toml::Table = content.parse().map_err(|e: toml::de::Error| {
        ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: e.to_string(),
        }
    })?;

    let module_section = table.get("module").and_then(|v| v.as_table()).ok_or_else(|| {
        ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: "missing [module] section".into(),
        }
    })?;

    // Parse transport
    let transport_str = module_section
        .get("transport")
        .and_then(|v| v.as_str())
        .unwrap_or("python");
    let transport = Transport::from_str(transport_str);

    // Parse module type (required)
    let type_str = module_section
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: "missing 'type' field in [module] section".into(),
        })?;
    let module_type = parse_module_type(type_str).ok_or_else(|| {
        ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: format!(
                "unknown module type '{}'. Valid types: tool, hook, context, approval, provider, orchestrator, resolver",
                type_str
            ),
        }
    })?;

    // Build artifact based on transport
    let artifact = match transport {
        Transport::Grpc => {
            let endpoint = table
                .get("grpc")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("endpoint"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| ModuleResolverError::TomlParseError {
                    path: module_path.to_path_buf(),
                    reason: "gRPC transport requires [grpc] section with 'endpoint' field".into(),
                })?;
            ModuleArtifact::GrpcEndpoint(endpoint.to_string())
        }
        Transport::Wasm => {
            // If artifact path specified in TOML, use it; otherwise will be detected later
            let wasm_filename = module_section
                .get("artifact")
                .and_then(|v| v.as_str())
                .unwrap_or("module.wasm");
            let wasm_path = module_path.join(wasm_filename);
            // Don't read bytes here — the caller will read them if the file exists
            ModuleArtifact::WasmBytes {
                bytes: Vec::new(),
                path: wasm_path,
            }
        }
        Transport::Python | Transport::Native => {
            // Derive Python module name from directory name
            let dir_name = module_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let python_name = dir_name.replace('-', "_");
            ModuleArtifact::PythonModule(python_name)
        }
    };

    Ok(ModuleManifest {
        transport,
        module_type,
        artifact,
    })
}

/// Convert a type string to a `ModuleType`.
fn parse_module_type(s: &str) -> Option<ModuleType> {
    match s {
        "tool" => Some(ModuleType::Tool),
        "hook" => Some(ModuleType::Hook),
        "context" => Some(ModuleType::Context),
        "approval" => Some(ModuleType::Approval),
        "provider" => Some(ModuleType::Provider),
        "orchestrator" => Some(ModuleType::Orchestrator),
        "resolver" => Some(ModuleType::Resolver),
        _ => None,
    }
}
```

**Important:** You need to add an `Approval` variant to the `ModuleType` enum in `models.rs`. Currently it has `Orchestrator, Provider, Tool, Context, Hook, Resolver`. The design requires `Approval` too (for `amplifier:modules/approval-provider`). Open `crates/amplifier-core/src/models.rs` and add `Approval` to the `ModuleType` enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleType {
    Orchestrator,
    Provider,
    Tool,
    Context,
    Hook,
    Approval,
    Resolver,
}
```

Also add `use toml;` is not needed since we use the fully-qualified `toml::Table` in the code — the `toml` crate is used directly via `content.parse()`.

### Step 5: Run tests to verify they pass

```bash
cargo test -p amplifier-core --features wasm -- parse_toml -v
```

**Expected:** All 6 `parse_toml_*` tests pass.

### Step 6: Run clippy

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
```

**Expected:** Clean.

### Step 7: Commit

```bash
git add crates/amplifier-core/Cargo.toml crates/amplifier-core/src/module_resolver.rs crates/amplifier-core/src/models.rs
git commit -m "feat(resolver): add amplifier.toml reader with TOML parsing"
```

---

## Task 2: `.wasm` File Scanner

**Files:**
- Modify: `crates/amplifier-core/src/module_resolver.rs`

### Step 1: Write the failing test

Add to `mod tests`:

```rust
    #[test]
    fn scan_wasm_finds_wasm_file() {
        // Create a temp dir with a .wasm file
        let dir = tempfile::tempdir().expect("create temp dir");
        let wasm_path = dir.path().join("echo-tool.wasm");
        std::fs::write(&wasm_path, b"fake wasm bytes").expect("write wasm file");

        let found = scan_for_wasm_file(dir.path()).expect("should find wasm file");
        assert_eq!(found.file_name().unwrap(), "echo-tool.wasm");
    }

    #[test]
    fn scan_wasm_returns_none_for_empty_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let result = scan_for_wasm_file(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn scan_wasm_ignores_non_wasm_files() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("README.md"), b"hello").expect("write");
        std::fs::write(dir.path().join("lib.py"), b"pass").expect("write");

        let result = scan_for_wasm_file(dir.path());
        assert!(result.is_none());
    }
```

You'll need the `tempfile` crate for tests. Add it to `Cargo.toml` under `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

(Check if `[dev-dependencies]` already exists — if not, add the section. The `tokio` dev-dependency is needed for async tests in Task 10.)

### Step 2: Run test to verify it fails

```bash
cargo test -p amplifier-core --features wasm -- scan_wasm -v
```

**Expected:** FAIL — `scan_for_wasm_file` doesn't exist yet.

### Step 3: Write the implementation

Add this function in `module_resolver.rs` (above `resolve_module`):

```rust
/// Scan a directory for `.wasm` files. Returns the path to the first one found, or None.
pub(crate) fn scan_for_wasm_file(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
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
```

### Step 4: Run test to verify it passes

```bash
cargo test -p amplifier-core --features wasm -- scan_wasm -v
```

**Expected:** All 3 `scan_wasm_*` tests pass.

### Step 5: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs crates/amplifier-core/Cargo.toml
git commit -m "feat(resolver): add .wasm file scanner"
```

---

## Task 3: WASM Component Metadata Parser

**Files:**
- Modify: `crates/amplifier-core/src/module_resolver.rs`

This is the most important detection step. Given `.wasm` bytes, load the component and inspect its exports to determine the `ModuleType`.

### Step 1: Write the failing test

Add to `mod tests`. These tests use the real `.wasm` fixtures:

```rust
    #[cfg(feature = "wasm")]
    fn fixture(name: &str) -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest.join("../../tests/fixtures/wasm").join(name);
        std::fs::read(&path)
            .unwrap_or_else(|e| panic!("fixture '{}' not found at {}: {}", name, path.display(), e))
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_tool() {
        let bytes = fixture("echo-tool.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let module_type = detect_wasm_module_type(&bytes, engine.inner(), Path::new("echo-tool.wasm"))
            .expect("should detect tool");
        assert_eq!(module_type, ModuleType::Tool);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_hook() {
        let bytes = fixture("deny-hook.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let module_type = detect_wasm_module_type(&bytes, engine.inner(), Path::new("deny-hook.wasm"))
            .expect("should detect hook");
        assert_eq!(module_type, ModuleType::Hook);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_context() {
        let bytes = fixture("memory-context.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let module_type = detect_wasm_module_type(&bytes, engine.inner(), Path::new("memory-context.wasm"))
            .expect("should detect context");
        assert_eq!(module_type, ModuleType::Context);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_approval() {
        let bytes = fixture("auto-approve.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let module_type = detect_wasm_module_type(&bytes, engine.inner(), Path::new("auto-approve.wasm"))
            .expect("should detect approval");
        assert_eq!(module_type, ModuleType::Approval);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_provider() {
        let bytes = fixture("echo-provider.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let module_type = detect_wasm_module_type(&bytes, engine.inner(), Path::new("echo-provider.wasm"))
            .expect("should detect provider");
        assert_eq!(module_type, ModuleType::Provider);
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn detect_wasm_module_type_orchestrator() {
        let bytes = fixture("passthrough-orchestrator.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let module_type = detect_wasm_module_type(&bytes, engine.inner(), Path::new("passthrough-orchestrator.wasm"))
            .expect("should detect orchestrator");
        assert_eq!(module_type, ModuleType::Orchestrator);
    }
```

### Step 2: Run test to verify they fail

```bash
cargo test -p amplifier-core --features wasm -- detect_wasm_module_type -v
```

**Expected:** FAIL — `detect_wasm_module_type` doesn't exist.

### Step 3: Write the implementation

Add this to `module_resolver.rs`. Put it above `resolve_module`, gated behind `#[cfg(feature = "wasm")]`:

```rust
/// Known Amplifier WIT interface names and their corresponding module types.
///
/// These are the versioned interface names embedded in WASM components by
/// `cargo component` when building against `wit/amplifier-modules.wit`.
#[cfg(feature = "wasm")]
const KNOWN_INTERFACES: &[(&str, ModuleType)] = &[
    ("amplifier:modules/tool", ModuleType::Tool),
    ("amplifier:modules/hook-handler", ModuleType::Hook),
    ("amplifier:modules/context-manager", ModuleType::Context),
    ("amplifier:modules/approval-provider", ModuleType::Approval),
    ("amplifier:modules/provider", ModuleType::Provider),
    ("amplifier:modules/orchestrator", ModuleType::Orchestrator),
];

/// Inspect a WASM component's exports to determine which Amplifier module type it implements.
///
/// Loads the component using the provided wasmtime engine, then iterates over
/// its exported interface names looking for matches against `KNOWN_INTERFACES`.
///
/// Returns `Ok(ModuleType)` if exactly one known interface is found.
/// Returns `Err` if zero or more than one known interface is exported.
#[cfg(feature = "wasm")]
pub(crate) fn detect_wasm_module_type(
    wasm_bytes: &[u8],
    engine: std::sync::Arc<wasmtime::Engine>,
    wasm_path: &Path,
) -> Result<ModuleType, ModuleResolverError> {
    let component = wasmtime::component::Component::new(&engine, wasm_bytes).map_err(|e| {
        ModuleResolverError::WasmLoadError {
            path: wasm_path.to_path_buf(),
            reason: format!("failed to compile WASM component: {e}"),
        }
    })?;

    // Get the component type and inspect exports
    let component_type = component.component_type();
    let mut found: Vec<(String, ModuleType)> = Vec::new();

    for (name, _export) in component_type.exports(&engine) {
        for (interface_prefix, module_type) in KNOWN_INTERFACES {
            // Export names may include the version suffix (e.g., "amplifier:modules/tool@1.0.0")
            // so we use starts_with to match the base name.
            if name.starts_with(interface_prefix) {
                found.push((name.to_string(), module_type.clone()));
            }
        }
    }

    match found.len() {
        0 => Err(ModuleResolverError::UnknownWasmInterface {
            path: wasm_path.to_path_buf(),
        }),
        1 => Ok(found.into_iter().next().unwrap().1),
        _ => Err(ModuleResolverError::AmbiguousWasmInterface {
            path: wasm_path.to_path_buf(),
            found: found.into_iter().map(|(name, _)| name).collect(),
        }),
    }
}
```

**Important:** You need to add `use std::sync::Arc;` at the top of the file if not already present. Also, the `ModuleType` enum needs `Clone` — check that it already has `#[derive(Debug, Clone, ...)]` in `models.rs`. (It does: `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`.)

### Step 4: Run tests to verify they pass

```bash
cargo test -p amplifier-core --features wasm -- detect_wasm_module_type -v
```

**Expected:** All 6 `detect_wasm_module_type_*` tests pass.

**NOTE:** If any test fails because the export name format doesn't match (e.g., it uses a different versioning scheme), read the actual error message. You may need to adjust the `starts_with` matching or add debug logging to see what the actual export names are. A quick debugging approach:

```rust
// Temporary debug: print all exports
for (name, _) in component_type.exports(&engine) {
    eprintln!("EXPORT: {name}");
}
```

Run with `cargo test ... -- --nocapture` to see the output.

### Step 5: Run clippy

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
```

### Step 6: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs
git commit -m "feat(resolver): add WASM component metadata parser for module type detection"
```

---

## Task 4: Python Package Detector

**Files:**
- Modify: `crates/amplifier-core/src/module_resolver.rs`

### Step 1: Write the failing test

Add to `mod tests`:

```rust
    #[test]
    fn detect_python_package_with_init_py() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("__init__.py"), b"# Python package").expect("write");

        let result = detect_python_package(dir.path());
        assert!(result.is_some());
        let name = result.unwrap();
        // Package name is derived from directory name
        assert!(!name.is_empty());
    }

    #[test]
    fn detect_python_package_with_nested_package() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let pkg_dir = dir.path().join("amplifier_module_tool_bash");
        std::fs::create_dir(&pkg_dir).expect("create pkg dir");
        std::fs::write(pkg_dir.join("__init__.py"), b"# Package").expect("write");

        let result = detect_python_package(dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn detect_python_package_empty_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let result = detect_python_package(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn detect_python_package_no_init_py() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("README.md"), b"hello").expect("write");
        std::fs::write(dir.path().join("main.py"), b"print('hi')").expect("write");

        let result = detect_python_package(dir.path());
        assert!(result.is_none());
    }
```

### Step 2: Run test to verify they fail

```bash
cargo test -p amplifier-core --features wasm -- detect_python -v
```

**Expected:** FAIL — `detect_python_package` doesn't exist.

### Step 3: Write the implementation

Add to `module_resolver.rs`:

```rust
/// Check if a directory contains a Python package (has `__init__.py`).
///
/// Checks two locations:
/// 1. `path/__init__.py` (the directory itself is a package)
/// 2. `path/<subdirectory>/__init__.py` (nested package, e.g., `amplifier_module_*`)
///
/// Returns the Python package name if found, or None.
pub(crate) fn detect_python_package(dir: &Path) -> Option<String> {
    // Check if the directory itself has __init__.py
    if dir.join("__init__.py").exists() {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown_module");
        return Some(name.replace('-', "_"));
    }

    // Check for a subdirectory with __init__.py (nested package)
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("__init__.py").exists() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown_module");
                return Some(name.replace('-', "_"));
            }
        }
    }

    None
}
```

### Step 4: Run tests to verify they pass

```bash
cargo test -p amplifier-core --features wasm -- detect_python -v
```

**Expected:** All 4 `detect_python_*` tests pass.

### Step 5: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs
git commit -m "feat(resolver): add Python package detector"
```

---

## Task 5: Orchestrate Detection — `resolve_module()`

**Files:**
- Modify: `crates/amplifier-core/src/module_resolver.rs`

This ties Tasks 1–4 together into the `resolve_module()` function.

### Step 1: Write the failing tests

Add to `mod tests`:

```rust
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
        let wasm_bytes = fixture("echo-tool.wasm");
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
```

### Step 2: Run test to verify they fail

```bash
cargo test -p amplifier-core --features wasm -- resolve_module -v
```

**Expected:** FAIL — `resolve_module` still has `todo!()`.

### Step 3: Write the implementation

Replace the `resolve_module` function body (remove the `todo!()`):

```rust
/// Resolve a module from a filesystem path.
///
/// Inspects the directory at `path` and returns a `ModuleManifest`
/// describing the transport, module type, and artifact location.
///
/// Detection order (first match wins):
/// 1. `amplifier.toml` (explicit override — always honored when present)
/// 2. `.wasm` files (auto-detect via Component Model metadata)
/// 3. Python package (`__init__.py` fallback)
/// 4. Error (clear guidance message)
pub fn resolve_module(path: &Path) -> Result<ModuleManifest, ModuleResolverError> {
    // Validate the path exists
    if !path.exists() {
        return Err(ModuleResolverError::PathNotFound {
            path: path.to_path_buf(),
        });
    }

    // Step 1: Check for amplifier.toml (explicit override)
    let toml_path = path.join("amplifier.toml");
    if toml_path.exists() {
        let content = std::fs::read_to_string(&toml_path).map_err(|e| ModuleResolverError::Io {
            path: toml_path.clone(),
            source: e,
        })?;
        return parse_amplifier_toml(&content, path);
    }

    // Step 2: Check for .wasm files
    if let Some(wasm_path) = scan_for_wasm_file(path) {
        let bytes = std::fs::read(&wasm_path).map_err(|e| ModuleResolverError::Io {
            path: wasm_path.clone(),
            source: e,
        })?;

        // Detect module type from WASM component metadata
        #[cfg(feature = "wasm")]
        {
            let engine = crate::wasm_engine::WasmEngine::new().map_err(|e| {
                ModuleResolverError::WasmLoadError {
                    path: wasm_path.clone(),
                    reason: format!("failed to create WASM engine: {e}"),
                }
            })?;
            let module_type = detect_wasm_module_type(&bytes, engine.inner(), &wasm_path)?;
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
        {
            return Err(ModuleResolverError::WasmLoadError {
                path: wasm_path,
                reason: "WASM support not enabled. Compile with --features wasm".into(),
            });
        }
    }

    // Step 3: Check for Python package
    if let Some(package_name) = detect_python_package(path) {
        return Ok(ModuleManifest {
            transport: Transport::Python,
            module_type: ModuleType::Tool, // Default; Python side will refine
            artifact: ModuleArtifact::PythonModule(package_name),
        });
    }

    // Step 4: Nothing found
    Err(ModuleResolverError::NoArtifactFound {
        path: path.to_path_buf(),
    })
}
```

### Step 4: Run tests to verify they pass

```bash
cargo test -p amplifier-core --features wasm -- resolve_module -v
```

**Expected:** All 5 `resolve_module_*` tests pass.

### Step 5: Run clippy

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
```

### Step 6: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs
git commit -m "feat(resolver): implement resolve_module() detection pipeline"
```

---

## Task 6: `load_module()` Dispatch Function

**Files:**
- Modify: `crates/amplifier-core/src/module_resolver.rs`

### Step 1: Write the failing test

Add to `mod tests`:

```rust
    #[cfg(feature = "wasm")]
    #[tokio::test]
    async fn load_module_wasm_tool() {
        // Create a temp dir with a real fixture
        let dir = tempfile::tempdir().expect("create temp dir");
        let wasm_bytes = fixture("echo-tool.wasm");
        std::fs::write(dir.path().join("echo-tool.wasm"), &wasm_bytes).expect("write wasm");

        let manifest = resolve_module(dir.path()).expect("should resolve");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let coordinator = std::sync::Arc::new(crate::coordinator::Coordinator::new_for_test());
        let result = load_module(&manifest, engine.inner(), Some(coordinator));
        assert!(result.is_ok());
        match result.unwrap() {
            LoadedModule::Tool(tool) => assert_eq!(tool.name(), "echo-tool"),
            other => panic!("expected Tool, got {:?}", other.variant_name()),
        }
    }

    #[test]
    fn load_module_python_returns_signal() {
        let manifest = ModuleManifest {
            transport: Transport::Python,
            module_type: ModuleType::Tool,
            artifact: ModuleArtifact::PythonModule("my_tool".into()),
        };
        // Python loading should NOT be handled in Rust — return a signal
        let engine_placeholder = crate::wasm_engine::WasmEngine::new().unwrap();
        let result = load_module(&manifest, engine_placeholder.inner(), None);
        assert!(result.is_ok());
        match result.unwrap() {
            LoadedModule::PythonDelegated { package_name } => {
                assert_eq!(package_name, "my_tool");
            }
            other => panic!("expected PythonDelegated, got {:?}", other.variant_name()),
        }
    }
```

### Step 2: Run test to verify they fail

```bash
cargo test -p amplifier-core --features wasm -- load_module -v
```

**Expected:** FAIL — `load_module` and `LoadedModule` don't exist.

### Step 3: Write the implementation

Add to `module_resolver.rs`:

```rust
/// The result of loading a module through the resolver.
///
/// For WASM and gRPC, returns a loaded `Arc<dyn Trait>`.
/// For Python, returns a signal that the caller should use importlib.
#[cfg(feature = "wasm")]
pub enum LoadedModule {
    Tool(std::sync::Arc<dyn crate::traits::Tool>),
    Hook(std::sync::Arc<dyn crate::traits::HookHandler>),
    Context(std::sync::Arc<dyn crate::traits::ContextManager>),
    Approval(std::sync::Arc<dyn crate::traits::ApprovalProvider>),
    Provider(std::sync::Arc<dyn crate::traits::Provider>),
    Orchestrator(std::sync::Arc<dyn crate::traits::Orchestrator>),
    /// Python modules can't be loaded in Rust — this signals the caller
    /// to use importlib on the Python side.
    PythonDelegated { package_name: String },
}

#[cfg(feature = "wasm")]
impl LoadedModule {
    /// Return a string name for the variant (for debug/error messages).
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

/// Load a module from a resolved `ModuleManifest`.
///
/// Dispatches to the correct `load_wasm_*` or `load_grpc_*` function
/// from `transport.rs` based on the manifest's transport and module type.
///
/// For Python transport, returns `LoadedModule::PythonDelegated` — the
/// Python host should handle loading via importlib.
///
/// # Arguments
///
/// * `manifest` — The resolved module manifest from `resolve_module()`.
/// * `engine` — A shared wasmtime Engine (from `WasmEngine::new().inner()`).
/// * `coordinator` — Optional coordinator (required only for orchestrator modules).
#[cfg(feature = "wasm")]
pub fn load_module(
    manifest: &ModuleManifest,
    engine: std::sync::Arc<wasmtime::Engine>,
    coordinator: Option<std::sync::Arc<crate::coordinator::Coordinator>>,
) -> Result<LoadedModule, Box<dyn std::error::Error + Send + Sync>> {
    match &manifest.transport {
        Transport::Python | Transport::Native => {
            // Python modules are loaded by the Python host via importlib
            if let ModuleArtifact::PythonModule(name) = &manifest.artifact {
                return Ok(LoadedModule::PythonDelegated {
                    package_name: name.clone(),
                });
            }
            Err("Python transport but artifact is not PythonModule".into())
        }
        Transport::Wasm => {
            let bytes = match &manifest.artifact {
                ModuleArtifact::WasmBytes { bytes, .. } => bytes,
                _ => return Err("WASM transport but artifact is not WasmBytes".into()),
            };
            match &manifest.module_type {
                ModuleType::Tool => {
                    let loaded = crate::transport::load_wasm_tool(bytes, engine)?;
                    Ok(LoadedModule::Tool(loaded))
                }
                ModuleType::Hook => {
                    let loaded = crate::transport::load_wasm_hook(bytes, engine)?;
                    Ok(LoadedModule::Hook(loaded))
                }
                ModuleType::Context => {
                    let loaded = crate::transport::load_wasm_context(bytes, engine)?;
                    Ok(LoadedModule::Context(loaded))
                }
                ModuleType::Approval => {
                    let loaded = crate::transport::load_wasm_approval(bytes, engine)?;
                    Ok(LoadedModule::Approval(loaded))
                }
                ModuleType::Provider => {
                    let loaded = crate::transport::load_wasm_provider(bytes, engine)?;
                    Ok(LoadedModule::Provider(loaded))
                }
                ModuleType::Orchestrator => {
                    let coord = coordinator.ok_or(
                        "Orchestrator modules require a Coordinator, but none was provided",
                    )?;
                    let loaded = crate::transport::load_wasm_orchestrator(bytes, engine, coord)?;
                    Ok(LoadedModule::Orchestrator(loaded))
                }
                ModuleType::Resolver => {
                    Err("Resolver modules are not loadable via WASM transport".into())
                }
            }
        }
        Transport::Grpc => {
            // gRPC loading is async — for now return an error indicating
            // the caller should use the async load_grpc_* functions directly.
            // A full async load_module is deferred to avoid changing the sync API.
            Err("gRPC module loading requires async runtime. Use load_grpc_tool() / load_grpc_orchestrator() directly.".into())
        }
    }
}
```

### Step 4: Run tests to verify they pass

```bash
cargo test -p amplifier-core --features wasm -- load_module -v
```

**Expected:** Both `load_module_*` tests pass.

### Step 5: Run full test suite

```bash
cargo test -p amplifier-core --features wasm
```

**Expected:** All existing tests still pass, plus all new module_resolver tests.

### Step 6: Run clippy

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
```

### Step 7: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs
git commit -m "feat(resolver): add load_module() dispatch function"
```

---

## Task 7: PyO3 Bindings — Expose `resolve_module()` and `load_module()` to Python

**Files:**
- Modify: `bindings/python/Cargo.toml` (add `wasm` feature)
- Modify: `bindings/python/src/lib.rs` (add functions)

### Step 1: Update `bindings/python/Cargo.toml`

Change the `amplifier-core` dependency to enable the `wasm` feature:

```toml
amplifier-core = { path = "../../crates/amplifier-core", features = ["wasm"] }
```

### Step 2: Write the Python-facing functions

Open `bindings/python/src/lib.rs`. At the very bottom of the file, just before the closing `}` of the module (or after the last `#[pymethods]` block), add these standalone `#[pyfunction]` functions:

```rust
// ---------------------------------------------------------------------------
// Module resolver bindings (Phase 4)
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
    // Transport
    let transport_str = match manifest.transport {
        amplifier_core::transport::Transport::Python => "python",
        amplifier_core::transport::Transport::Wasm => "wasm",
        amplifier_core::transport::Transport::Grpc => "grpc",
        amplifier_core::transport::Transport::Native => "native",
    };
    dict.set_item("transport", transport_str)?;

    // Module type
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

    // Artifact
    match &manifest.artifact {
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

/// Load a WASM module from a resolved manifest path and mount it on the coordinator.
///
/// This is the Rust-side loading for WASM modules. Python modules are handled
/// by importlib on the Python side — this function returns an error for Python transport.
///
/// Returns a dict with "status" = "loaded" and "module_type" on success.
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

    let engine = amplifier_core::wasm_engine::WasmEngine::new()
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("WASM engine creation failed: {e}")))?;

    let coordinator = std::sync::Arc::new(amplifier_core::Coordinator::new_for_test());
    let loaded = amplifier_core::module_resolver::load_module(&manifest, engine.inner(), Some(coordinator))
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Module loading failed: {e}")))?;

    let dict = PyDict::new(py);
    dict.set_item("status", "loaded")?;
    dict.set_item("module_type", loaded.variant_name())?;
    Ok(dict.unbind())
}
```

Then register these functions in the `#[pymodule]` function. Find the `_engine` module function (it should be at the bottom of the file). If it looks like:

```rust
#[pymodule]
fn _engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySession>()?;
    // ...
    Ok(())
}
```

Add:

```rust
    m.add_function(wrap_pyfunction!(resolve_module, m)?)?;
    m.add_function(wrap_pyfunction!(load_wasm_from_path, m)?)?;
```

**NOTE:** If the `#[pymodule]` function isn't at the bottom of the file, search for `#[pymodule]` to find it. Read the file first to confirm the exact structure before editing.

### Step 3: Verify it compiles

```bash
cd /home/bkrabach/dev/rust-devrust-core/amplifier-core
cargo check -p amplifier-core-py
```

**Expected:** Compiles without errors.

### Step 4: Run clippy

```bash
cargo clippy -p amplifier-core-py -- -D warnings
```

### Step 5: Commit

```bash
git add bindings/python/Cargo.toml bindings/python/src/lib.rs
git commit -m "feat(resolver): expose resolve_module() and load_wasm_from_path() via PyO3"
```

---

## Task 8: Update `loader_dispatch.py`

**Files:**
- Modify: `python/amplifier_core/loader_dispatch.py`

### Step 1: Read the current file

```bash
cat python/amplifier_core/loader_dispatch.py
```

Verify it still has the `NotImplementedError` for WASM and native transports.

### Step 2: Update the file

Replace the entire `load_module` function in `loader_dispatch.py` with:

```python
async def load_module(
    module_id: str,
    config: dict[str, Any] | None,
    source_path: str,
    coordinator: Any,
) -> Any:
    """Load a module from a resolved source path.

    Uses the Rust module resolver to auto-detect transport type.
    Falls back to Python loader for backward compatibility.

    Args:
        module_id: Module identifier (e.g., "tool-database")
        config: Optional module configuration dict
        source_path: Resolved filesystem path to the module
        coordinator: The coordinator instance (RustCoordinator or ModuleCoordinator)

    Returns:
        Mount function for the module

    Raises:
        NotImplementedError: For transport types not yet supported
        ValueError: If module cannot be loaded
    """
    # Try Rust resolver first for auto-detection
    try:
        from amplifier_core._engine import resolve_module as rust_resolve

        manifest = rust_resolve(source_path)
        transport = manifest.get("transport", "python")
    except ImportError:
        # Rust engine not available — fall back to TOML-based detection
        logger.debug("Rust engine not available, using Python-only transport detection")
        transport = _detect_transport(source_path)
    except Exception as e:
        # Rust resolver failed — fall back to TOML-based detection
        logger.debug(f"Rust resolver failed for '{module_id}': {e}, falling back to Python detection")
        transport = _detect_transport(source_path)

    if transport == "grpc":
        from .loader_grpc import load_grpc_module

        meta = _read_module_meta(source_path)
        return await load_grpc_module(module_id, config, meta, coordinator)

    if transport == "wasm":
        try:
            from amplifier_core._engine import load_wasm_from_path

            result = load_wasm_from_path(source_path)
            logger.info(
                f"[module:mount] {module_id} loaded via WASM resolver: {result}"
            )
            # WASM modules are loaded into the Rust coordinator directly.
            # Return a no-op mount function since the module is already loaded.
            async def _noop_mount(coord: Any) -> None:
                pass
            return _noop_mount
        except ImportError:
            raise NotImplementedError(
                f"WASM module loading for '{module_id}' requires the Rust engine. "
                "Install amplifier-core with Rust extensions enabled."
            )
        except Exception as e:
            raise ValueError(
                f"Failed to load WASM module '{module_id}' from {source_path}: {e}"
            ) from e

    if transport == "native":
        raise NotImplementedError(
            f"Native Rust module loading not yet implemented for '{module_id}'. "
            "Use transport = 'grpc' to load Rust modules as gRPC services."
        )

    # Default: existing Python loader (backward compatible)
    from .loader import ModuleLoader

    loader = coordinator.loader or ModuleLoader(coordinator=coordinator)
    return await loader.load(module_id, config, source_hint=source_path)
```

### Step 3: Verify Python syntax

```bash
python3 -c "import ast; ast.parse(open('python/amplifier_core/loader_dispatch.py').read()); print('OK')"
```

**Expected:** Prints `OK`.

### Step 4: Commit

```bash
git add python/amplifier_core/loader_dispatch.py
git commit -m "feat(resolver): wire WASM/gRPC branches to Rust resolver in loader_dispatch.py"
```

---

## Task 9: Napi-RS Bindings — Expose `resolveModule()` and `loadModule()` to TypeScript

**Files:**
- Modify: `bindings/node/Cargo.toml` (add `wasm` feature)
- Modify: `bindings/node/src/lib.rs` (add functions)

### Step 1: Update `bindings/node/Cargo.toml`

Change the `amplifier-core` dependency to enable the `wasm` feature:

```toml
amplifier-core = { path = "../../crates/amplifier-core", features = ["wasm"] }
```

### Step 2: Add the Napi-RS functions

Open `bindings/node/src/lib.rs`. Add at the bottom, before the closing of the file:

```rust
// ---------------------------------------------------------------------------
// Module resolver bindings (Phase 4)
// ---------------------------------------------------------------------------

/// Result from resolving a module path.
#[napi(object)]
pub struct JsModuleManifest {
    /// Transport type: "python", "wasm", "grpc", "native"
    pub transport: String,
    /// Module type: "tool", "hook", "context", "approval", "provider", "orchestrator"
    pub module_type: String,
    /// Artifact type: "wasm", "grpc", "python"
    pub artifact_type: String,
    /// Path to WASM artifact (if artifact_type is "wasm")
    pub artifact_path: Option<String>,
    /// gRPC endpoint (if artifact_type is "grpc")
    pub endpoint: Option<String>,
    /// Python package name (if artifact_type is "python")
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
        amplifier_core::ModuleType::Tool => "tool",
        amplifier_core::ModuleType::Hook => "hook",
        amplifier_core::ModuleType::Context => "context",
        amplifier_core::ModuleType::Approval => "approval",
        amplifier_core::ModuleType::Provider => "provider",
        amplifier_core::ModuleType::Orchestrator => "orchestrator",
        amplifier_core::ModuleType::Resolver => "resolver",
    };

    let (artifact_type, artifact_path, endpoint, package_name) = match &manifest.artifact {
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
             TypeScript hosts cannot load Python modules."
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
    let loaded = amplifier_core::module_resolver::load_module(
        &manifest,
        engine.inner(),
        Some(coordinator),
    )
    .map_err(|e| Error::from_reason(format!("Module loading failed: {e}")))?;

    Ok(format!("loaded:{}", loaded.variant_name()))
}
```

### Step 3: Verify it compiles

```bash
cargo check -p amplifier-core-node
```

**Expected:** Compiles without errors.

### Step 4: Run clippy

```bash
cargo clippy -p amplifier-core-node -- -D warnings
```

### Step 5: Commit

```bash
git add bindings/node/Cargo.toml bindings/node/src/lib.rs
git commit -m "feat(resolver): expose resolveModule() and loadWasmFromPath() via Napi-RS"
```

---

## Task 10: Integration Tests — Full Pipeline E2E

**Files:**
- Create: `crates/amplifier-core/tests/module_resolver_e2e.rs`

### Step 1: Create the integration test file

Create `crates/amplifier-core/tests/module_resolver_e2e.rs`:

```rust
//! Module resolver E2E integration tests.
//!
//! Tests the full pipeline: resolve_module() → detect type → load → verify.
//!
//! Run with: cargo test -p amplifier-core --features wasm --test module_resolver_e2e

#![cfg(feature = "wasm")]

use std::path::Path;
use std::sync::Arc;

use amplifier_core::models::ModuleType;
use amplifier_core::module_resolver::{
    resolve_module, load_module, ModuleArtifact, LoadedModule, ModuleResolverError,
};
use amplifier_core::transport::Transport;
use amplifier_core::wasm_engine::WasmEngine;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../tests/fixtures/wasm").join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture '{}' not found at {}: {}", name, path.display(), e))
}

fn make_engine() -> Arc<wasmtime::Engine> {
    WasmEngine::new()
        .expect("WasmEngine::new() should succeed")
        .inner()
}

/// Create a temp dir with a single .wasm fixture copied into it.
fn dir_with_wasm(fixture_name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp dir");
    let bytes = fixture(fixture_name);
    std::fs::write(dir.path().join(fixture_name), &bytes).expect("write fixture");
    dir
}

// ---------------------------------------------------------------------------
// Test: resolve + detect type for each of the 6 WASM module types
// ---------------------------------------------------------------------------

#[test]
fn resolve_wasm_tool() {
    let dir = dir_with_wasm("echo-tool.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Tool);
}

#[test]
fn resolve_wasm_hook() {
    let dir = dir_with_wasm("deny-hook.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Hook);
}

#[test]
fn resolve_wasm_context() {
    let dir = dir_with_wasm("memory-context.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Context);
}

#[test]
fn resolve_wasm_approval() {
    let dir = dir_with_wasm("auto-approve.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Approval);
}

#[test]
fn resolve_wasm_provider() {
    let dir = dir_with_wasm("echo-provider.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Provider);
}

#[test]
fn resolve_wasm_orchestrator() {
    let dir = dir_with_wasm("passthrough-orchestrator.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Orchestrator);
}

// ---------------------------------------------------------------------------
// Test: Python package detection
// ---------------------------------------------------------------------------

#[test]
fn resolve_python_package() {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(dir.path().join("__init__.py"), b"# package").expect("write");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Python);
    assert_eq!(manifest.module_type, ModuleType::Tool); // default for Python
    match &manifest.artifact {
        ModuleArtifact::PythonModule(_) => {} // OK
        other => panic!("expected PythonModule, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Test: amplifier.toml gRPC detection
// ---------------------------------------------------------------------------

#[test]
fn resolve_amplifier_toml_grpc() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let toml = r#"
[module]
transport = "grpc"
type = "tool"

[grpc]
endpoint = "http://localhost:50051"
"#;
    std::fs::write(dir.path().join("amplifier.toml"), toml).expect("write toml");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    assert_eq!(manifest.transport, Transport::Grpc);
    assert_eq!(manifest.module_type, ModuleType::Tool);
    match &manifest.artifact {
        ModuleArtifact::GrpcEndpoint(ep) => assert_eq!(ep, "http://localhost:50051"),
        other => panic!("expected GrpcEndpoint, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Test: amplifier.toml overrides auto-detection
// ---------------------------------------------------------------------------

#[test]
fn resolve_amplifier_toml_overrides_auto() {
    let dir = tempfile::tempdir().expect("create temp dir");

    // Put both a .wasm file AND an amplifier.toml — TOML should win
    let bytes = fixture("echo-tool.wasm");
    std::fs::write(dir.path().join("echo-tool.wasm"), &bytes).expect("write wasm");

    let toml = r#"
[module]
transport = "grpc"
type = "provider"

[grpc]
endpoint = "http://override:1234"
"#;
    std::fs::write(dir.path().join("amplifier.toml"), toml).expect("write toml");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    // TOML takes priority over .wasm auto-detection
    assert_eq!(manifest.transport, Transport::Grpc);
    assert_eq!(manifest.module_type, ModuleType::Provider);
}

// ---------------------------------------------------------------------------
// Test: error cases
// ---------------------------------------------------------------------------

#[test]
fn resolve_empty_dir_errors() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let result = resolve_module(dir.path());
    assert!(result.is_err());

    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("could not detect"), "error: {msg}");
}

#[test]
fn resolve_nonexistent_path_errors() {
    let result = resolve_module(Path::new("/nonexistent/path/module"));
    assert!(result.is_err());

    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("does not exist"), "error: {msg}");
}

// ---------------------------------------------------------------------------
// Test: full pipeline — resolve → load → execute (echo-tool roundtrip)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn load_module_wasm_tool_e2e() {
    let dir = dir_with_wasm("echo-tool.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");

    let engine = make_engine();
    let coordinator = Arc::new(amplifier_core::Coordinator::new_for_test());
    let loaded = load_module(&manifest, engine, Some(coordinator))
        .expect("should load");

    match loaded {
        LoadedModule::Tool(tool) => {
            // Verify the tool loaded correctly
            assert_eq!(tool.name(), "echo-tool");

            // Execute it and verify roundtrip
            let input = serde_json::json!({"message": "hello from resolver", "count": 7});
            let result = tool.execute(input.clone()).await.expect("execute should succeed");
            assert!(result.success);
            assert_eq!(result.output, Some(input));
        }
        other => panic!("expected Tool, got {}", other.variant_name()),
    }
}

#[tokio::test]
async fn load_module_wasm_hook_e2e() {
    let dir = dir_with_wasm("deny-hook.wasm");
    let manifest = resolve_module(dir.path()).expect("should resolve");

    let engine = make_engine();
    let loaded = load_module(&manifest, engine, None).expect("should load");

    match loaded {
        LoadedModule::Hook(_hook) => {
            // Hook loaded successfully — that's the assertion
        }
        other => panic!("expected Hook, got {}", other.variant_name()),
    }
}

#[test]
fn load_module_python_returns_delegated() {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(dir.path().join("__init__.py"), b"# package").expect("write");

    let manifest = resolve_module(dir.path()).expect("should resolve");
    let engine = make_engine();
    let loaded = load_module(&manifest, engine, None).expect("should load");

    match loaded {
        LoadedModule::PythonDelegated { package_name } => {
            assert!(!package_name.is_empty());
        }
        other => panic!("expected PythonDelegated, got {}", other.variant_name()),
    }
}
```

### Step 2: Run the integration tests

```bash
cargo test -p amplifier-core --features wasm --test module_resolver_e2e -v
```

**Expected:** All tests pass. If any fail, read the error message and fix.

### Step 3: Run the full test suite to ensure nothing is broken

```bash
cargo test -p amplifier-core --features wasm
```

**Expected:** All tests pass (existing + new).

### Step 4: Run clippy on everything

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core-py -- -D warnings
cargo clippy -p amplifier-core-node -- -D warnings
```

**Expected:** Clean across all three crates.

### Step 5: Commit

```bash
git add crates/amplifier-core/tests/module_resolver_e2e.rs
git commit -m "test(resolver): add E2E integration tests for full resolve → load → execute pipeline"
```

---

## Final Checklist

After all 11 tasks are complete, verify:

1. **All Rust tests pass:**
   ```bash
   cargo test -p amplifier-core --features wasm
   ```

2. **All integration tests pass:**
   ```bash
   cargo test -p amplifier-core --features wasm --test module_resolver_e2e -v
   cargo test -p amplifier-core --features wasm --test wasm_e2e -v
   ```

3. **Clippy is clean:**
   ```bash
   cargo clippy -p amplifier-core --features wasm -- -D warnings
   cargo clippy -p amplifier-core-py -- -D warnings
   cargo clippy -p amplifier-core-node -- -D warnings
   ```

4. **Python syntax is valid:**
   ```bash
   python3 -c "import ast; ast.parse(open('python/amplifier_core/loader_dispatch.py').read()); print('OK')"
   ```

5. **Git log shows clean conventional commits:**
   ```bash
   git log --oneline -11
   ```

   Expected (newest first):
   ```
   test(resolver): add E2E integration tests for full resolve → load → execute pipeline
   feat(resolver): expose resolveModule() and loadWasmFromPath() via Napi-RS
   feat(resolver): wire WASM/gRPC branches to Rust resolver in loader_dispatch.py
   feat(resolver): expose resolve_module() and load_wasm_from_path() via PyO3
   feat(resolver): add load_module() dispatch function
   feat(resolver): implement resolve_module() detection pipeline
   feat(resolver): add Python package detector
   feat(resolver): add WASM component metadata parser for module type detection
   feat(resolver): add .wasm file scanner
   feat(resolver): add amplifier.toml reader with TOML parsing
   feat(resolver): add ModuleManifest, ModuleArtifact types and module_resolver skeleton
   ```

---

## Summary of New/Modified Files

| Action | File |
|---|---|
| **Create** | `crates/amplifier-core/src/module_resolver.rs` |
| **Create** | `crates/amplifier-core/tests/module_resolver_e2e.rs` |
| Modify | `crates/amplifier-core/src/lib.rs` (add `pub mod module_resolver;`) |
| Modify | `crates/amplifier-core/src/models.rs` (add `Approval` variant to `ModuleType`) |
| Modify | `crates/amplifier-core/Cargo.toml` (add `toml` + `tempfile` deps) |
| Modify | `bindings/python/Cargo.toml` (add `wasm` feature) |
| Modify | `bindings/python/src/lib.rs` (add `resolve_module`, `load_wasm_from_path`) |
| Modify | `bindings/node/Cargo.toml` (add `wasm` feature) |
| Modify | `bindings/node/src/lib.rs` (add `resolve_module`, `load_wasm_from_path`) |
| Modify | `python/amplifier_core/loader_dispatch.py` (wire WASM/gRPC to Rust) |
