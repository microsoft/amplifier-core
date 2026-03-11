# Unified Module Storage & Arc Sharing Fix

> Make Rust the single source of truth for module storage across all non-Python transports, fix Arc sharing for binding layers, and establish the Python-from-Rust-host pattern.

**Status:** Approved
**Date:** 2026-03-09
**Prerequisites:** PR #39 (medium-priority fixes), Phase 2 (Napi-RS bindings), Phase 3 (WASM loading), Phase 4 (module resolver)

---

## 1. Goal

Make Rust the single source of truth for module storage across all non-Python transports, fix the Arc sharing problem for binding layers, and establish the architectural pattern for loading Python modules from non-Python hosts — all while preserving 100% backward compatibility for the existing Python ecosystem.

---

## 2. Background

The Rust `Coordinator` stores modules in typed fields (`tools: Mutex<HashMap<String, Arc<dyn Tool>>>`, etc.) while the Python bindings maintain a parallel `mount_points: Py<PyDict>` for Python module dispatch. Two problems block non-Python hosts from using the kernel effectively:

1. **Arc sharing is broken.** `HookRegistry` is owned by value inside `Coordinator`, so binding layers (Node, Go, etc.) can't obtain shared ownership. The Node bindings work around this by creating disconnected copies on each getter call — a fundamentally broken pattern.

2. **gRPC transport is incomplete.** Only `load_grpc_tool()` and `load_grpc_orchestrator()` exist. The other four module types (provider, hook, context, approval) have no gRPC loading path, blocking polyglot module bundles.

3. **Module resolver is locked behind `wasm` feature.** A Rust host wanting only gRPC + native modules must pull in the entire wasmtime dependency chain just to access `resolve_module()`.

4. **No documented pattern for Python-from-Rust.** The gRPC bridges already solve this, but no architectural guidance exists for non-Python hosts encountering Python modules.

---

## 3. Backward Compatibility Constraint

**Python backward compat is sacred and non-negotiable.** The following contract is preserved unchanged:

- `coordinator.mount_points` — mutable dict with 6 keys, has a setter for wholesale replacement
- `coordinator.mount_points["tools"]["name"] = obj` — direct dict mutation
- `coordinator.get("providers")` / `coordinator.get("tools")` — returns typed dicts
- `coordinator.mount(point, module, name=)` / `coordinator.unmount(point, name=)` — async mount/unmount
- `coordinator.hooks` property, `coordinator.hooks.register(event, handler)`
- All community module patterns (anthropic provider, shell-hook, approval hooks)

**No backward compat needed for non-Python bindings** — nobody is using Node, Go, C#, or C++ bindings yet. Retcon freely to the correct final shape.

---

## 4. Architecture: Two Clean Storage Paths

The design explicitly maintains two independent, non-overlapping storage paths:

```
┌─────────────────────────────────────────────────────────┐
│  Rust Typed Storage                                     │
│  HashMap<String, Arc<dyn Tool>>                         │
│  HashMap<String, Arc<dyn Provider>>                     │
│  ...                                                    │
│  Serves: Rust-native, WASM, gRPC, future Go/C#/C++     │
├─────────────────────────────────────────────────────────┤
│  Python mount_points Dict                               │
│  PyDict with 6 keys, dict protocol semantics            │
│  Serves: existing Python ecosystem (unchanged)          │
└─────────────────────────────────────────────────────────┘
```

No module is mounted in both simultaneously in production. The Coordinator is transport-agnostic — `Arc<dyn Tool>` is `Arc<dyn Tool>` whether the module is native Rust, WASM, or gRPC.

**Why not unify?** The Python `mount_points` dict is a deeply entrenched de facto public API with dict protocol semantics, direct mutation, identity guarantees, and wholesale replacement. Migrating it to Rust would break the Python ecosystem for no runtime benefit, and the "bridge sandwich" (Python→Rust→Python) for the Orchestrator trait is a showstopper.

---

## 5. Components

### 5.1 `Arc<HookRegistry>` in Coordinator

**Change:** In `coordinator.rs`, change `hooks: HookRegistry` to `hooks: Arc<HookRegistry>`.

**What changes:**
- Constructor wraps in `Arc::new(HookRegistry::new())` — 1 line
- New accessor: `hooks_shared(&self) -> Arc<HookRegistry>` — clones the Arc for shared ownership

**What doesn't change:**
- Existing `hooks(&self) -> &HookRegistry` accessor works unchanged via `Arc::Deref`
- All ~16 existing call sites use `&HookRegistry` — zero source changes
- Python bindings unaffected (they create their own HookRegistry)
- HookRegistry internals already use `Arc<Mutex<HashMap>>` — outer Arc is consistent

### 5.2 Fix Node Bindings

Delete factory methods, replace with getters that share the real instances.

**The fix:**
- `JsAmplifierSession.coordinator` — getter returning `JsCoordinator` wrapping the Session's real `Arc<Coordinator>` via `coordinator_shared()`
- `JsCoordinator.hooks` — getter returning `JsHookRegistry` wrapping the real `Arc<HookRegistry>` via `hooks_shared()`
- Delete `create_coordinator()` and `create_hook_registry()` factory methods
- Delete all "Future TODO #1" workaround comments and warning log messages
- Delete cached-config reconstruction logic

