# Python Session Init Polyglot Dispatch

> Make WASM and gRPC modules loadable from bundle config in the Python host, so bundle authors can declare polyglot modules alongside Python ones and session init auto-detects and loads them.

**Status:** Approved
**Date:** 2026-03-09
**Prerequisites:** Unified Module Storage & Arc Sharing Fix, Phase 3 (WASM loading), Phase 4 (module resolver)

---

## 1. Goal

Make WASM and gRPC modules loadable from bundle config in the Python host, so bundle authors can declare polyglot modules alongside Python ones and session init auto-detects and loads them. Also provide an explicit Python API for loading specific WASM modules into a running session (already works via `load_and_mount_wasm(coordinator, path)`).

---

## 2. Background

The Python host has all the pieces for polyglot module loading but they aren't wired together:

- `_session_init.py` loads all modules via `loader.load()` at 5 call sites (orchestrator, context, providers, tools, hooks) — Python-only today
- `loader_dispatch.py` exists as a complete polyglot router but is orphaned — nothing calls it in production
- Rust `resolve_module()` (transport detection) and `load_and_mount_wasm()` (WASM loading + coordinator mounting) are already exposed to Python via PyO3
- `session.py:AmplifierSession.initialize()` and `_session_init.py:initialize_session()` contain ~200 lines of near-identical module loading logic

