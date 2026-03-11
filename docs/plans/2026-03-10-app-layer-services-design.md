# App-Layer Services & Review Fixes

> Wire `ApprovalProvider` and `DisplayService` as cross-language Rust traits on the Coordinator, fix WASM hook registration with `get-subscriptions`, and address remaining code review findings.

**Status:** Approved
**Date:** 2026-03-10
**Prerequisites:** Phase 3 (WASM loading), Phase 4 (module resolver), Session Init Polyglot Dispatch

---

## 1. Goal

Enable non-Python hosts to provide app-layer services (approval, display) through the Rust Coordinator while preserving 100% Python backward compatibility. Fix WASM hook registration and address code review findings from the polyglot dispatch work.

---

## 2. Background

amplifier-core has a Rust kernel with Python bindings (PyO3) and Node bindings (Napi-RS). Four app-layer services exist today but only as Python-side concerns: `ApprovalSystem`, `DisplaySystem`, `ModuleLoader`, and `ModuleSourceResolver`. The Rust Coordinator has no awareness of any of them — they're all `Py<PyAny>` blobs in the Python bindings.

`ApprovalProvider` already exists as a Rust trait in `traits.rs` with gRPC and WASM bridges, but is not wired to the Coordinator struct. `DisplayService` has no Rust representation at all.

Two independent reviewers confirmed: only `ApprovalProvider` and `DisplayService` belong as kernel traits. `ModuleLoader` and `SourceResolver` are foundation/app-layer concerns — inherently language-specific, involve file I/O, and violate kernel principles.

Additionally, WASM hook modules are silently dropped at mount time (no registration mechanism), engine errors are swallowed at debug level, `PyWasmOrchestrator` silently discards 5 of 6 parameters without documentation, and `_safe_exception_str` is duplicated across two files.

---

## 3. Architecture Decision: What Goes Where

The kernel defines traits for services the kernel **dispatches through** during its core coordination lifecycle:

| Service | Kernel dispatches through it? | Verdict |
|---------|-------------------------------|---------|
| `ApprovalProvider` | Yes — hook pipeline calls `request_approval()` when hook returns `ask_user` | **Kernel trait** |
| `DisplayService` | Yes — hook pipeline calls `show_message()` when hook returns `user_message` | **Kernel trait** |
| `ModuleLoader` | No — app layer calls it during init | **Foundation/app layer** |
| `SourceResolver` | No — `ModuleLoader` calls it | **Foundation/app layer** |

The load loop (`_session_init.py`) is pure policy — module ordering, error handling, multi-instance remapping are all decisions two teams could disagree on. It stays in foundation/app layer.

Per-language SDKs are premature — wait for ≥2 non-Python apps to prove the need.

---

## 4. Components

### 4.1 Wire `ApprovalProvider` to Coordinator

The `ApprovalProvider` Rust trait already exists in `traits.rs` with gRPC and WASM bridges. The Rust `Coordinator` struct has no field for it — only the Python `PyCoordinator` has `approval_system_obj: Py<PyAny>`.

Add to the Rust `Coordinator`, following the same pattern as `orchestrator` and `context` (single-slot, Option):

- Field: `approval_provider: Mutex<Option<Arc<dyn ApprovalProvider>>>`
- Accessor: `set_approval_provider(Arc<dyn ApprovalProvider>)`
- Accessor: `approval_provider() -> Option<Arc<dyn ApprovalProvider>>`

Add `PyApprovalProviderBridge` in the Python bindings — wraps the Python `ApprovalSystem` object and implements the Rust `ApprovalProvider` trait, following the exact pattern of `PyHookHandlerBridge`. When the Python app provides an `approval_system`, the PyO3 layer wraps it and sets it on the Rust Coordinator.

**Note on dual Python protocols:** Two Python approval protocols exist — `ApprovalSystem` (simple: prompt, options, timeout, default → string) in `approval.py` and `ApprovalProvider` (typed: ApprovalRequest → ApprovalResponse) in `interfaces.py`. The Rust trait matches `ApprovalProvider`. The PyO3 bridge wraps the simpler `ApprovalSystem` by adapting between the two interfaces.

**Python backward compat:** The `coordinator.approval_system` property still works — PyO3 bridge wraps it to the Rust trait.

### 4.2 Add `DisplayService` Trait

