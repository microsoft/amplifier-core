# Phase 4: Cross-Language Module Resolver Design

> Automatic transport detection and module loading — developers write `{"module": "tool-slack"}` and the framework handles everything.

**Status:** Approved
**Date:** 2026-03-05
**Phase:** 4 of 5 (Cross-Language SDK)
**Parent design:** `docs/plans/2026-03-02-cross-language-session-sdk-design.md`
**Prerequisites:** PR #35 (Phase 2 — Napi-RS/TypeScript bindings + wasmtime 42), PR #36 (gRPC v2 debt fix), PR #38 (Phase 3 — WASM module loading)

---

## 1. Goal

Implement the cross-language module resolver that makes transport invisible to developers. Given a resolved filesystem path to a module, automatically detect the transport (Python, WASM, gRPC) and module type (Tool, Provider, Orchestrator, etc.), then load it through the correct bridge. Developers write `{"module": "tool-slack"}` in bundle YAML and the framework handles everything.

---

## 2. Background

This is Phase 4 of the 5-phase Cross-Language SDK plan. Phase 4 is the **glue layer** — it connects two systems that currently exist side by side:

- **Python side:** `loader.py` → `loader_dispatch.py` → `importlib` (resolves module IDs to Python packages)
- **Rust side:** `transport.rs` with `load_wasm_*` and `load_grpc_*` functions (loads modules from bytes/endpoints into `Arc<dyn Trait>`)

Phase 4 connects them: given a resolved module path, auto-detect the language/transport and route to the correct Rust loader.

**Dependencies (all complete):**

- **Phase 1 (complete):** Python/PyO3 bridge
- **PR #35 / Phase 2 (merged to dev):** TypeScript/Napi-RS bindings + wasmtime 42
- **PR #36 (merged to dev):** Full bridge fidelity + all 9 KernelService RPCs
- **PR #38 / Phase 3 (on dev):** WASM module loading — all 6 module types via Component Model

**Current state:** The `loader_dispatch.py` has the routing skeleton (reads `amplifier.toml`, checks transport), but the WASM and native branches raise `NotImplementedError`. The `ModuleSource` protocol returns a `Path`, which works for Python but needs extending for WASM/gRPC. All work happens on the `dev/cross-language-sdk` branch.

---

## 3. Key Design Decisions

1. **Split architecture** — Rust does transport detection (pure logic), Python/TS foundation resolves URIs to paths (I/O, unchanged). Clear ownership at the boundary: foundation returns a `Path`, kernel takes it from there. This follows CORE_DEVELOPMENT_PRINCIPLES §5: "logic goes in Rust, not in bindings."

2. **Parse WASM component WIT metadata** for module type detection — the Component Model embeds interface names in the binary. Self-describing, zero configuration. No naming conventions or extra manifest files needed for WASM modules.