**Pattern for all future bindings** (Go, C#, etc.): getters that share the real Arc, never factory methods that create disconnected copies.

### 5.3 Complete gRPC Transport Symmetry

**The gap:** `transport.rs` has `load_grpc_tool()` and `load_grpc_orchestrator()` but is missing four module types.

**Add:**
- `pub async fn load_grpc_provider(endpoint: &str) -> Result<Arc<dyn Provider>>`
- `pub async fn load_grpc_hook(endpoint: &str) -> Result<Arc<dyn HookHandler>>`
- `pub async fn load_grpc_context(endpoint: &str) -> Result<Arc<dyn ContextManager>>`
- `pub async fn load_grpc_approval(endpoint: &str) -> Result<Arc<dyn ApprovalProvider>>`

Each is ~3-5 lines delegating to the corresponding `GrpcXxxBridge::connect()`. Completes the transport surface so any host language can load any module type over gRPC.

### 5.4 Decouple `LoadedModule` from `wasm` Feature Gate

**Split into feature-gated tiers:**

**Always available (no feature gate):**
- `resolve_module()` — detects transport type from path
- `ModuleManifest`, `ModuleArtifact`, `Transport` types
- `LoadedModule` variants for all module types
- gRPC and native loading paths

**Behind `#[cfg(feature = "wasm")]` only:**
- WASM component metadata parsing
- `load_wasm_*` functions
- wasmtime `Engine` parameter on `load_module()`
- WASM-specific detection in `resolve_module()` (`.wasm` file scanning)

**Result:** `cargo add amplifier-core` (no features) gives access to `resolve_module()` → `load_grpc_provider()` for polyglot loading. Add `features = ["wasm"]` only when WASM module loading is needed.

### 5.5 Documentation Strategy — Docstrings Are the Source of Truth

**Principle:** No API usage examples in design docs or prose markdown. Per Context Poisoning prevention principles, each concept is documented in exactly ONE place.

For API usage, that place is **Rust `/// # Examples` doc-test blocks**:
- Compiled and tested by `cargo test` — drift caught as compile failures
- Surfaced by LSP hover via rust-analyzer
- Surfaced by `cargo doc` for browsable HTML
- Single source of truth — no separate markdown to keep in sync

**This design doc covers:** Architectural decisions and rationale only.

**Implementation tasks will include:** Adding/updating doc-tests on `hooks_shared()`, the 4 new transport functions, `resolve_module()`, and `LoadedModule` dispatch patterns.

**For binding layers** (Node, future Go/C#): Each binding's README gets a single quick-start example, but authoritative API docs are generated from the binding code itself (TypeScript `.d.ts` types, Go godoc, etc.).

---

## 6. Python-from-Rust-Host Pattern

When a non-Python host encounters a Python module:

```
resolve_module(path)          Host Policy               Rust Kernel
  → Transport::Python    →    spawn gRPC adapter    →   load_grpc_provider()
    + package name             (host decides how)        → Arc<dyn Provider>
```

**The dispatch rule:** The resolver returns `Transport::Python` with the package name. It does NOT spawn processes or manage adapters. The resolver detects; the host decides.

**The adapter contract:** A future `amplifier-grpc-adapter` Python package (~200-400 lines) wraps any Python module as a gRPC service using the existing proto contracts. Not part of this design — documented as the intended edge-layer pattern.

**Why gRPC, not embedded Python (PyO3):**
- Full Python isolation (own process, own GIL)
- All 6 gRPC bridges already exist and work
- No GIL contention across modules
- No interpreter lifecycle management in the kernel (violates "mechanism not policy")
- Works for ANY host language, not just Rust

**Why NOT the kernel's responsibility:** Process spawning is policy. The kernel provides gRPC bridge mechanism; the host decides when/how to spawn adapters. Different deployments could use different strategies (containerized, Lambda, sidecar).

---

## 7. Universal API Shape

After this design, every language follows the same pattern:

1. Create `AmplifierSession` from config
2. Get `coordinator` (shared via Arc, not copied)
3. Mount modules via typed methods
4. Get `hooks` from coordinator (shared via Arc, not copied)
5. Call `execute()`

For polyglot bundles, the host dispatches on `Transport` from the module resolver:

| Transport | Action |
|-----------|--------|
| `Native` | Direct `Arc<dyn Trait>` |
| `Wasm` | `load_wasm_*()` functions |
| `Grpc` | `load_grpc_*()` functions |
| `Python` (non-Python host) | Spawn gRPC adapter, then `load_grpc_*()` |
| `Python` (Python host) | Existing Python import path, unchanged |

---

## 8. Rejected Alternatives

1. **Full Rust storage unification** — Migrate Python `mount_points` to Rust. Rejected: `mount_points` is a deeply entrenched de facto public API. The bridge sandwich (Python→Rust→Python) for the Orchestrator trait is a showstopper, and the backward compat risk is critical.

2. **Embedded Python from Rust host (PyO3)** — Rejected: puts interpreter lifecycle management (policy) in the kernel, creates GIL contention, requires 5 new complex bridge types when gRPC already works.

3. **Python modules via WASM compilation** — Immediately disqualified. Python community modules use C extensions (httpx, aiohttp), asyncio, and filesystem access — none viable in WASM.

4. **`Arc<dyn ModuleRegistry>` trait abstraction** — YAGNI. Typed `HashMap` fields on Coordinator are simple, correct, and sufficient.

---

## 9. Open Questions / Future Work

1. **Python gRPC adapter** — `amplifier-grpc-adapter` Python package (~200-400 lines). Edge-layer project for a future sprint.

2. **Unifying Python and Rust HookRegistries** — Currently Python creates its own HookRegistry independent of the Coordinator's. Could be unified so hooks registered from Rust are visible to Python and vice versa. Separate decision.

3. **`process_hook_result` stays in Python** — Every branch routes to Python subsystems (context manager, approval system, display system). If Rust consumers need hook result processing, build a parallel Rust implementation.

4. **Go/C#/C++ native bindings** — The Arc sharing fix and gRPC symmetry completion prepare the architecture. Binding design is future work.