`DisplayService` exists only as a Python Protocol (`display.py`) with one method: `show_message(message, level, source)`. The Rust Coordinator has no awareness of it. It's consumed by `process_hook_result` when a hook returns `action: "user_message"`.

Add a `DisplayService` trait to `traits.rs`:

```rust
pub trait DisplayService: Send + Sync {
    fn show_message(
        &self,
        message: &str,
        level: &str,    // "info", "warning", "error"
        source: &str,   // e.g. "hook", "system"
    ) -> Pin<Box<dyn Future<Output = Result<(), AmplifierError>> + Send + '_>>;
}
```

Add to Coordinator:

- Field: `display_service: Mutex<Option<Arc<dyn DisplayService>>>`
- Accessor: `set_display_service(Arc<dyn DisplayService>)`
- Accessor: `display_service() -> Option<Arc<dyn DisplayService>>`

Add `PyDisplayServiceBridge` in the Python bindings — wraps the Python `DisplaySystem` object and implements the Rust trait. ~30 lines, simplest possible bridge. Display is fire-and-forget with a fallback to `logger.info()` if the service isn't set.

**What this enables:** A Rust or Node host can provide its own display implementation (WebSocketDisplay, StdoutDisplay, etc.) and hook results with `user_message` action reach it through the kernel's dispatch.

**Python backward compat:** The `coordinator.display_system` property still works — same bridge pattern.

### 4.3 Fix C1 — WASM Hook Registration via `get-subscriptions`

WASM hook modules loaded via `_make_wasm_mount` are silently dropped — the `PyWasmHook` wrapper is created but never registered with `coordinator.hooks`. There's no mechanism for a WASM hook to declare which events it handles.

**Part A: Add `get-subscriptions` to the WIT hook interface:**

```wit
interface hook-handler {
    handle: func(event: list<u8>) -> result<list<u8>, string>;
    get-subscriptions: func(config: list<u8>) -> list<event-subscription>;
}

record event-subscription {
    event: string,
    priority: s32,
    name: string,
}
```

The guest SDK (`amplifier-guest`) gets a corresponding Rust trait method:

```rust
pub trait HookHandler {
    fn handle(&self, event: &str, data: Value) -> Result<HookResult, String>;
    fn get_subscriptions(&self, config: Value) -> Vec<EventSubscription>;
}
```

**Part B: Host calls `get-subscriptions` at mount time:**

In `load_and_mount_wasm()` for hook modules, after loading the WASM binary:

1. Call `get-subscriptions(config)` on the guest
2. For each returned subscription, create a proxy `WasmHookBridge` handler and call `coordinator.hooks.register(event, handler, priority, name)`
3. Collect unregister functions, return cleanup

The `_make_wasm_mount` closure in `loader.py` then handles hooks correctly — `load_and_mount_wasm()` returns `status: "mounted"` with the registrations done.

**Same pattern for gRPC:** Add a `GetSubscriptions` RPC to the gRPC `HookService` so gRPC hooks can self-describe their subscriptions.

**Future bidirectional path (comment only):** If hooks need to read coordinator state during registration (e.g., conditionally subscribe based on mounted providers), a `register-hook` function can be added to the `kernel-service` host import interface, enabling imperative registration matching the Python `coordinator.hooks.register()` pattern. Document this in the WIT file and host implementation as a comment.

### 4.4 Fix I1 — Promote Engine Errors to Warning

In `loader.py`'s transport dispatch, `except Exception as engine_err` logs at `debug` level. A real `resolve_module()` failure (corrupt `amplifier.toml`, wrong permissions, Rust engine bug) is silently swallowed and the module falls through to the Python loader with a misleading error.

Change from `logger.debug(...)` to `logger.warning(...)`:

```python
except Exception as engine_err:
    logger.warning(
        f"resolve_module failed for '{module_id}': {engine_err}, "
        "falling through to Python loader"
    )
```

One line change. The `ImportError` path stays at `debug` — Rust engine not installed is a valid defensive pattern even though wheels always include it.

### 4.5 Fix I3 — `PyWasmOrchestrator` Documentation

`PyWasmOrchestrator.execute()` accepts the full Python Orchestrator Protocol (6 parameters: `prompt`, `context`, `providers`, `tools`, `hooks`, `coordinator`) then silently discards 5 of them with `let _ = (context, providers, tools, hooks, coordinator)`.