3. **Three runtime transport paths:**
   - **Python** → `importlib` (existing behavior, backward compatible)
   - **WASM** → wasmtime `load_wasm_*` functions (from Phase 3)
   - **gRPC** → `load_grpc_*` functions (explicit opt-in via `amplifier.toml`)

   No runtime "native Rust" path (that's compile-time linking, not discovery). No auto-compilation of source code — the resolver discovers pre-built artifacts.

4. **Serves both Python and TypeScript hosts** — Rust resolver exposed via PyO3 AND Napi-RS. TypeScript host apps get the same auto-detection for free. This was reinforced by the existing TypeScript/Node bindings from Phase 2 being a concrete second consumer.

5. **`amplifier.toml` remains optional** — auto-detection is the primary path; explicit declaration is an override/escape hatch (especially for gRPC endpoints).

6. **Foundation layer unchanged** — `ModuleSourceResolver` protocol, `SimpleSourceResolver`, bundle YAML format all stay the same. The resolver operates on the output of foundation resolution (a filesystem path), not the input.

---

## 4. Resolver Pipeline

Three stages with clear ownership:

```
Bundle YAML                Foundation              Rust Kernel
{"module": "tool-slack",   resolve URI →           inspect path →
 "source": "git+..."}      filesystem Path         detect transport →
                                                    load module →
                                                    Arc<dyn Tool>
```

### Stage 1: URI Resolution (Foundation — Python/TS, unchanged)

`source_hint` → filesystem `Path`. Git clone, local path resolution, package lookup. Already works. No changes needed.

### Stage 2: Transport Detection (Rust kernel — new `module_resolver.rs`)

Given a `Path`, inspect its contents and determine:

- What transport to use (Python, WASM, gRPC)
- What module type it is (Tool, Provider, Orchestrator, etc.)
- Where the loadable artifact is (`.wasm` file path, gRPC endpoint, Python package name)

### Stage 3: Module Loading (Rust kernel — existing `transport.rs`)

Call the appropriate `load_wasm_*` / `load_grpc_*` function with the detected parameters. Returns `Arc<dyn Trait>`.

### ModuleManifest — The Resolver's Output

```rust
pub struct ModuleManifest {
    pub transport: Transport,          // Python | Wasm | Grpc
    pub module_type: ModuleType,       // Tool | Provider | Orchestrator | etc.
    pub artifact: ModuleArtifact,      // WasmBytes(Vec<u8>) | GrpcEndpoint(String) | PythonModule(String)
}
```

The Python `loader_dispatch.py` calls into Rust via PyO3 to get a `ModuleManifest`, then either loads the WASM/gRPC module directly in Rust or falls through to the existing Python importlib path.

---

## 5. Transport Detection Logic

**New file:** `crates/amplifier-core/src/module_resolver.rs`

The resolver takes a filesystem path and returns a `ModuleManifest`. Detection is ordered — first match wins:

### Step 1: Check for `amplifier.toml` (explicit override)

- If present, read `transport` and `type` fields
- For gRPC: read `[grpc] endpoint`
- Always honored when present — this is the escape hatch

### Step 2: Check for `.wasm` files

- Scan the directory for `*.wasm` files
- If found, parse the WASM component's embedded WIT metadata using `wasmtime::component::Component::new()` + inspect exports
- Match exported interface names against known Amplifier interfaces to determine module type
- Return `Transport::Wasm` with the artifact bytes

### Step 3: Check for Python package

- Look for `__init__.py` or a `mount()` function pattern
- Return `Transport::Python` with the package name
- Backward-compatible fallback for the existing ecosystem

### Step 4: No match → error

- Clear error: "Could not detect module transport at path X. Expected: .wasm file, amplifier.toml, or Python package."

### Source code files are not loadable artifacts

`Cargo.toml`, `package.json`, `go.mod` indicate source code, not loadable artifacts. The resolver doesn't compile — it discovers pre-built artifacts. A Rust module author runs `cargo component build` before publishing; the resolver finds the resulting `.wasm`. If they haven't built, the error message guides them.

---

## 6. WASM Component Metadata Parsing

How the resolver determines module type from a `.wasm` file:

1. **Load the component** using `wasmtime::component::Component::new(&engine, &bytes)` (reuses shared `WasmEngine` from Phase 3)
2. **Inspect the component's exports** — component type metadata reveals which interfaces are exported
3. **Match against known Amplifier interface names:**

| Exported interface | Module type detected |
|---|---|
| `amplifier:modules/tool` | `ModuleType::Tool` |
| `amplifier:modules/hook-handler` | `ModuleType::Hook` |
| `amplifier:modules/context-manager` | `ModuleType::Context` |
| `amplifier:modules/approval-provider` | `ModuleType::Approval` |
| `amplifier:modules/provider` | `ModuleType::Provider` |
| `amplifier:modules/orchestrator` | `ModuleType::Orchestrator` |

4. **If no match** → error: "WASM component does not export any known Amplifier module interface"
5. **If multiple matches** → error (a component should implement exactly one module type)

Module authors compile with `amplifier_guest::export_tool!(MyTool)` → the macro exports the `amplifier:modules/tool` interface → the resolver reads it back. Self-describing, zero configuration.

---

## 7. PyO3 + Napi-RS Bindings

The resolver is Rust code, exposed to both host languages.

### PyO3 Binding (Python hosts)

```python
from amplifier_core._engine import resolve_module

manifest = resolve_module("/path/to/resolved/module")
# Returns: {"transport": "wasm", "module_type": "tool", "artifact_path": "/path/to/tool.wasm"}
```

`loader_dispatch.py` becomes a thin wrapper:

1. Foundation resolves source URI → filesystem path (unchanged)
2. Call `resolve_module(path)` → get `ModuleManifest` from Rust
3. If `transport == "python"` → existing `importlib` path (unchanged)
4. If `transport == "wasm"` → call `load_wasm_module(manifest)` in Rust via PyO3 → `Arc<dyn Trait>` mounted on coordinator
5. If `transport == "grpc"` → call `load_grpc_module(manifest)` in Rust via PyO3

### Napi-RS Binding (TypeScript hosts)

```typescript
import { resolveModule, loadModule } from '@amplifier/core';

const manifest = resolveModule('/path/to/module');
if (manifest.transport === 'wasm' || manifest.transport === 'grpc') {
    loadModule(coordinator, manifest);
}
```

### Cross-host constraint

The TypeScript host can't load Python modules (no `importlib`). If the resolver detects a Python module from a TS host, it returns an error with guidance: "Python module detected — compile to WASM or run as gRPC sidecar." This is a natural consequence of the three-path model.

---

## 8. Integration with Existing Loader Chain

Minimal changes to wire everything together.

### Python side — `loader_dispatch.py` changes

**Today's flow:**

```
_session_init.py → loader.load(module_id, config, source_hint)
  → loader_dispatch.py._detect_transport(path) → reads amplifier.toml
  → if python: importlib path
  → if grpc: loader_grpc.py
  → if wasm: NotImplementedError ❌
```

**Phase 4 flow:**

```
_session_init.py → loader.load(module_id, config, source_hint)
  → Foundation resolves source_hint → filesystem Path (unchanged)
  → Call Rust: resolve_module(path) → ModuleManifest
  → if python: importlib path (unchanged)
  → if wasm: Call Rust: load_wasm_module(manifest) → Arc<dyn Trait> on coordinator
  → if grpc: Call Rust: load_grpc_module(manifest) → Arc<dyn Trait> on coordinator
```

### TypeScript side — new `resolveAndLoadModule()` in Napi-RS

```typescript
const manifest = resolveModule('/path/to/module');
if (manifest.transport === 'wasm' || manifest.transport === 'grpc') {
    loadModule(coordinator, manifest);
}
```

### What stays unchanged

- Bundle YAML format — zero config changes
- Foundation source URI resolution — still resolves `git+https://...` to paths
- `ModuleSourceResolver` protocol — still returns paths
- Python module loading via `importlib` — the Python path is untouched
- All existing Python modules work exactly as before

### What's new

- `module_resolver.rs` in Rust kernel — source inspection + transport detection
- PyO3 binding: `resolve_module(path) → ModuleManifest`
- Napi-RS binding: `resolveModule(path) → ModuleManifest`
- `loader_dispatch.py` WASM/gRPC branches wired to Rust instead of `NotImplementedError`
- `load_module(coordinator, manifest)` convenience function dispatching to the correct loader

---

## 9. Deliverables

1. **`crates/amplifier-core/src/module_resolver.rs`** — Rust module with transport detection: `amplifier.toml` reader, `.wasm` scanner, WASM component metadata parser, Python package detector. Returns `ModuleManifest`.
2. **`ModuleManifest` + `ModuleArtifact` types** — the resolver's output struct
3. **`load_module(coordinator, manifest)` convenience function** — dispatches to correct `load_wasm_*` / `load_grpc_*`
4. **PyO3 binding:** `resolve_module(path)` + `load_module(coordinator, manifest)` exposed to Python
5. **Napi-RS binding:** `resolveModule(path)` + `loadModule(coordinator, manifest)` exposed to TypeScript
6. **`loader_dispatch.py` updated** — WASM and gRPC branches call through to Rust
7. **Tests covering all detection paths**

---

## 10. Testing Strategy

| Test | What it validates |
|---|---|
| `resolve_wasm_tool` | Directory with `echo-tool.wasm` → detects WASM transport + Tool type via component metadata |
| `resolve_wasm_provider` | Directory with `echo-provider.wasm` → detects Provider type |
| `resolve_python_package` | Directory with `__init__.py` → detects Python transport |
| `resolve_amplifier_toml_grpc` | Directory with `amplifier.toml` transport=grpc → detects gRPC + reads endpoint |
| `resolve_amplifier_toml_overrides_auto` | Directory with both `.wasm` and `amplifier.toml` → toml wins |
| `resolve_empty_dir_errors` | Empty directory → clear error message |
| `resolve_no_known_interface_errors` | `.wasm` that doesn't export Amplifier interface → error |
| `load_module_wasm_tool_e2e` | Full pipeline: resolve → load → execute echo-tool → verify roundtrip |
| `load_module_grpc_not_found` | gRPC endpoint that doesn't exist → clean error |
| Python integration: `test_loader_dispatch_wasm` | Python loader resolves path → calls Rust → mounts WASM tool on coordinator |
| Node integration: `test_resolve_and_load_wasm` | TS host resolves path → calls Rust → mounts WASM tool on coordinator |

Reuses Phase 3 fixtures: existing `tests/fixtures/wasm/*.wasm` files as test inputs. No new fixtures needed.

---

## 11. Not in Scope

- Auto-compilation of source code (Rust → WASM, Go → WASM)
- Module hot-reload
- Module marketplace / registry
- Changes to bundle YAML format
- Changes to foundation source URI resolution
- Go/C#/C++ native host bindings (Phase 5)
- Non-Rust WASM guest SDKs (Phase 5)
