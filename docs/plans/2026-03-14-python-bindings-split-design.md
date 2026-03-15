# Python Bindings Split + Thin Binding Layer Design

## Goal

Split the monolithic `bindings/python/src/lib.rs` (4,129 lines) into ~14 focused modules, decompose the oversized `PyCoordinator` (~900 lines) into a subdirectory with 4 sub-modules, and push `process_hook_result()` routing logic from Python into the Rust binding layer — eliminating `_rust_wrappers.py` entirely and making the Python binding as thin as possible while preserving full backward compatibility.

## Background

The Python binding's `lib.rs` has grown to 4,129 lines — a single file containing 13 PyO3 classes, 5 exported functions, 41 event constants, and all associated bridge implementations. The Node binding was already split into focused modules in PR #46, establishing the pattern. Meanwhile, the Python side carries a ~247-line Python wrapper layer (`_rust_wrappers.py`) that subclasses the Rust-backed `ModuleCoordinator` to add `process_hook_result()` routing, cleanup logic, and token budget tracking. This wrapper exists because the Rust binding layer lacked a `ContextManagerBridge` — the only bridge not yet implemented.

Exploration of the downstream consumers (amplifier-foundation, amplifier-app-cli) confirmed that `_rust_wrappers.py` and `process_hook_result()` are fully internal — zero external references. The external contract surface (`AmplifierSession`, `ModuleCoordinator`, `HookResult` import paths, all submodule paths) is well-defined and must be preserved exactly.

## Approach

**Chosen: Approach A — "Thinner Binding, Smarter Binding Layer"**

Move `process_hook_result` routing + token budgeting into the Rust binding layer (`PyCoordinator`), not the kernel. Add `PyContextManagerBridge` as a new trait bridge alongside the existing `PyApprovalProviderBridge` and `PyDisplayServiceBridge`. Delete `_rust_wrappers.py`. The kernel stays pure mechanism. Each language binding can adopt the pattern independently.

**Rejected alternatives:**

- **Approach B (Push into kernel):** Would violate "mechanism not policy" — the kernel would encode the specific dispatch table. Bigger blast radius on kernel backward-compat.
- **Approach C (Shared binding helper crate):** Over-engineering for ~150 lines of dispatch logic. The two bindings have very different session models and would diverge quickly.

## Architecture

### File Split Layout

