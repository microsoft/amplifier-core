# Python Bindings Mechanical File Split — Phase 1 Implementation Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Split the monolithic `bindings/python/src/lib.rs` (4,129 lines) into 10 focused module files plus a slim router `lib.rs`, with zero behavior change.

**Architecture:** Each logical domain (helpers, bridges, session, coordinator, etc.) moves to its own file. `lib.rs` becomes a thin router: `mod` declarations, `pub(crate) use` re-exports, the `#[pymodule]` registration function, and the existing unit tests. This mirrors the Node binding split (PR #46). Every extracted type and function gets `pub(crate)` visibility; only `lib.rs` decides what's truly public via the `#[pymodule]`.

**Tech Stack:** Rust, PyO3 0.28, pyo3-async-runtimes (tokio), serde_json, amplifier-core kernel crate.

**Design doc:** `docs/plans/2026-03-14-python-bindings-split-design.md`

**Reference pattern:** `bindings/node/src/lib.rs` (45-line router with `mod` + `pub use` re-exports)

---

## How to Read This Plan

Every task follows the same loop:

1. **Create** a new `.rs` file with the import header shown
2. **Move** the specified line range out of `lib.rs` and paste it below the imports
3. **Annotate** every `struct`, `fn`, and `impl` that other modules reference with `pub(crate)`
4. **Wire** the new module into `lib.rs` by adding `mod foo;` and `pub(crate) use foo::*;`
5. **Delete** the moved lines from `lib.rs`
6. **Verify** with `cargo check -p amplifier-core-py`
7. **Commit**

The `pub(crate) use foo::*;` re-export is a temporary bridge — it keeps the rest of `lib.rs` compiling by making all moved items available at the crate root, exactly where they were before. Task 11 replaces these wildcard re-exports with explicit ones.

`cargo check` is your safety net after every task. If it complains about a missing import, add it. If it complains about visibility, add `pub(crate)`. The compiler is always right.

---

## Key Paths

| What | Path (relative to repo root) |
|------|-----|
| File being split | `amplifier-core/bindings/python/src/lib.rs` |
| New module files | `amplifier-core/bindings/python/src/{helpers,bridges,cancellation,...}.rs` |
| Cargo workspace | `amplifier-core/` |
| Build config | `amplifier-core/bindings/python/Cargo.toml` (crate = `amplifier-core-py`) |
| Node reference | `amplifier-core/bindings/node/src/lib.rs` |
| Python tests | `amplifier-core/tests/` |

All `cargo` and `git` commands assume you are in `amplifier-core/`.

---

## Task 0: Verify Baseline

Before touching anything, prove the world is green.

**Step 1: Rust compilation check**
```bash
cd amplifier-core && cargo check -p amplifier-core-py
```
Expected: compiles with zero errors.

**Step 2: Rust unit tests**
```bash
cd amplifier-core && cargo test -p amplifier-core-py
```
Expected: 13 tests pass (all in `lib.rs::tests`).

**Step 3: Clippy**
```bash
cd amplifier-core && cargo clippy -p amplifier-core-py -- -W clippy::all
```
Expected: no errors (warnings are OK).

**Step 4: Commit baseline (if anything was dirty)**
```bash
cd amplifier-core && git add -A && git status
```
If clean, move on. If dirty, commit first:
```bash
git commit -m "chore: clean baseline before python bindings split"
```

---

## Task 1: Create `helpers.rs`

Extract the two shared utility functions that every other module depends on.

**Source lines:** `lib.rs` lines 30–51 (the `wrap_future_as_coroutine` function and `try_model_dump` function).

**File:** `bindings/python/src/helpers.rs`

**Step 1: Create the file with this exact content**

```rust
// ---------------------------------------------------------------------------
// Shared helper utilities
// ---------------------------------------------------------------------------

use pyo3::prelude::*;

/// Wrap a future_into_py result in a Python coroutine via _async_compat._wrap().
/// This makes PyO3 async methods return proper coroutines (not just awaitables),
/// ensuring compatibility with asyncio.create_task(), inspect.iscoroutine(), etc.
pub(crate) fn wrap_future_as_coroutine<'py>(
    py: Python<'py>,
    future: PyResult<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let future = future?;
    let wrapper = py
        .import("amplifier_core._async_compat")?
        .getattr("_wrap")?;
    wrapper.call1((&future,))
}

/// Try `model_dump()` on a Python object (Pydantic BaseModel → dict).
/// Falls back to the original object reference if not a Pydantic model.
pub(crate) fn try_model_dump<'py>(obj: &Bound<'py, PyAny>) -> Bound<'py, PyAny> {
    match obj.call_method0("model_dump") {
        Ok(dict) => dict,
        Err(_) => obj.clone(),
    }
}
```

This is the **complete file** — nothing to move from lib.rs, you're writing it fresh based on the existing code.

**Step 2: Wire into lib.rs**

At the top of `lib.rs`, directly after the top-level `use` block (after line 28), add:

```rust
mod helpers;
pub(crate) use helpers::*;
```

**Step 3: Delete lines 30–51 from lib.rs**

Remove the `wrap_future_as_coroutine` function (lines 30–42) and `try_model_dump` function (lines 44–51). Leave the blank line at 52 if you like.

**Step 4: Verify**
```bash
cargo check -p amplifier-core-py
```
Expected: compiles. All remaining code in `lib.rs` still finds `wrap_future_as_coroutine` and `try_model_dump` via the wildcard re-export.

**Step 5: Commit**
```bash
git add bindings/python/src/helpers.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract helpers.rs (wrap_future_as_coroutine, try_model_dump)"
```

---

## Task 2: Create `bridges.rs`

Extract the three trait bridge structs that adapt Python callables into Rust traits.

**Source lines:** `lib.rs` lines 53–383 (from the `// --- PyHookHandlerBridge` banner through the closing `}` of `PyDisplayServiceBridge`'s `show_message` impl).

**File:** `bindings/python/src/bridges.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// Trait bridges — adapt Python callables into Rust trait objects
// ---------------------------------------------------------------------------

use std::future::Future;
use std::pin::Pin;

use pyo3::prelude::*;
use serde_json::Value;

use amplifier_core::errors::{AmplifierError, HookError, SessionError};
use amplifier_core::models::{HookAction, HookResult};
use amplifier_core::traits::HookHandler;

use crate::helpers::try_model_dump;

// Paste lines 53–383 from lib.rs below this line.
// (PyHookHandlerBridge, PyApprovalProviderBridge, PyDisplayServiceBridge)
```

**Step 2: Move lines 53–383 from lib.rs into `bridges.rs`**

Paste them below the import header. Then make these visibility changes:

| Item | Change |
|------|--------|
| `struct PyHookHandlerBridge` | → `pub(crate) struct PyHookHandlerBridge` |
| `struct PyApprovalProviderBridge` | → `pub(crate) struct PyApprovalProviderBridge` |
| `struct PyDisplayServiceBridge` | → `pub(crate) struct PyDisplayServiceBridge` |

The `unsafe impl Send/Sync` blocks and the trait `impl` blocks do **not** need `pub(crate)` — they are inherent to the struct.

**Step 3: Wire into lib.rs**

Add after the `mod helpers;` line:

```rust
mod bridges;
pub(crate) use bridges::*;
```

**Step 4: Delete lines 53–383 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

**Step 6: Commit**
```bash
git add bindings/python/src/bridges.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract bridges.rs (PyHookHandlerBridge, PyApprovalProviderBridge, PyDisplayServiceBridge)"
```

---

## Task 3: Create `cancellation.rs`

Extract the `PyCancellationToken` pyclass.

**Source lines:** `lib.rs` lines 1319–1497 (from the `// --- PyCancellationToken` banner through the closing `}` of the `#[pymethods]` impl).

**File:** `bindings/python/src/cancellation.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// PyCancellationToken — wraps amplifier_core::CancellationToken
// ---------------------------------------------------------------------------

use std::collections::HashSet;
use std::sync::Arc;

use pyo3::prelude::*;

use crate::helpers::wrap_future_as_coroutine;

// Paste lines 1319–1497 from lib.rs below this line.
// (Skip the banner comment — it's already above.)
```

**Step 2: Move lines 1319–1497 from lib.rs into `cancellation.rs`**

Skip the duplicate banner comment (the file header already has it). Then make these changes:

| Item | Change |
|------|--------|
| `struct PyCancellationToken` | → `pub(crate) struct PyCancellationToken` |

Note: The `#[pyclass(name = "RustCancellationToken")]` attribute stays. The `#[pymethods]` impl block and its methods do not need `pub(crate)`.

**Step 3: Wire into lib.rs**

Add after the other `mod` declarations:

```rust
mod cancellation;
pub(crate) use cancellation::*;
```

**Step 4: Delete lines 1319–1497 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

**Step 6: Commit**
```bash
git add bindings/python/src/cancellation.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract cancellation.rs (PyCancellationToken)"
```

---

## Task 4: Create `errors.rs`

Extract the `PyProviderError` pyclass, including its non-pymethods `from_rust()` impl block.

**Source lines:** `lib.rs` lines 2399–2652 (from the `// --- PyProviderError` banner through the closing `}` of the `impl PyProviderError { fn from_rust(...) }` block).

**File:** `bindings/python/src/errors.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// PyProviderError — exposes amplifier_core::errors::ProviderError fields
// ---------------------------------------------------------------------------

use pyo3::prelude::*;

// Paste lines 2399–2652 from lib.rs below this line.
```

**Step 2: Move lines 2399–2652 from lib.rs into `errors.rs`**

Make these changes:

| Item | Change |
|------|--------|
| `struct PyProviderError` | → `pub(crate) struct PyProviderError` |
| `fn from_rust(...)` (the non-pymethods impl) | → `pub(crate) fn from_rust(...)` |

**Step 3: Wire into lib.rs**

```rust
mod errors;
pub(crate) use errors::*;
```

**Step 4: Delete lines 2399–2652 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

**Step 6: Commit**
```bash
git add bindings/python/src/errors.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract errors.rs (PyProviderError)"
```

---

## Task 5: Create `retry.rs`

Extract the `PyRetryConfig` pyclass and the two standalone pyfunctions (`classify_error_message`, `compute_delay`).

**Source lines:** `lib.rs` lines 2654–2855 (from the `// --- PyRetryConfig` banner through the closing `}` of `compute_delay`).

**File:** `bindings/python/src/retry.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// PyRetryConfig — wraps amplifier_core::retry::RetryConfig
// Retry utility functions: classify_error_message, compute_delay
// ---------------------------------------------------------------------------

use pyo3::prelude::*;

// Paste lines 2654–2855 from lib.rs below this line.
```

**Step 2: Move lines 2654–2855 from lib.rs into `retry.rs`**

Make these changes:

| Item | Change |
|------|--------|
| `struct PyRetryConfig` | → `pub(crate) struct PyRetryConfig` |
| `fn classify_error_message(...)` (the `#[pyfunction]`) | → `pub(crate) fn classify_error_message(...)` |
| `fn compute_delay(...)` (the `#[pyfunction]`) | → `pub(crate) fn compute_delay(...)` |

Note: `compute_delay` takes `&PyRetryConfig` as a parameter. Since both are in the same file, no cross-module import is needed.

**Step 3: Wire into lib.rs**

```rust
mod retry;
pub(crate) use retry::*;
```

**Step 4: Delete lines 2654–2855 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

**Step 6: Commit**
```bash
git add bindings/python/src/retry.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract retry.rs (PyRetryConfig, classify_error_message, compute_delay)"
```

---

## Task 6: Create `hooks.rs`

Extract `PyUnregisterFn` and `PyHookRegistry`. This is the first module that depends on another extracted module (`bridges.rs` — it creates `Arc<PyHookHandlerBridge>` in the `register()` method).

**Source lines:** `lib.rs` lines 1048–1317 (from the `// --- PyUnregisterFn` banner through the closing `}` of the `PyHookRegistry` `#[pymethods]` impl, including the `#[classattr]` event constants).

**File:** `bindings/python/src/hooks.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// PyUnregisterFn + PyHookRegistry — wraps amplifier_core::HookRegistry
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;
use serde_json::Value;

use crate::bridges::PyHookHandlerBridge;
use crate::helpers::wrap_future_as_coroutine;

// Paste lines 1048–1317 from lib.rs below this line.
```

**Step 2: Move lines 1048–1317 from lib.rs into `hooks.rs`**

Make these changes:

| Item | Change |
|------|--------|
| `struct PyUnregisterFn` | → `pub(crate) struct PyUnregisterFn` |
| `struct PyHookRegistry` | → `pub(crate) struct PyHookRegistry` |

**Step 3: Wire into lib.rs**

```rust
mod hooks;
pub(crate) use hooks::*;
```

**Step 4: Delete lines 1048–1317 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

If you get import errors about `amplifier_core::HookRegistry` or `amplifier_core::traits::HookHandler`, add them to the import header of `hooks.rs`. The exact imports depend on which types the `register()` and `emit()` methods reference — let `cargo check` guide you.

**Step 6: Commit**
```bash
git add bindings/python/src/hooks.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract hooks.rs (PyUnregisterFn, PyHookRegistry)"
```

---

## Task 7: Create `session.rs`

Extract the `PySession` pyclass. This module references `PyCoordinator` (still in `lib.rs` at this point) and `PyHookRegistry` (already in `hooks.rs`). Both are accessible from `session.rs` via `use crate::` — `PyCoordinator` because private items in the crate root are visible to descendant modules, and `PyHookRegistry` via the wildcard re-export.

**Source lines:** `lib.rs` lines 385–1046 (from the `// --- PySession` banner through the closing `}` of the `__aexit__` method).

**File:** `bindings/python/src/session.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// PySession — wraps amplifier_core::Session
// ---------------------------------------------------------------------------

use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::coordinator::PyCoordinator;
use crate::helpers::wrap_future_as_coroutine;
use crate::hooks::PyHookRegistry;

// Paste lines 385–1046 from lib.rs below this line.
```

> **Important:** At this point, `coordinator.rs` does not exist yet — `PyCoordinator` is still defined in `lib.rs`. The import `use crate::coordinator::PyCoordinator;` will NOT work yet. Instead, temporarily use:
>
> ```rust
> use crate::PyCoordinator;
> ```
>
> This works because `PyCoordinator` is defined at the crate root (in `lib.rs`), and child modules can access items from their parent module. When `coordinator.rs` is extracted in Task 9 and `pub(crate) use coordinator::*;` is added to `lib.rs`, this import path continues to resolve via the re-export — no change needed.

**Step 2: Move lines 385–1046 from lib.rs into `session.rs`**

Make these changes:

| Item | Change |
|------|--------|
| `struct PySession` | → `pub(crate) struct PySession` |

**Step 3: Wire into lib.rs**

```rust
mod session;
pub(crate) use session::*;
```

**Step 4: Delete lines 385–1046 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

If you get errors about missing types (e.g., `tokio::sync::Mutex`, `Py<PyAny>`, `serde_json::Value`), add the relevant imports to `session.rs`. Let `cargo check` guide you — each error tells you exactly what's missing.

**Step 6: Commit**
```bash
git add bindings/python/src/session.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract session.rs (PySession)"
```

---

## Task 8: Create `module_resolver.rs`

Extract the two standalone pyfunctions for module resolution. This is a leaf module with no dependencies on other extracted modules.

**Source lines:** `lib.rs` lines 2857–2942 (from the `// --- Module resolver bindings` banner through the closing `}` of `load_wasm_from_path`).

**File:** `bindings/python/src/module_resolver.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// Module resolver bindings — resolve_module, load_wasm_from_path
// ---------------------------------------------------------------------------

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

// Paste lines 2857–2942 from lib.rs below this line.
```

**Step 2: Move lines 2857–2942 from lib.rs into `module_resolver.rs`**

Make these changes:

| Item | Change |
|------|--------|
| `fn resolve_module(...)` (the `#[pyfunction]`) | → `pub(crate) fn resolve_module(...)` |
| `fn load_wasm_from_path(...)` (the `#[pyfunction]`) | → `pub(crate) fn load_wasm_from_path(...)` |

**Step 3: Wire into lib.rs**

```rust
mod module_resolver;
pub(crate) use module_resolver::*;
```

**Step 4: Delete lines 2857–2942 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

**Step 6: Commit**
```bash
git add bindings/python/src/module_resolver.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract module_resolver.rs (resolve_module, load_wasm_from_path)"
```

---

## Task 9: Create `coordinator.rs`

Extract the `PyCoordinator` pyclass — the largest single extraction at ~900 lines. In Phase 1 this stays as one flat file. Phase 2 decomposes it into a subdirectory.

**Source lines:** `lib.rs` lines 1499–2397 (from the `// --- PyCoordinator` banner through the closing `}` of the `to_dict()` method).

**File:** `bindings/python/src/coordinator.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// PyCoordinator — wraps amplifier_core::Coordinator
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value;

use crate::cancellation::PyCancellationToken;
use crate::helpers::{try_model_dump, wrap_future_as_coroutine};
use crate::hooks::PyHookRegistry;

// Paste lines 1499–2397 from lib.rs below this line.
```

**Step 2: Move lines 1499–2397 from lib.rs into `coordinator.rs`**

Make these changes:

| Item | Change |
|------|--------|
| `struct PyCoordinator` | → `pub(crate) struct PyCoordinator` |
| All fields on `PyCoordinator` | → add `pub(crate)` to each field |

The fields need `pub(crate)` because `wasm.rs` (Task 10) accesses `coordinator.inner` and `coordinator.mount_points` in the `load_and_mount_wasm` function. Here's what the struct should look like after annotation:

```rust
#[pyclass(name = "RustCoordinator", subclass)]
pub(crate) struct PyCoordinator {
    pub(crate) inner: Arc<amplifier_core::Coordinator>,
    pub(crate) mount_points: Py<PyDict>,
    pub(crate) py_hooks: Py<PyAny>,
    pub(crate) py_cancellation: Py<PyCancellationToken>,
    pub(crate) session_ref: Py<PyAny>,
    pub(crate) session_id: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) config_dict: Py<PyAny>,
    pub(crate) capabilities: Py<PyDict>,
    pub(crate) cleanup_fns: Py<PyList>,
    pub(crate) channels_dict: Py<PyDict>,
    pub(crate) current_turn_injections: usize,
    pub(crate) approval_system_obj: Py<PyAny>,
    pub(crate) display_system_obj: Py<PyAny>,
    pub(crate) loader_obj: Py<PyAny>,
}
```

**Step 3: Wire into lib.rs**

```rust
mod coordinator;
pub(crate) use coordinator::*;
```

**Step 4: Delete lines 1499–2397 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

This is the biggest extraction. If `cargo check` complains about missing imports, add them. Common ones you might need: `log::error!` / `log::warn!` (used in cleanup/mount methods), or additional `amplifier_core::` types used in `to_dict()`.

**Step 6: Commit**
```bash
git add bindings/python/src/coordinator.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract coordinator.rs (PyCoordinator, ~900 lines)"
```

---

## Task 10: Create `wasm.rs`

Extract all 6 `PyWasm*` wrapper classes, the `NullContextManager` helper struct, and the `load_and_mount_wasm` pyfunction.

**Source lines:** `lib.rs` lines 2944–3823 (from the `// --- PyWasmTool` banner through the closing `}` of `load_and_mount_wasm`).

This range includes:
- `PyWasmTool` (lines 2944–3048)
- `PyWasmProvider` (lines 3050–3190)
- `PyWasmHook` (lines 3192–3263)
- `PyWasmContext` (lines 3265–3445)
- `PyWasmOrchestrator` (lines 3447–3556)
- `NullContextManager` (lines 3558–3598)
- `PyWasmApproval` (lines 3600–3676)
- `load_and_mount_wasm` (lines 3678–3823)

**File:** `bindings/python/src/wasm.rs`

**Step 1: Create the file with this import header**

```rust
// ---------------------------------------------------------------------------
// WASM module wrappers — PyWasmTool, PyWasmProvider, PyWasmHook,
// PyWasmContext, PyWasmOrchestrator, PyWasmApproval, load_and_mount_wasm
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

use crate::coordinator::PyCoordinator;
use crate::helpers::{try_model_dump, wrap_future_as_coroutine};

// Paste lines 2944–3823 from lib.rs below this line.
```

**Step 2: Move lines 2944–3823 from lib.rs into `wasm.rs`**

Make these changes:

| Item | Change |
|------|--------|
| `struct PyWasmTool` | → `pub(crate) struct PyWasmTool` |
| `struct PyWasmProvider` | → `pub(crate) struct PyWasmProvider` |
| `struct PyWasmHook` | → `pub(crate) struct PyWasmHook` |
| `struct PyWasmContext` | → `pub(crate) struct PyWasmContext` |
| `struct PyWasmOrchestrator` | → `pub(crate) struct PyWasmOrchestrator` |
| `struct NullContextManager` | → `pub(crate) struct NullContextManager` |
| `struct PyWasmApproval` | → `pub(crate) struct PyWasmApproval` |
| `fn load_and_mount_wasm(...)` | → `pub(crate) fn load_and_mount_wasm(...)` |

Also add `pub(crate)` to the `inner` field of each `PyWasm*` struct (they're constructed in `load_and_mount_wasm` which is the same file, but `pub(crate)` is the standard for this split).

**Step 3: Wire into lib.rs**

```rust
mod wasm;
pub(crate) use wasm::*;
```

**Step 4: Delete lines 2944–3823 from lib.rs**

**Step 5: Verify**
```bash
cargo check -p amplifier-core-py
```

If `cargo check` complains about `amplifier_core::ContextError` (used in `NullContextManager`), add `use amplifier_core::ContextError;` to the imports.

**Step 6: Commit**
```bash
git add bindings/python/src/wasm.rs bindings/python/src/lib.rs
git commit -m "refactor(py): extract wasm.rs (6 PyWasm* classes, NullContextManager, load_and_mount_wasm)"
```

---

## Task 11: Rewrite `lib.rs` as a Router

After Tasks 1–10, `lib.rs` should contain only:
- The module doc comment (original lines 1–14)
- Some now-unused `use` statements (original lines 16–28)
- The `mod` + `pub(crate) use` lines you've been adding
- `#[pymodule] fn _engine(...)` (original lines 3825–4003)
- `#[cfg(test)] mod tests` (original lines 4005–4129)

Now rewrite `lib.rs` as a clean router. Replace its entire contents with:

**File:** `bindings/python/src/lib.rs`

**Step 1: Write the complete new lib.rs**

```rust
//! PyO3 bridge for amplifier-core.
//!
//! This crate wraps the pure Rust kernel types and exposes them
//! as Python classes via PyO3. It compiles into the `_engine`
//! extension module that ships inside the `amplifier_core` Python package.
//!
//! # Exposed classes
//!
//! | Python name             | Rust wrapper         | Inner type                  |
//! |-------------------------|----------------------|-----------------------------|
//! | `RustSession`           | [`PySession`]        | `amplifier_core::Session`   |
//! | `RustHookRegistry`      | [`PyHookRegistry`]   | `amplifier_core::HookRegistry` |
//! | `RustCancellationToken` | [`PyCancellationToken`] | `amplifier_core::CancellationToken` |
//! | `RustCoordinator`       | [`PyCoordinator`]    | `amplifier_core::Coordinator` |

// ---- Module declarations ----
mod bridges;
mod cancellation;
mod coordinator;
mod errors;
mod helpers;
mod hooks;
mod module_resolver;
mod retry;
mod session;
mod wasm;

// ---- Re-exports (pub(crate) so #[pymodule] + tests can see them) ----
pub(crate) use cancellation::PyCancellationToken;
pub(crate) use coordinator::PyCoordinator;
pub(crate) use errors::PyProviderError;
pub(crate) use hooks::{PyHookRegistry, PyUnregisterFn};
pub(crate) use module_resolver::{load_wasm_from_path, resolve_module};
pub(crate) use retry::{classify_error_message, compute_delay, PyRetryConfig};
pub(crate) use session::PySession;
pub(crate) use wasm::{
    load_and_mount_wasm, PyWasmApproval, PyWasmContext, PyWasmHook, PyWasmOrchestrator,
    PyWasmProvider, PyWasmTool,
};

use pyo3::prelude::*;

// ---- Module registration ----

/// The compiled Rust extension module.
/// Python imports this as `amplifier_core._engine`.
#[pymodule]
fn _engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();
    m.add("__version__", "1.0.0")?;
    m.add("RUST_AVAILABLE", true)?;

    // Classes
    m.add_class::<PySession>()?;
    m.add_class::<PyUnregisterFn>()?;
    m.add_class::<PyHookRegistry>()?;
    m.add_class::<PyCancellationToken>()?;
    m.add_class::<PyCoordinator>()?;
    m.add_class::<PyProviderError>()?;
    m.add_class::<PyRetryConfig>()?;
    m.add_class::<PyWasmTool>()?;
    m.add_class::<PyWasmProvider>()?;
    m.add_class::<PyWasmHook>()?;
    m.add_class::<PyWasmContext>()?;
    m.add_class::<PyWasmOrchestrator>()?;
    m.add_class::<PyWasmApproval>()?;

    // Functions
    m.add_function(wrap_pyfunction!(classify_error_message, m)?)?;
    m.add_function(wrap_pyfunction!(compute_delay, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_module, m)?)?;
    m.add_function(wrap_pyfunction!(load_wasm_from_path, m)?)?;
    m.add_function(wrap_pyfunction!(load_and_mount_wasm, m)?)?;

    // -----------------------------------------------------------------------
    // Event constants — expose all 41 canonical events from amplifier_core
    // -----------------------------------------------------------------------

    // Session lifecycle
    m.add("SESSION_START", amplifier_core::events::SESSION_START)?;
    m.add("SESSION_END", amplifier_core::events::SESSION_END)?;
    m.add("SESSION_FORK", amplifier_core::events::SESSION_FORK)?;
    m.add("SESSION_RESUME", amplifier_core::events::SESSION_RESUME)?;

    // Prompt lifecycle
    m.add("PROMPT_SUBMIT", amplifier_core::events::PROMPT_SUBMIT)?;
    m.add("PROMPT_COMPLETE", amplifier_core::events::PROMPT_COMPLETE)?;

    // Planning
    m.add("PLAN_START", amplifier_core::events::PLAN_START)?;
    m.add("PLAN_END", amplifier_core::events::PLAN_END)?;

    // Provider calls
    m.add("PROVIDER_REQUEST", amplifier_core::events::PROVIDER_REQUEST)?;
    m.add(
        "PROVIDER_RESPONSE",
        amplifier_core::events::PROVIDER_RESPONSE,
    )?;
    m.add("PROVIDER_RETRY", amplifier_core::events::PROVIDER_RETRY)?;
    m.add("PROVIDER_ERROR", amplifier_core::events::PROVIDER_ERROR)?;
    m.add(
        "PROVIDER_THROTTLE",
        amplifier_core::events::PROVIDER_THROTTLE,
    )?;
    m.add(
        "PROVIDER_TOOL_SEQUENCE_REPAIRED",
        amplifier_core::events::PROVIDER_TOOL_SEQUENCE_REPAIRED,
    )?;
    m.add("PROVIDER_RESOLVE", amplifier_core::events::PROVIDER_RESOLVE)?;

    // LLM events
    m.add("LLM_REQUEST", amplifier_core::events::LLM_REQUEST)?;
    m.add("LLM_RESPONSE", amplifier_core::events::LLM_RESPONSE)?;

    // Content block events
    m.add(
        "CONTENT_BLOCK_START",
        amplifier_core::events::CONTENT_BLOCK_START,
    )?;
    m.add(
        "CONTENT_BLOCK_DELTA",
        amplifier_core::events::CONTENT_BLOCK_DELTA,
    )?;
    m.add(
        "CONTENT_BLOCK_END",
        amplifier_core::events::CONTENT_BLOCK_END,
    )?;

    // Thinking events
    m.add("THINKING_DELTA", amplifier_core::events::THINKING_DELTA)?;
    m.add("THINKING_FINAL", amplifier_core::events::THINKING_FINAL)?;

    // Tool invocations
    m.add("TOOL_PRE", amplifier_core::events::TOOL_PRE)?;
    m.add("TOOL_POST", amplifier_core::events::TOOL_POST)?;
    m.add("TOOL_ERROR", amplifier_core::events::TOOL_ERROR)?;

    // Context management
    m.add(
        "CONTEXT_PRE_COMPACT",
        amplifier_core::events::CONTEXT_PRE_COMPACT,
    )?;
    m.add(
        "CONTEXT_POST_COMPACT",
        amplifier_core::events::CONTEXT_POST_COMPACT,
    )?;
    m.add(
        "CONTEXT_COMPACTION",
        amplifier_core::events::CONTEXT_COMPACTION,
    )?;
    m.add("CONTEXT_INCLUDE", amplifier_core::events::CONTEXT_INCLUDE)?;

    // Orchestrator lifecycle
    m.add(
        "ORCHESTRATOR_COMPLETE",
        amplifier_core::events::ORCHESTRATOR_COMPLETE,
    )?;
    m.add("EXECUTION_START", amplifier_core::events::EXECUTION_START)?;
    m.add("EXECUTION_END", amplifier_core::events::EXECUTION_END)?;

    // User notifications
    m.add(
        "USER_NOTIFICATION",
        amplifier_core::events::USER_NOTIFICATION,
    )?;

    // Artifacts
    m.add("ARTIFACT_WRITE", amplifier_core::events::ARTIFACT_WRITE)?;
    m.add("ARTIFACT_READ", amplifier_core::events::ARTIFACT_READ)?;

    // Policy / approvals
    m.add("POLICY_VIOLATION", amplifier_core::events::POLICY_VIOLATION)?;
    m.add(
        "APPROVAL_REQUIRED",
        amplifier_core::events::APPROVAL_REQUIRED,
    )?;
    m.add("APPROVAL_GRANTED", amplifier_core::events::APPROVAL_GRANTED)?;
    m.add("APPROVAL_DENIED", amplifier_core::events::APPROVAL_DENIED)?;

    // Cancellation lifecycle
    m.add("CANCEL_REQUESTED", amplifier_core::events::CANCEL_REQUESTED)?;
    m.add("CANCEL_COMPLETED", amplifier_core::events::CANCEL_COMPLETED)?;

    // Aggregate list of all events
    m.add("ALL_EVENTS", amplifier_core::events::ALL_EVENTS.to_vec())?;

    // -----------------------------------------------------------------------
    // Capabilities — expose all 16 well-known capability constants
    // -----------------------------------------------------------------------

    // Capabilities — Tier 1 (core)
    m.add("TOOLS", amplifier_core::capabilities::TOOLS)?;
    m.add("STREAMING", amplifier_core::capabilities::STREAMING)?;
    m.add("THINKING", amplifier_core::capabilities::THINKING)?;
    m.add("VISION", amplifier_core::capabilities::VISION)?;
    m.add("JSON_MODE", amplifier_core::capabilities::JSON_MODE)?;
    // Capabilities — Tier 2 (extended)
    m.add("FAST", amplifier_core::capabilities::FAST)?;
    m.add(
        "CODE_EXECUTION",
        amplifier_core::capabilities::CODE_EXECUTION,
    )?;
    m.add("WEB_SEARCH", amplifier_core::capabilities::WEB_SEARCH)?;
    m.add("DEEP_RESEARCH", amplifier_core::capabilities::DEEP_RESEARCH)?;
    m.add("LOCAL", amplifier_core::capabilities::LOCAL)?;
    m.add("AUDIO", amplifier_core::capabilities::AUDIO)?;
    m.add(
        "IMAGE_GENERATION",
        amplifier_core::capabilities::IMAGE_GENERATION,
    )?;
    m.add("COMPUTER_USE", amplifier_core::capabilities::COMPUTER_USE)?;
    m.add("EMBEDDINGS", amplifier_core::capabilities::EMBEDDINGS)?;
    m.add("LONG_CONTEXT", amplifier_core::capabilities::LONG_CONTEXT)?;
    m.add("BATCH", amplifier_core::capabilities::BATCH)?;

    // Collections
    m.add(
        "ALL_WELL_KNOWN_CAPABILITIES",
        amplifier_core::capabilities::ALL_WELL_KNOWN_CAPABILITIES.to_vec(),
    )?;

    Ok(())
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify PySession type exists and is constructable.
    #[test]
    fn py_session_type_exists() {
        let _: fn() -> PySession = || panic!("just checking type exists");
    }

    /// Verify PyHookRegistry type exists and is constructable.
    #[test]
    fn py_hook_registry_type_exists() {
        let _: fn() -> PyHookRegistry = || panic!("just checking type exists");
    }

    /// Verify PyCancellationToken type exists and is constructable.
    #[test]
    fn py_cancellation_token_type_exists() {
        let _: fn() -> PyCancellationToken = || panic!("just checking type exists");
    }

    /// Verify PyCoordinator type name exists (no longer constructable without Python GIL).
    #[test]
    fn py_coordinator_type_exists() {
        fn _assert_type_compiles(_: &PyCoordinator) {}
    }

    /// Verify CancellationToken can be created and used without Python.
    #[test]
    fn cancellation_token_works_standalone() {
        let token = amplifier_core::CancellationToken::new();
        assert!(!token.is_cancelled());
        token.request_graceful();
        assert!(token.is_cancelled());
        assert!(token.is_graceful());
    }

    /// Verify HookRegistry can be created without Python.
    #[test]
    fn hook_registry_works_standalone() {
        let registry = amplifier_core::HookRegistry::new();
        let handlers = registry.list_handlers(None);
        assert!(handlers.is_empty());
    }

    /// Verify Session can be created without Python.
    #[test]
    fn session_works_standalone() {
        let config = amplifier_core::SessionConfig::minimal("loop-basic", "context-simple");
        let session = amplifier_core::Session::new(config, None, None);
        assert!(!session.session_id().is_empty());
        assert!(!session.is_initialized());
    }

    /// Verify that `log` and `pyo3-log` crates are available in the bindings crate.
    #[test]
    fn log_and_pyo3_log_available() {
        log::info!("test log from bindings crate");
        let _: fn() -> pyo3_log::ResetHandle = pyo3_log::init;
    }

    /// Verify PyWasmTool wrapper type exists.
    #[test]
    fn py_wasm_tool_type_exists() {
        fn _assert_type_compiles(_: &PyWasmTool) {}
    }

    /// Verify PyWasmHook wrapper type exists.
    #[test]
    fn py_wasm_hook_type_exists() {
        fn _assert_type_compiles(_: &PyWasmHook) {}
    }

    /// Verify PyWasmContext wrapper type exists.
    #[test]
    fn py_wasm_context_type_exists() {
        fn _assert_type_compiles(_: &PyWasmContext) {}
    }

    /// Verify PyWasmOrchestrator wrapper type exists.
    #[test]
    fn py_wasm_orchestrator_type_exists() {
        fn _assert_type_compiles(_: &PyWasmOrchestrator) {}
    }

    /// Verify PyWasmApproval wrapper type exists.
    #[test]
    fn py_wasm_approval_type_exists() {
        fn _assert_type_compiles(_: &PyWasmApproval) {}
    }

    /// Document the contract for load_and_mount_wasm.
    #[test]
    fn load_and_mount_wasm_contract() {
        use pyo3::types::PyDict;
        let _exists =
            load_and_mount_wasm as fn(Python<'_>, &PyCoordinator, String) -> PyResult<Py<PyDict>>;
    }
}
```

**Step 2: Verify**
```bash
cargo check -p amplifier-core-py
```

If `wrap_pyfunction!` cannot find functions from submodules, try using the fully-qualified path pattern instead:
```rust
m.add_function(wrap_pyfunction!(retry::classify_error_message, m)?)?;
```
But test the simple re-export approach first — PyO3 0.28+ should handle it.

**Step 3: Run Rust unit tests**
```bash
cargo test -p amplifier-core-py
```
Expected: all 13 tests pass.

**Step 4: Clippy**
```bash
cargo clippy -p amplifier-core-py -- -W clippy::all
```
Expected: no errors. Fix any unused-import warnings in module files.

**Step 5: Commit**
```bash
git add bindings/python/src/lib.rs
git commit -m "refactor(py): rewrite lib.rs as clean router (~230 lines)"
```

---

## Task 12: Final Verification

This is the full gate. Everything must pass.

**Step 1: Rust compilation**
```bash
cargo check -p amplifier-core-py
```
Expected: clean compile.

**Step 2: Clippy lint**
```bash
cargo clippy -p amplifier-core-py -- -W clippy::all
```
Expected: no errors.

**Step 3: Rust unit tests**
```bash
cargo test -p amplifier-core-py
```
Expected: 13 tests pass.

**Step 4: Build the Python extension**
```bash
cd amplifier-core && maturin develop
```
Expected: builds successfully, installs into the virtualenv.

**Step 5: Run the full Python test suite**
```bash
cd amplifier-core && uv run pytest tests/ -q --tb=short -m "not slow"
```
Expected: 517 tests pass, 0 failures. This is a mechanical refactor — zero test behavior should change.

**Step 6: Verify file count**

```bash
ls -la bindings/python/src/*.rs
```
Expected: 11 files:
```
bindings/python/src/bridges.rs
bindings/python/src/cancellation.rs
bindings/python/src/coordinator.rs
bindings/python/src/errors.rs
bindings/python/src/helpers.rs
bindings/python/src/hooks.rs
bindings/python/src/lib.rs
bindings/python/src/module_resolver.rs
bindings/python/src/retry.rs
bindings/python/src/session.rs
bindings/python/src/wasm.rs
```

**Step 7: Verify lib.rs is the router**
```bash
wc -l bindings/python/src/lib.rs
```
Expected: ~230 lines (the exact count depends on formatting, but it should be well under 300).

**Step 8: Final commit**
```bash
git add -A
git commit -m "refactor(py): complete Phase 1 — split lib.rs into 10 focused modules

Split the monolithic 4,129-line lib.rs into:
- helpers.rs (~30 lines) — wrap_future_as_coroutine, try_model_dump
- bridges.rs (~330 lines) — PyHookHandlerBridge, PyApprovalProviderBridge, PyDisplayServiceBridge
- cancellation.rs (~180 lines) — PyCancellationToken
- errors.rs (~255 lines) — PyProviderError
- retry.rs (~200 lines) — PyRetryConfig, classify_error_message, compute_delay
- hooks.rs (~270 lines) — PyUnregisterFn, PyHookRegistry
- session.rs (~660 lines) — PySession
- module_resolver.rs (~90 lines) — resolve_module, load_wasm_from_path
- coordinator.rs (~900 lines) — PyCoordinator (single file; Phase 2 decomposes to subdir)
- wasm.rs (~880 lines) — 6 PyWasm* classes, NullContextManager, load_and_mount_wasm
- lib.rs (~230 lines) — router: mod decls, re-exports, #[pymodule], tests

Zero behavior change. All 517 Python tests pass. All 13 Rust tests pass."
```

---

## Cross-Module Visibility Reference

This table summarizes which items are `pub(crate)` and where they're referenced. Useful when debugging import errors.

| Item | Defined In | Referenced From | Import Path |
|------|-----------|----------------|-------------|
| `wrap_future_as_coroutine` | `helpers.rs` | 8+ methods across all modules | `use crate::helpers::wrap_future_as_coroutine;` |
| `try_model_dump` | `helpers.rs` | `bridges.rs`, `coordinator.rs`, `wasm.rs` | `use crate::helpers::try_model_dump;` |
| `PyHookHandlerBridge` | `bridges.rs` | `hooks.rs` | `use crate::bridges::PyHookHandlerBridge;` |
| `PyApprovalProviderBridge` | `bridges.rs` | (Phase 2: coordinator/hook_dispatch) | `use crate::bridges::PyApprovalProviderBridge;` |
| `PyDisplayServiceBridge` | `bridges.rs` | (Phase 2: coordinator/hook_dispatch) | `use crate::bridges::PyDisplayServiceBridge;` |
| `PyCancellationToken` | `cancellation.rs` | `coordinator.rs` | `use crate::cancellation::PyCancellationToken;` |
| `PyHookRegistry` | `hooks.rs` | `session.rs`, `coordinator.rs` | `use crate::hooks::PyHookRegistry;` |
| `PyUnregisterFn` | `hooks.rs` | `lib.rs` (pymodule registration) | `use crate::hooks::PyUnregisterFn;` |
| `PyCoordinator` | `coordinator.rs` | `session.rs`, `wasm.rs` | `use crate::coordinator::PyCoordinator;` |
| `PyProviderError` | `errors.rs` | `lib.rs` (pymodule registration) | `use crate::errors::PyProviderError;` |
| `PyRetryConfig` | `retry.rs` | `lib.rs` (pymodule registration) | `use crate::retry::PyRetryConfig;` |
| `classify_error_message` | `retry.rs` | `lib.rs` (pymodule registration) | `use crate::retry::classify_error_message;` |
| `compute_delay` | `retry.rs` | `lib.rs` (pymodule registration) | `use crate::retry::compute_delay;` |
| `load_and_mount_wasm` | `wasm.rs` | `lib.rs` (pymodule registration) | `use crate::wasm::load_and_mount_wasm;` |
| `PyWasmTool` + 5 siblings | `wasm.rs` | `lib.rs` (pymodule registration) | `use crate::wasm::PyWasmTool;` etc. |

---

## Troubleshooting

### "cannot find type `PyCoordinator` in this scope" in session.rs

During Tasks 1–8, `PyCoordinator` is still defined in `lib.rs`. Use `use crate::PyCoordinator;` (not `use crate::coordinator::PyCoordinator;`). After Task 9 extracts coordinator.rs, the re-export in lib.rs ensures the path still works.

### "cannot find function `wrap_pyfunction`" or "no function `classify_error_message`"

`wrap_pyfunction!` needs the function to be reachable at the call site. If `pub(crate) use retry::classify_error_message;` at the top of lib.rs doesn't work with `wrap_pyfunction!(classify_error_message, m)`, try the fully-qualified form: `wrap_pyfunction!(crate::retry::classify_error_message, m)`.

### Unused import warnings after extraction

Expected during Tasks 1–10 — the top-level `use` statements in `lib.rs` become progressively unused as code moves out. Don't waste time cleaning them up incrementally; Task 11 replaces all of lib.rs.

### `cargo check` passes but `cargo test` fails

The tests use `use super::*;` which depends on the re-exports in lib.rs. Make sure your `pub(crate) use foo::*;` lines are present. If a test can't find a type, the re-export is missing.

### Line numbers shifted after a previous task

After each extraction, the line numbers in lib.rs shift (because you deleted lines). The line numbers in this plan refer to the **original** 4,129-line file. If you're doing tasks sequentially, use the banner comments (`// --- PySession`, `// --- PyCoordinator`, etc.) to find the right sections, not line numbers.

---

## What's NOT in Phase 1

These are explicitly out of scope. Do not do them:

- **Coordinator subdirectory decomposition** (Phase 2) — `coordinator.rs` stays as one flat file
- **`process_hook_result` in Rust** (Phase 3) — no new behavior
- **Deleting `_rust_wrappers.py`** (Phase 3) — Python wrapper layer stays untouched
- **Any behavior changes** — this is a mechanical file move, period