The current signature is **correct**. `_session_exec.run_orchestrator()` always passes all 6 kwargs to whatever orchestrator is mounted — there is one unified dispatch path. Changing the signature would cause a `TypeError` at runtime.

The fix is documentation, not code change:

1. **Add a `log::warn!` in `load_and_mount_wasm()`** when mounting a WASM orchestrator noting that context/providers/tools/hooks/coordinator are not forwarded to WASM guests in this version, and that the WASM guest accesses kernel services via host imports instead.
2. **Improve the doc comment on `PyWasmOrchestrator.execute()`** explaining why the params are accepted and discarded — protocol conformance with the WASM guest using host imports for kernel access.
3. **Add a comment pointing to the future path:** forwarding context/providers/tools to the WASM guest via kernel-service host imports, so WASM orchestrators that need session state can pull it on demand rather than receiving it as parameters.

`NullContextManager` stays — the Rust `Orchestrator` trait requires a `context` parameter. When WASM orchestrators gain real context forwarding, it gets replaced with the actual context.

### 4.6 Fix I4 — Deduplicate `_safe_exception_str`

`_safe_exception_str` is defined identically in both `session.py` and `_session_init.py`. Since `session.py` now delegates to `_session_init.py`, the copy in `session.py` is redundant.

Delete `_safe_exception_str` from `session.py`. If it's still called there, import from `_session_init`. If it's not used after deduplication, just delete it.

---

## 5. Python Backward Compatibility

- `coordinator.approval_system` property still works — PyO3 bridge wraps it to the Rust trait
- `coordinator.display_system` property still works — same bridge pattern
- `_session_init.py` calling `loader.load()` is unchanged
- `session.py:initialize()` delegates to `_session_init.initialize_session()` (already done)
- WASM hook modules now properly register instead of being silently dropped — bug fix, not behavior change

---

## 6. Rejected Alternatives

1. **ModuleLoader as kernel trait** — Module loading is inherently language-specific (Python uses importlib, Rust uses dylibs, Node uses require()). No meaningful cross-language abstraction. Would create an FFI trampoline (Rust → GIL → Python importlib → GIL → Rust coordinator) that adds indirection without value. Violates "no file I/O in kernel."

2. **SourceResolver as kernel trait with Coordinator field** — Source resolution involves network I/O (git clone), caching, authentication — all policy. The kernel never calls it. Trait definition in `traits.rs` is acceptable for bridge generation, but not as a Coordinator field.

3. **Session::initialize() in Rust kernel** — The load loop is pure policy: module ordering, error handling, multi-instance remapping, fork event emission. Two teams could disagree on every decision. The kernel provides mount primitives; foundation builds the loading machinery on top.

4. **Per-language Foundation SDKs** — Premature. Wait for ≥2 non-Python apps to prove the need. When they arrive, standalone `amplifier-sdk-*` repos.

5. **Bidirectional WASM hook registration** — Would let WASM guests call `coordinator.hooks.register()` via kernel-service host imports. More powerful but significantly more complex (requires hybrid WIT world importing kernel-service + exporting hook-handler). The self-describing `get-subscriptions` approach covers 95%+ of real hook use cases. Bidirectional can be added later without breaking changes.

6. **Changing `PyWasmOrchestrator.execute()` to accept only `prompt`** — Would break at runtime. `_session_exec.run_orchestrator()` always passes all 6 kwargs via a unified dispatch path. The current full-signature approach is correct protocol conformance.

---

## 7. Open Questions / Future Work

1. **Consolidate Python approval protocols** — `ApprovalSystem` (simple) and `ApprovalProvider` (typed) are two competing interfaces. Consider converging them.
2. **WASM hook cleanup** — Do WASM hooks need cleanup functions? The `get-subscriptions` approach returns unregister closures from the host — cleanup is host-managed. But if the WASM guest holds resources, it may need a `cleanup` export in the WIT.
3. **Bidirectional WASM registration** — Add `register-hook` to kernel-service when a real use case requires reading coordinator state during hook registration.
4. **`SourceResolver` trait in `traits.rs`** — Acceptable for gRPC/WASM bridge generation even though the kernel doesn't dispatch through it. Can be added when a gRPC source resolver is needed.