Following the Node split pattern (PR #46), `lib.rs` becomes a pure router with `mod` declarations, `pub use` re-exports, and the `#[pymodule]` registration function. Each logical domain gets its own file.

Proposed file layout for `bindings/python/src/`:

| File | ~Lines | Contents |
|------|--------|----------|
| `lib.rs` | ~230 | `mod` declarations + `pub use` re-exports + `#[pymodule] fn _engine(...)` with all 13 class registrations, 5 function registrations, 41 event constants |
| `helpers.rs` | ~60 | `wrap_future_as_coroutine()` + `try_model_dump()` — shared utilities used by every other module |
| `bridges.rs` | ~400 | `PyHookHandlerBridge` + `PyApprovalProviderBridge` + `PyDisplayServiceBridge` + new `PyContextManagerBridge` |
| `session.rs` | ~645 | `PySession` |
| `hooks.rs` | ~270 | `PyUnregisterFn` + `PyHookRegistry` |
| `cancellation.rs` | ~180 | `PyCancellationToken` |
| `coordinator/mod.rs` | ~250 | `PyCoordinator` struct def + `#[new]`, lifecycle methods, `to_dict()` |
| `coordinator/mount_points.rs` | ~300 | All `.mount()`, `.get()`, `.get_tool()`, `.get_provider()`, etc. |
| `coordinator/capabilities.rs` | ~200 | `.register_capability()`, `.get_capability()`, `.capabilities`, contribution channels |
| `coordinator/hook_dispatch.rs` | ~250 | `process_hook_result()` — routing logic moved from Python, plus token budget tracking |
| `errors.rs` | ~260 | `PyProviderError` |
| `retry.rs` | ~200 | `PyRetryConfig` + `classify_error_message` + `compute_delay` |
| `module_resolver.rs` | ~90 | `resolve_module` + `load_wasm_from_path` |
| `wasm.rs` | ~880 | All 6 `PyWasm*` structs + `load_and_mount_wasm` |

Key decisions:

- `lib.rs` stays larger than Node's (230 vs 45 lines) because PyO3 requires the explicit `#[pymodule]` registration + 41 event constants. This is unavoidable.
- `helpers.rs` is separate from `bridges.rs` because helpers are pure utilities while bridges implement Rust traits.
- The coordinator gets a subdirectory (`coordinator/mod.rs` + 3 sub-modules) rather than a single file, addressing the 900-line problem.
- `hook_dispatch.rs` inside the coordinator is where `process_hook_result()` lands — this is the Tier 3 payload.

### Dependency Graph

No circular dependencies — the graph is acyclic:

```
helpers <- (everything)
bridges <- hooks, coordinator/hook_dispatch
cancellation <- coordinator/mod
hooks <- session
coordinator <- wasm, session
errors, retry, module_resolver, wasm <- (leaf modules, no dependents)
```

## Components

### Coordinator Decomposition

The coordinator is the largest PyO3 class (~900 lines). The subdirectory breaks down as follows:

**`coordinator/mod.rs`** (~250 lines) — The struct definition and lifecycle:

- `#[pyclass(subclass)]` struct with all fields (inner `Arc<Coordinator>`, `py_cancellation`, `mount_points`, `session_state`, etc.)
- `#[new]` constructor
- `_set_session()`, `session_id`, `parent_id`, `config`
- `cleanup()` — the async teardown that runs cleanup functions with Python's `BaseException`/`Exception` fatal-propagation logic (moved from `_rust_wrappers.py`)
- `to_dict()`, `__repr__`

**`coordinator/mount_points.rs`** (~300 lines) — Module storage and retrieval:

- `mount()`, `unmount()`, `get()`
- `get_tool()`, `get_provider()`, `get_context()`, `get_orchestrator()`, `get_approval()`
- `mount_tool()` convenience method
- `reset_turn()`, `start_tool_tracking()`, `finish_tool_tracking()`, `track_injection()`
- `loader` property

**`coordinator/capabilities.rs`** (~200 lines) — The extensibility surface:

- `register_capability()`, `get_capability()`, `capabilities` property
- `add_cleanup_fn()`
- `register_contribution_channel()`, `contribute()`, `collect_contributions()`

**`coordinator/hook_dispatch.rs`** (~250 lines) — The Tier 3 payload:

- `process_hook_result()` — moved from Python `_rust_wrappers.py`, routes `HookResult` actions:
  - `inject_context` → calls `PyContextManagerBridge` (new, in `bridges.rs`)
  - `ask_user` → calls `PyApprovalProviderBridge` (existing)
  - `user_message` → calls `PyDisplayServiceBridge` (existing)
- Token budget tracking (`injection_budget_per_turn`, `injection_size_limit`, `_current_turn_injections`)
- Size validation for context injections

**Visibility:** `mod.rs` defines the struct with `pub(crate)` fields. The three sub-modules use `impl PyCoordinator` blocks that access those fields. All sub-modules are `pub(crate)` — the only public export is `PyCoordinator` itself via `lib.rs`.

### ContextManagerBridge

New `PyContextManagerBridge` (in `bridges.rs`, ~80 lines):

- Mirrors the existing `PyApprovalProviderBridge` and `PyDisplayServiceBridge` pattern
- Holds a `Py<PyAny>` reference to the Python context manager object
- Adapts its `add_message()` method for use from Rust
- Acquires GIL, calls `py_context.call_method("add_message", (message,))`
- Wraps result with `wrap_future_as_coroutine` if async

### process_hook_result in Rust

`process_hook_result()` in `coordinator/hook_dispatch.rs` (~250 lines):

- Routing logic moved from `_rust_wrappers.py` as a `#[pymethods]` async method on `PyCoordinator`
- Receives `HookResult` (deserialized from Python dict via serde)
- Matches on `result.action`:
  - `"inject_context"` → validate size limits, check token budget, call `PyContextManagerBridge.add_message()` with timestamp + provenance metadata
  - `"ask_user"` → call `PyApprovalProviderBridge.request_approval()` (bridge already exists)
  - `"user_message"` → call `PyDisplayServiceBridge.show_message()` (bridge already exists)
  - `"continue"` / `None` → no-op
- Token budget tracking: `injection_budget_per_turn` and `injection_size_limit` become fields on `PyCoordinator`, with `reset_turn()` clearing the per-turn accumulator

### Cross-Module Visibility Map

Types that need `pub(crate)` visibility (cross-module references):

| Type | Defined In | Referenced From | Why |
|------|-----------|----------------|-----|
| `PyHookRegistry` | `hooks.rs` | `session.rs` | `PySession::execute()` does `extract::<PyRef<PyHookRegistry>>()` |
| `PyCancellationToken` | `cancellation.rs` | `coordinator/mod.rs` | `PyCoordinator` holds `Py<PyCancellationToken>` field |
| `PyCoordinator` | `coordinator/mod.rs` | `wasm.rs` | `load_and_mount_wasm` takes `&PyCoordinator` parameter |
| `PyHookHandlerBridge` | `bridges.rs` | `hooks.rs` | `PyHookRegistry::register()` creates `Arc<PyHookHandlerBridge>` |
| `PyContextManagerBridge` | `bridges.rs` | `coordinator/hook_dispatch.rs` | `process_hook_result()` calls context bridge |
| `PyApprovalProviderBridge` | `bridges.rs` | `coordinator/hook_dispatch.rs` | `process_hook_result()` calls approval bridge |
| `PyDisplayServiceBridge` | `bridges.rs` | `coordinator/hook_dispatch.rs` | `process_hook_result()` calls display bridge |
| `wrap_future_as_coroutine` | `helpers.rs` | 8+ methods across all modules | Shared async helper |
| `try_model_dump` | `helpers.rs` | `bridges.rs`, `wasm.rs` | Pydantic dict conversion |

Visibility rule: Everything is `pub(crate)` by default. Only `lib.rs` decides what's truly public (via `pub use` re-exports into the `#[pymodule]`). This matches the Node split pattern exactly.

Import pattern in each module:

```rust
use crate::helpers::wrap_future_as_coroutine;
use crate::bridges::PyHookHandlerBridge;
// etc.
```

## Data Flow

### Hook Result Dispatch (after refactor)

```
Python module hook fires
  → Rust kernel HookRegistry dispatches via PyHookHandlerBridge
    → Returns HookResult to PyCoordinator.process_hook_result() [now in Rust]
      → Match on action:
        ├── "inject_context" → validate budget → PyContextManagerBridge.add_message() → Python ContextManager
        ├── "ask_user" → PyApprovalProviderBridge.request_approval() → Python ApprovalProvider
        ├── "user_message" → PyDisplayServiceBridge.show_message() → Python DisplayService
        └── "continue" / None → no-op
```

### Python-side Changes

What gets deleted:

- `_rust_wrappers.py` — entirely (~247 lines). `ModuleCoordinator` subclass no longer needed.
- The `cleanup()` fatal-exception logic moves into `coordinator/mod.rs` (the `BaseException`/`Exception` distinction handled via PyO3's `PyErr` type checking)

What changes in `__init__.py`:

- `ModuleCoordinator` import is replaced — `RustCoordinator` (now `PyCoordinator`) directly provides `process_hook_result()` and the cleanup behavior
- The public name `ModuleCoordinator` becomes an alias to `RustCoordinator` for backward compat

## Duplicated Logic Consolidation

### Config Validation (canonicalize in Rust, delete Python copies)

- `session.orchestrator` required check — currently in Python `session.py` AND Rust `PySession::new()`
- `session.context` required check — same duplication
- After: Rust `PySession::new()` is the single source of truth

### Mount-Point Presence Checks (canonicalize in Rust)

- "No orchestrator mounted" / "No context manager mounted" / "No providers mounted" — currently in `_session_exec.py`, `session.py`, AND partially in `PySession::execute_inner`
- After: Rust `PySession::execute()` does the check. `_session_exec.py`'s `run_orchestrator()` becomes thinner.

### Token Budget / Injection Size Validation (move to Rust)

- `injection_budget_per_turn`, `injection_size_limit`, `_current_turn_injections` — currently fields on Python `ModuleCoordinator`
- After: These become fields on `PyCoordinator` in `coordinator/hook_dispatch.rs`

### What We Explicitly DON'T Consolidate

- `_session_init.py` — depends on `ModuleLoader` (Python `importlib`), can't move to Rust without a Rust module loader
- `session.py` / `hooks.py` legacy files — left alive for test compatibility, noted as tech debt
- Pydantic models, error hierarchy, Protocol interfaces — Python ecosystem contract

### Net Python-Side Deletions

- `_rust_wrappers.py` — deleted entirely (~247 lines)
- Logic removed from `_session_exec.py` — mount-point checks (~15 lines)
- Token budget fields removed from Python wrapper layer

## Error Handling

- `cleanup()` fatal-exception logic (currently in Python `_rust_wrappers.py`) moves to `coordinator/mod.rs`. The `BaseException` vs `Exception` distinction is handled via PyO3's `PyErr::is_instance_of::<PyBaseException>()` type checking — fatal exceptions propagate immediately, non-fatal exceptions are collected and logged.
- `process_hook_result()` errors: bridge call failures (e.g., Python `add_message()` raises) are caught at the Rust level and converted to appropriate `PyErr` types, preserving the existing error contract.
- Token budget violations return descriptive errors (injection size exceeded, per-turn budget exhausted) rather than silently dropping injections.

## External Contract Surface

Confirmed by exploration of amplifier-foundation and amplifier-app-cli:

**Fully internal (safe to refactor freely):**

- `_rust_wrappers` module and `process_hook_result()` — zero external references
- `_engine` module — zero external references
- Raw `Rust*` type aliases

**Must preserve (external dependencies):**

- `AmplifierSession` constructor with 6 params + `.coordinator`, `.session_id`, `.config` (mutable dict), `.execute()`, `.initialize()`, `.cleanup()`, context manager protocol
- `ModuleCoordinator` with `.mount()`, `.get()`, `.register_capability()`, `.get_capability()`, `.hooks`, `.session_id`, `.session_state` (mutable dict), `.approval_system`, `.display_system`, `.cancellation`
- `HookResult` via 3 import paths (all must resolve to same type):
  - `amplifier_core.HookResult`
  - `amplifier_core.models.HookResult`
  - `amplifier_core.hooks.HookResult`
- All submodule import paths: `amplifier_core.models`, `amplifier_core.hooks`, `amplifier_core.llm_errors`, `amplifier_core.loader`, `amplifier_core.validation`, `amplifier_core.events`, `amplifier_core.approval`, `amplifier_core.message_models`

## Testing Strategy

### Primary Safety Net — Existing 517 Python Tests

- Run `cd amplifier-core && maturin develop && uv run pytest tests/ -q --tb=short -m "not slow"` before AND after
- Zero test failures is the gate. This is a mechanical refactor — no test should change behavior.

### Rust-Side Verification

- `cargo check` on `bindings/python/` after each file move — catches visibility errors immediately
- `cargo clippy` — catches dead imports, unused `pub(crate)` items
- Existing 13 unit tests in the `#[cfg(test)] mod tests` block stay in `lib.rs` (or move to a `tests.rs` if they need access to multiple modules)

### New Tests for Tier 3 Work

- `process_hook_result` routing: test each action branch (`inject_context`, `ask_user`, `user_message`, `continue`) with mock Python objects from Rust
- `PyContextManagerBridge`: test that it correctly calls the Python `add_message()` method
- Token budget enforcement: test that injections exceeding `injection_size_limit` are rejected, and that per-turn budget accumulation works

### Backward Compatibility Verification

- `HookResult` import path test: verify all 3 paths resolve to same type
- `ModuleCoordinator` alias test: verify `from amplifier_core import ModuleCoordinator` still works and the type has `process_hook_result`, `mount`, `get`, `hooks`, `session_state`, etc.
- `session.config` mutability test: verify `session.config["key"] = value` still works and reflects in `coordinator.config`

### What We Don't Test in This Pass

- Foundation and CLI integration — separate repos with their own CI. We verify the contract surface is unchanged but don't run their test suites here.
- Legacy `session.py` / `hooks.py` — left untouched, their existing tests continue to pass.

## Open Questions / Future Work

- **Legacy retirement:** `session.py` and `hooks.py` are left alive for test compatibility. Should be addressed in a future pass to expose `status`/`loader` on `RustSession`, migrate tests, and delete the legacy Python files.
- **Session init:** `_session_init.py` cannot move to Rust without a Rust module loader — future work if/when Rust module loading is implemented.
- **Coordinator growth:** `PyCoordinator` at ~900 lines is the largest module even after decomposition into 4 sub-files — monitor for further growth.
- **Node parity:** Node binding could adopt the same bridge pattern for `process_hook_result` when Node session lifecycle is implemented.