The previous attempt to wire `loader_dispatch.py` into `_session_init.py` was reverted (PR #39) due to bugs: dict `source_hint` TypeError crash, SESSION_FORK events silently dropped, untested Rust FFI on critical path. This design takes a fundamentally different approach.

`amplifier-core` always ships as compiled wheels with Rust extensions — no pure-Python install path exists.

---

## 3. Architecture: Absorb Dispatch Into the Loader

The key architectural insight: `loader_dispatch.py` was at the WRONG abstraction boundary. It sat between two interfaces that don't agree on types (`source_hint` opaque URI vs `source_path` resolved filesystem path). The right integration point is INSIDE `loader.py` at the exact moment where a `source_hint` has already been resolved to a filesystem path, but before Python importlib loading.

```
┌─────────────────────────────────────────────────────────┐
│  _session_init.py / session.py                          │  ← Transport-unaware
│  loader.load(module_id, config, source_hint, coordinator)│    (unchanged API + coordinator param)
└──────────────────────────┬──────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│  loader.py:ModuleLoader._resolve_source()               │  ← URI → path (Python policy)
│  ModuleSourceResolver.resolve(source_hint)              │     via mountable resolver module
└──────────────────────────┬──────────────────────────────┘
                           │ filesystem path
                           ▼
┌─────────────────────────────────────────────────────────┐
│  Rust resolve_module(path)  [via PyO3]                  │  ← path → transport (Rust mechanism)
│  Returns: {transport, module_type, artifact}            │     single source of truth
└──────────┬───────────┬──────────┬───────────────────────┘
           │           │          │
       python        wasm       grpc
           │           │          │
           ▼           ▼          ▼
┌──────────────┐ ┌──────────┐ ┌──────────────┐
│ _load_entry  │ │ load_and │ │ load_grpc    │
│ _point() /   │ │ _mount   │ │ _module()    │
│ _filesystem()│ │ _wasm()  │ │              │
│  [Python]    │ │ [Rust]   │ │ [Python+Rust]│
└──────────────┘ └──────────┘ └──────────────┘
```

Transport is invisible to `_session_init.py` — it calls `loader.load()` the same way for all module types. The loader handles dispatch internally, after source resolution.

---

## 4. Components

### 4.1 Deduplicate Session Init

`session.py:AmplifierSession.initialize()` delegates to `_session_init.initialize_session()`. Eliminates ~200 lines of duplicated loading logic. The pure-Python `AmplifierSession` and the Rust `PySession` both call the same function.

```python
# session.py:AmplifierSession.initialize()
async def initialize(self) -> None:
    if self._initialized:
        return
    from ._session_init import initialize_session
    await initialize_session(
        self.config, self.coordinator, self.session_id, self.parent_id
    )
    self._initialized = True
```

**What gets deleted:** ~200 lines of duplicated loading logic in `session.py` (config parsing, load loops for all 5 module types, multi-instance provider remapping, SESSION_FORK emission, `_safe_exception_str` helper).

**What stays:** `_session_init.initialize_session()` becomes the single implementation. Its signature and behavior are unchanged.

### 4.2 Add Transport Dispatch Inside `loader.py`

After `ModuleSourceResolver` resolves a `source_hint` to a filesystem path, but before the Python importlib loading, call Rust `resolve_module()` to detect transport and branch.

The logic (~15 lines inside `loader.py:ModuleLoader.load()`):

```python
# After: module_path = await resolver.resolve(...)
# Before: existing _load_entry_point / _load_filesystem

from amplifier_core._engine import resolve_module

manifest = resolve_module(str(module_path))
transport = manifest.get("transport", "python")

if transport == "wasm":
    return self._make_wasm_mount(module_path, coordinator)
elif transport == "grpc":
    return self._make_grpc_mount(module_path, config, coordinator)
# else: fall through to existing Python loading (unchanged)
```

**Key design decisions:**

- **Rust `resolve_module()` is the single source of truth** for transport detection — no Python reimplementation
- **No `try/except ImportError` fallback** — Rust extensions always ship in wheels, there is no pure-Python install path
- **The existing Python loading path is the `else` branch** — zero changes to how Python modules load today
- **`loader.load()` gains `coordinator=None`** — backward compatible, existing callers that don't pass it work unchanged
- **`_session_init.py` passes `coordinator=coordinator`** at its 5 call sites — this is the only change to session init, which stays transport-unaware

### 4.3 Delete `loader_dispatch.py`

`loader_dispatch.py` (131 lines) gets deleted. It has three fundamental problems that can't be fixed incrementally:

1. **Interface mismatch** — expects `source_path` (resolved filesystem path) but callers have `source_hint` (opaque URI). This caused the dict `source_hint` TypeError crash in PR #39.
2. **Duplicates Rust logic** — Python `_detect_transport()` and `_read_module_meta()` reimplement what Rust `resolve_module()` already does (with WASM introspection, security checks, SHA-256 verification).
3. **Violates CORE_DEVELOPMENT_PRINCIPLES §5** — "Don't duplicate logic across languages."

Its transport routing logic moves into `_make_wasm_mount` and `_make_grpc_mount` helpers on `ModuleLoader`. Its tests (`test_loader_dispatch_wasm.py`) get refactored to test the new dispatch path inside `loader.load()` — same test logic, different entry point.

`loader_grpc.py` stays — it contains the actual `GrpcToolBridge` implementation that speaks proto.

### 4.4 Handle All 6 WASM Module Types

`load_and_mount_wasm()` in the Rust PyO3 bindings currently auto-mounts tools only (wraps in `PyWasmTool`, puts into `mount_points["tools"]`). For all other module types, it returns `status: "loaded"` without mounting.

Extend `load_and_mount_wasm()` to auto-mount all 6 module types. New `PyWasm*` wrappers, each implementing the corresponding Python Protocol:

| Module Type | Rust Bridge | Python Wrapper | Mount Target |
|-------------|-------------|----------------|-------------|
| Tool | `Arc<dyn Tool>` | `PyWasmTool` (exists) | `mount_points["tools"]` |
| Hook | `Arc<dyn HookHandler>` | `PyWasmHook` (new) | `coordinator.hooks.register()` |
| Provider | `Arc<dyn Provider>` | `PyWasmProvider` (new) | `mount_points["providers"]` |
| Context | `Arc<dyn ContextManager>` | `PyWasmContext` (new) | `mount_points["context"]` |
| Orchestrator | `Arc<dyn Orchestrator>` | `PyWasmOrchestrator` (new) | `mount_points["orchestrator"]` |
| Approval | `Arc<dyn ApprovalProvider>` | `PyWasmApproval` (new) | Not stored in coordinator (Python-side concern) |

Each `PyWasm*` wrapper follows the same pattern as `PyWasmTool`: holds the `Arc<dyn Trait>`, exposes the Python Protocol methods (sync or async via `pyo3-async-runtimes`), and mounts into the coordinator's `mount_points` dict.

The `_make_wasm_mount` helper in `loader.py` then just calls `load_and_mount_wasm(coordinator, path)` and returns a cleanup function — Rust handles all the wrapping and mounting.

### 4.5 Documentation Strategy — Docstrings as Source of Truth

Same principle as the unified module storage design — no API usage examples in design docs that rot.

**What lives in code:**
- `/// # Examples` doc-tests on new Rust `PyWasm*` types
- Python docstrings on `loader.load()`'s new `coordinator` parameter
- Python docstrings on `_make_wasm_mount` and `_make_grpc_mount` helpers

**What this design doc covers:**
- Why `loader_dispatch.py` was deleted (wrong layer, duplicated Rust logic)
- The transport dispatch architecture (Rust `resolve_module()` as single source of truth)
- The `session.py` → `_session_init.py` deduplication decision
- The 6 `PyWasm*` wrapper types and their Python Protocol conformance

---

## 5. Python Backward Compatibility

- `loader.load()` gains `coordinator=None` — existing callers that don't pass it work unchanged
- `_session_init.py` continues to call `loader.load()` — just passes `coordinator` as a new keyword arg
- The Python loading path is the default `else` branch — zero behavior changes for Python modules
- `session.py:initialize()` delegates to the same `_session_init.initialize_session()` it was already near-duplicating

---

## 6. Rejected Alternative

**Wiring `loader_dispatch.py` into `_session_init.py`** — This was the previous approach (reverted from PR #39). Rejected because:

1. **Wrong abstraction boundary** — `_session_init` works with `source_hint` (opaque URI), `loader_dispatch` expects `source_path` (resolved filesystem path). Interface mismatch caused the dict `source_hint` TypeError crash.
2. **Transport leaks into session init** — violates CORE_DEVELOPMENT_PRINCIPLES §8: "Transport is invisible to developers."
3. **Duplicates Rust logic in Python** — `_detect_transport()` and `_read_module_meta()` reimplement `resolve_module()`.
4. **Two integration surfaces** — both `session.py` and `_session_init.py` would need wiring (vs. one change inside `loader.py`).

---

## 7. Open Questions / Future Work

1. **Non-tool WASM cleanup functions** — Do WASM hooks/providers/context/orchestrators need cleanup? `PyWasmTool` returns no cleanup fn. If WASM modules hold resources (gRPC connections, file handles), cleanup may be needed.
2. **WASM module hot-reload** — Future TODO #6. Not part of this design.
3. **gRPC adapter for Python-from-Rust-host** — Edge-layer project documented in the unified module storage design. Not part of this design.
