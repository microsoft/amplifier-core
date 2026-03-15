# Python Bindings Split — Phase 2: Coordinator Decomposition

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Decompose the monolithic `coordinator.rs` file (created by Phase 1) into a `coordinator/` subdirectory with 4 focused sub-modules, using PyO3's multiple `#[pymethods] impl` block pattern.

**Architecture:** PyO3 allows splitting a single `#[pyclass]` struct's method implementations across multiple files via separate `#[pymethods] impl` blocks in sibling modules. We exploit this to break `PyCoordinator`'s ~850 lines into a directory: `mod.rs` keeps the struct definition + lifecycle, `mount_points.rs` gets module storage/retrieval, `capabilities.rs` gets extensibility methods, and `hook_dispatch.rs` is created as a placeholder for Phase 3 work. All fields become `pub(crate)` so sub-modules can access them via `use super::PyCoordinator`.

**Tech Stack:** Rust, PyO3 0.28, pyo3-async-runtimes (tokio), amplifier-core kernel crate.

**Prerequisite:** Phase 1 is complete. The file `bindings/python/src/coordinator.rs` exists as a standalone file containing the entire `PyCoordinator` class extracted from `lib.rs`. The `lib.rs` file has `mod coordinator;` and `use coordinator::PyCoordinator;`.

---

## How This Plan Is Organized

There are **6 tasks**. Each one is a small, self-contained step. You do them in order. Do NOT skip ahead.

| Task | What happens | Risk level |
|------|-------------|------------|
| 1 | Convert `coordinator.rs` → `coordinator/mod.rs` (file rename) | Low — pure file move |
| 2 | Create `coordinator/mount_points.rs` (extract methods) | Medium — must get imports right |
| 3 | Create `coordinator/capabilities.rs` (extract methods) | Medium — same pattern as Task 2 |
| 4 | Create `coordinator/hook_dispatch.rs` (placeholder) | Low — empty file |
| 5 | Clean up `coordinator/mod.rs` (remove extracted code, add `mod` decls) | Medium — must not remove too much |
| 6 | Final verification (cargo check, clippy, maturin, pytest) | Low — just running commands |

---

## Current State (What You're Working With)

After Phase 1, the file layout is:

```
bindings/python/src/
├── lib.rs              ← has `mod coordinator;` and `use coordinator::PyCoordinator;`
└── coordinator.rs      ← ~850 lines, entire PyCoordinator class
```

The `coordinator.rs` file contains:
1. A `#[pyclass(name = "RustCoordinator", subclass)]` struct with ~16 fields
2. A single `#[pymethods] impl PyCoordinator` block with ~28 methods/properties

After Phase 2, the layout will be:

```
bindings/python/src/
├── lib.rs              ← unchanged `mod coordinator;` still works
└── coordinator/
    ├── mod.rs           ← struct def + lifecycle (~250 lines)
    ├── mount_points.rs  ← module storage/retrieval (~350 lines)
    ├── capabilities.rs  ← extensibility surface (~200 lines)
    └── hook_dispatch.rs ← placeholder for Phase 3 (~10 lines)
```

## Key Rust Concept You Need to Understand

PyO3 lets you have **multiple `#[pymethods] impl` blocks** for the same `#[pyclass]` struct, even across different files in the same crate. This is the entire mechanism that makes this split possible:

```rust
// coordinator/mod.rs — defines the struct
#[pyclass(subclass)]
pub(crate) struct PyCoordinator {
    pub(crate) inner: Arc<Coordinator>,  // pub(crate) so sub-modules can access
    // ...
}

#[pymethods]
impl PyCoordinator {
    #[new]
    fn new(...) -> Self { ... }  // constructor lives here
}

// coordinator/mount_points.rs — adds more methods to the SAME struct
use super::PyCoordinator;

#[pymethods]
impl PyCoordinator {
    fn mount(...) { ... }  // another #[pymethods] block, same struct
}
```

The Rust compiler merges all `#[pymethods] impl` blocks for the same type. Python sees one class with all methods combined.

---

## Method-to-Module Assignment

Here is the exact assignment of every existing method/property to its target module. **This is your source of truth.** If a method isn't listed, it stays in `mod.rs`.

### `coordinator/mod.rs` — Struct + Lifecycle (keeps these)

| Method/Property | Kind | Notes |
|----------------|------|-------|
| `PyCoordinator` struct definition | struct | ALL 16 fields, all become `pub(crate)` |
| `new()` | `#[new]` constructor | The `#[pyclass]` and `#[new]` must be in the same file |
| `_set_session()` | method | Session back-reference patching |
| `session_id` | `#[getter]` | |
| `parent_id` | `#[getter]` | |
| `session` | `#[getter]` | |
| `config` | `#[getter]` | |
| `cleanup()` | async method | Runs all registered cleanup fns |
| `_cleanup_fns` | `#[getter]` | Used by PySession |
| `to_dict()` | method | Introspection |

### `coordinator/mount_points.rs` — Module Storage & Retrieval (moves here)

| Method/Property | Kind | Notes |
|----------------|------|-------|
| `mount_points` | `#[getter]` + `#[setter]` | The dict itself |
| `mount()` | async method | Mount a module at a mount point |
| `get()` | method | Retrieve a mounted module |
| `unmount()` | async method | Remove a module from a mount point |
| `loader` | `#[getter]` + `#[setter]` | Module loader property |
| `approval_system` | `#[getter]` + `#[setter]` | Approval system property |
| `display_system` | `#[getter]` + `#[setter]` | Display system property |
| `hooks` | `#[getter]` | Hook registry property |
| `cancellation` | `#[getter]` | Cancellation token property |
| `reset_turn()` | method | Per-turn reset |
| `request_cancel()` | async method | Cancellation request |
| `_current_turn_injections` | `#[getter]` + `#[setter]` | Turn injection counter |
| `injection_budget_per_turn` | `#[getter]` | Config-derived budget |
| `injection_size_limit` | `#[getter]` | Config-derived limit |

### `coordinator/capabilities.rs` — Extensibility Surface (moves here)

| Method/Property | Kind | Notes |
|----------------|------|-------|
| `register_capability()` | method | Register a named capability |
| `get_capability()` | method | Retrieve a capability by name |
| `register_cleanup()` | method | Add a cleanup callable |
| `register_contributor()` | method | Register a contribution channel subscriber |
| `collect_contributions()` | async method | Collect from a channel |
| `channels` | `#[getter]` | Contribution channels dict |

### `coordinator/hook_dispatch.rs` — Placeholder

Empty `#[pymethods] impl PyCoordinator` block. Phase 3 will add `process_hook_result()` here.

---

### Task 1: Convert `coordinator.rs` to `coordinator/mod.rs`

**What you're doing:** Renaming a file and creating a directory. Zero code changes. Rust's module system resolves `mod coordinator;` to either `coordinator.rs` OR `coordinator/mod.rs` — they're equivalent. We're switching from the former to the latter.

**Files:**
- Delete: `bindings/python/src/coordinator.rs`
- Create: `bindings/python/src/coordinator/mod.rs` (identical content)
- Unchanged: `bindings/python/src/lib.rs` (the `mod coordinator;` line already works)

**Step 1: Create the coordinator directory and move the file**

Run:
```bash
cd amplifier-core && mkdir -p bindings/python/src/coordinator && mv bindings/python/src/coordinator.rs bindings/python/src/coordinator/mod.rs
```

Expected: Command succeeds silently. The file is now at `bindings/python/src/coordinator/mod.rs`.

**Step 2: Verify `cargo check` passes with zero code changes**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py 2>&1
```

Expected: `Finished` with no errors. Rust resolves `mod coordinator;` in `lib.rs` to `coordinator/mod.rs` automatically. If this fails, something went wrong with the file move — double-check the path.

**Step 3: Commit**

Run:
```bash
cd amplifier-core && git add bindings/python/src/coordinator/ && git add -u bindings/python/src/coordinator.rs && git commit -m "refactor(py): convert coordinator.rs to coordinator/mod.rs directory module"
```

---

### Task 2: Create `coordinator/mount_points.rs`

**What you're doing:** Creating a new file with a `#[pymethods] impl PyCoordinator` block containing all mount-point-related methods. You will CUT these methods from `mod.rs` and PASTE them here, then add the necessary imports at the top.

**Files:**
- Create: `bindings/python/src/coordinator/mount_points.rs`
- Modify: `bindings/python/src/coordinator/mod.rs` (add `mod mount_points;`, remove methods)

**Step 1: Create the `mount_points.rs` file**

Create `bindings/python/src/coordinator/mount_points.rs` with the following structure. The methods are CUT from `mod.rs` — you're moving them, not copying. I show the complete file below.

The file must start with these imports (adjust if the actual `mod.rs` uses slightly different imports for these methods — check before pasting):

```rust
//! Mount-point management for PyCoordinator.
//!
//! Contains all methods related to module storage, retrieval, turn tracking,
//! and system property access (approval, display, hooks, cancellation, loader).

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::helpers::wrap_future_as_coroutine;
use crate::bridges::PyApprovalProviderBridge;
use crate::bridges::PyDisplayServiceBridge;
use crate::cancellation::PyCancellationToken;

use std::sync::Arc;

use super::PyCoordinator;

#[pymethods]
impl PyCoordinator {
    // --- mount_points getter/setter ---
    // (CUT from mod.rs: the mount_points #[getter] and #[setter] methods)

    // --- mount() ---
    // (CUT from mod.rs: the entire mount() method)

    // --- get() ---
    // (CUT from mod.rs: the entire get() method)

    // --- unmount() ---
    // (CUT from mod.rs: the entire unmount() method)

    // --- loader getter/setter ---
    // (CUT from mod.rs: loader #[getter] and set_loader #[setter])

    // --- approval_system getter/setter ---
    // (CUT from mod.rs: approval_system #[getter] and set_approval_system #[setter])

    // --- display_system getter/setter ---
    // (CUT from mod.rs: display_system #[getter] and set_display_system #[setter])

    // --- hooks getter ---
    // (CUT from mod.rs: hooks #[getter])

    // --- cancellation getter ---
    // (CUT from mod.rs: cancellation #[getter])

    // --- request_cancel() ---
    // (CUT from mod.rs: the entire request_cancel() method)

    // --- reset_turn() ---
    // (CUT from mod.rs: the entire reset_turn() method)

    // --- _current_turn_injections getter/setter ---
    // (CUT from mod.rs: get_current_turn_injections and set_current_turn_injections)

    // --- injection_budget_per_turn getter ---
    // (CUT from mod.rs: the entire injection_budget_per_turn #[getter])

    // --- injection_size_limit getter ---
    // (CUT from mod.rs: the entire injection_size_limit #[getter])
}
```

**IMPORTANT:** Below is the **exact list of methods/properties to CUT from `mod.rs`** and paste into the `#[pymethods]` block above. Find each one by its function name. Cut the ENTIRE method including doc comments and `#[pyo3(...)]` / `#[getter]` / `#[setter]` attributes above it:

1. `fn mount_points(...)` — the `#[getter]` that returns `Bound<'py, PyDict>`
2. `fn set_mount_points(...)` — the `#[setter]`
3. `fn mount(...)` — the `#[pyo3(signature = (mount_point, module, name=None))]` method
4. `fn get(...)` — the `#[pyo3(signature = (mount_point, name=None))]` method
5. `fn unmount(...)` — the `#[pyo3(signature = (mount_point, name=None))]` method
6. `fn loader(...)` — the `#[getter]`
7. `fn set_loader(...)` — the `#[setter]`
8. `fn approval_system(...)` — the `#[getter]`
9. `fn set_approval_system(...)` — the `#[setter]`
10. `fn display_system(...)` — the `#[getter]`
11. `fn set_display_system(...)` — the `#[setter]`
12. `fn hooks(...)` — the `#[getter]`
13. `fn cancellation(...)` — the `#[getter]`
14. `fn request_cancel(...)` — the `#[pyo3(signature = (immediate=false))]` method
15. `fn reset_turn(...)` — the method
16. `fn get_current_turn_injections(...)` — the `#[getter(_current_turn_injections)]`
17. `fn set_current_turn_injections(...)` — the `#[setter(_current_turn_injections)]`
18. `fn injection_budget_per_turn(...)` — the `#[getter]`
19. `fn injection_size_limit(...)` — the `#[getter]`

**Step 2: Fix the imports at the top of `mount_points.rs`**

After pasting all the methods, read through each one and verify every type it uses is imported. Here's how to figure out what you need:

- Methods that use `wrap_future_as_coroutine` → need `use crate::helpers::wrap_future_as_coroutine;`
- Methods that use `pyo3_async_runtimes::tokio::future_into_py` → need that path (it's a direct call, no import needed beyond the crate dependency)
- `set_approval_system` creates `PyApprovalProviderBridge` → need `use crate::bridges::PyApprovalProviderBridge;`
- `set_display_system` creates `PyDisplayServiceBridge` → need `use crate::bridges::PyDisplayServiceBridge;`
- `cancellation` getter returns `Bound<'py, PyCancellationToken>` → need `use crate::cancellation::PyCancellationToken;`
- Methods using `Arc` → need `use std::sync::Arc;`
- Methods using `PyDict`, `PyValueError`, `PyRuntimeError` → need the PyO3 imports

**IMPORTANT about `crate::` paths:** After Phase 1, `helpers`, `bridges`, and `cancellation` are sibling modules at the crate root. From inside `coordinator/mount_points.rs`, you access them via `crate::helpers`, `crate::bridges`, `crate::cancellation`. You access the `PyCoordinator` struct via `super::PyCoordinator` (since `mod.rs` in the parent directory defines it).

If Phase 1 did NOT extract helpers/bridges/cancellation into separate modules (they may still be in `lib.rs`), then the types are accessible via `crate::` directly (e.g., `crate::wrap_future_as_coroutine`, `crate::PyApprovalProviderBridge`). **Read `lib.rs` to check.** The import paths must match the actual module structure.

**Step 3: Add `mod mount_points;` to `mod.rs`**

At the top of `coordinator/mod.rs`, after the existing `use` statements and before the `#[pyclass]` definition, add:

```rust
mod mount_points;
```

**Step 4: Verify `cargo check` passes**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py 2>&1
```

Expected: `Finished` with no errors.

**Common failures and how to fix them:**

| Error | Cause | Fix |
|-------|-------|-----|
| `field 'inner' is private` | Fields on `PyCoordinator` are private | Change field visibility in `mod.rs` from implicit private to `pub(crate)` — e.g., `pub(crate) inner: Arc<Coordinator>` |
| `cannot find function 'wrap_future_as_coroutine'` | Wrong import path | Check where the function lives (in `lib.rs` root? in `crate::helpers`?) and fix the `use` statement |
| `cannot find type 'PyCancellationToken'` | Wrong import path | Same — check where the type is defined and fix the `use` path |
| `duplicate definitions for 'mount_points'` | Method wasn't removed from `mod.rs` | Go back to `mod.rs` and delete the method you already moved |

**Step 5: Make ALL struct fields `pub(crate)` in `mod.rs`**

This is critical. The sub-modules need to access the struct's fields. In `coordinator/mod.rs`, change every field from:
```rust
    inner: Arc<amplifier_core::Coordinator>,
```
to:
```rust
    pub(crate) inner: Arc<amplifier_core::Coordinator>,
```

Do this for ALL 16 fields in the struct definition:
- `inner`
- `mount_points`
- `py_hooks`
- `py_cancellation`
- `session_ref`
- `session_id`
- `parent_id`
- `config_dict`
- `capabilities`
- `cleanup_fns`
- `channels_dict`
- `current_turn_injections`
- `approval_system_obj`
- `display_system_obj`
- `loader_obj`

**NOTE:** If `cargo check` in Step 4 already passed because fields were already `pub(crate)` from Phase 1, great — skip this. But if ANY field access error appeared, this is why.

**Step 6: Re-run `cargo check` after visibility fix**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py 2>&1
```

Expected: `Finished` with no errors.

**Step 7: Commit**

Run:
```bash
cd amplifier-core && git add bindings/python/src/coordinator/ && git commit -m "refactor(py): extract mount-point methods into coordinator/mount_points.rs"
```

---

### Task 3: Create `coordinator/capabilities.rs`

**What you're doing:** Same pattern as Task 2, but for the capability/extensibility methods.

**Files:**
- Create: `bindings/python/src/coordinator/capabilities.rs`
- Modify: `bindings/python/src/coordinator/mod.rs` (add `mod capabilities;`, remove methods)

**Step 1: Create the `capabilities.rs` file**

Create `bindings/python/src/coordinator/capabilities.rs` with this structure:

```rust
//! Capability registration and contribution channels for PyCoordinator.
//!
//! Contains methods for inter-module communication: capability registry,
//! cleanup function registration, and contribution channel management.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::PyCoordinator;

#[pymethods]
impl PyCoordinator {
    // (all methods cut from mod.rs go here)
}
```

**Step 2: CUT these methods from `mod.rs` and paste into the `#[pymethods]` block**

Here is the exact list of methods to move. Cut each one INCLUDING its doc comments and attributes:

1. `fn register_capability(...)` — the method that sets items on `self.capabilities`
2. `fn get_capability(...)` — the method that reads from `self.capabilities`
3. `fn register_cleanup(...)` — the method that appends to `self.cleanup_fns`
4. `fn register_contributor(...)` — the method that adds to `self.channels_dict`
5. `fn collect_contributions(...)` — the async method that collects from a channel
6. `fn channels(...)` — the `#[getter]` that returns `self.channels_dict`

**Step 3: Fix imports in `capabilities.rs`**

Read through each moved method and check what types it uses:

- `register_capability` uses `PyAny`, `Python` → covered by `pyo3::prelude::*`
- `register_cleanup` uses `PyAny`, `Python`, checks `is_callable()` → covered by `pyo3::prelude::*`
- `register_contributor` uses `PyDict`, `PyList` → need `use pyo3::types::{PyDict, PyList};`
- `collect_contributions` is the complex one:
  - If it uses `wrap_future_as_coroutine` → add `use crate::helpers::wrap_future_as_coroutine;` (or `crate::wrap_future_as_coroutine` depending on Phase 1 structure)
  - If it imports `amplifier_core._collect_helper` (Python-side) → no Rust import needed, it's a runtime `py.import()`
  - If it uses `pyo3_async_runtimes::tokio::future_into_py` → no import needed (fully qualified call)

**Step 4: Add `mod capabilities;` to `mod.rs`**

In `coordinator/mod.rs`, next to the existing `mod mount_points;`, add:

```rust
mod capabilities;
```

**Step 5: Verify `cargo check` passes**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py 2>&1
```

Expected: `Finished` with no errors.

**Troubleshooting:** Same table as Task 2. The most likely issue is a missing import or a method that wasn't fully removed from `mod.rs`.

**Step 6: Commit**

Run:
```bash
cd amplifier-core && git add bindings/python/src/coordinator/ && git commit -m "refactor(py): extract capability methods into coordinator/capabilities.rs"
```

---

### Task 4: Create `coordinator/hook_dispatch.rs` (Placeholder)

**What you're doing:** Creating an empty placeholder file. Phase 3 will fill it with `process_hook_result()` routing logic. Right now we just establish the file structure.

**Files:**
- Create: `bindings/python/src/coordinator/hook_dispatch.rs`
- Modify: `bindings/python/src/coordinator/mod.rs` (add `mod hook_dispatch;`)

**Step 1: Create the placeholder file**

Create `bindings/python/src/coordinator/hook_dispatch.rs` with exactly this content:

```rust
//! Hook result dispatch for PyCoordinator.
//!
//! This module will contain `process_hook_result()` — the routing logic
//! that dispatches `HookResult` actions to the appropriate bridges:
//!
//! - `inject_context` → PyContextManagerBridge
//! - `ask_user` → PyApprovalProviderBridge
//! - `user_message` → PyDisplayServiceBridge
//! - `continue` / None → no-op
//!
//! Also: token budget tracking (injection_budget_per_turn, injection_size_limit).
//!
//! **Status:** Placeholder — implementation is Phase 3 work.

// Phase 3 will add:
// use super::PyCoordinator;
// use crate::bridges::{PyApprovalProviderBridge, PyContextManagerBridge, PyDisplayServiceBridge};
//
// #[pymethods]
// impl PyCoordinator {
//     fn process_hook_result(...) { ... }
// }
```

**Step 2: Add `mod hook_dispatch;` to `mod.rs`**

In `coordinator/mod.rs`, add alongside the other mod declarations:

```rust
mod hook_dispatch;
```

**Step 3: Verify `cargo check` passes**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py 2>&1
```

Expected: `Finished` with no errors. An empty module (with only comments) compiles fine.

**Step 4: Commit**

Run:
```bash
cd amplifier-core && git add bindings/python/src/coordinator/hook_dispatch.rs bindings/python/src/coordinator/mod.rs && git commit -m "refactor(py): add placeholder coordinator/hook_dispatch.rs for Phase 3"
```

---

### Task 5: Clean Up `coordinator/mod.rs`

**What you're doing:** Verifying that `mod.rs` contains ONLY the struct definition, lifecycle methods, and `mod` declarations. Nothing else should remain. This is a verification + cleanup task.

**Files:**
- Modify: `bindings/python/src/coordinator/mod.rs`

**Step 1: Verify mod.rs structure**

Read `bindings/python/src/coordinator/mod.rs` and verify it contains EXACTLY these sections, in order:

1. **Module doc comment** — a `//!` comment describing the module
2. **Imports** — `use` statements for types needed by the remaining methods
3. **Sub-module declarations:**
   ```rust
   mod mount_points;
   mod capabilities;
   mod hook_dispatch;
   ```
4. **The `#[pyclass]` struct definition** with ALL fields `pub(crate)`:
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
5. **A `#[pymethods] impl PyCoordinator` block** containing ONLY these methods:
   - `new()` — the `#[new]` constructor
   - `session_id()` — `#[getter]`
   - `parent_id()` — `#[getter]`
   - `session()` — `#[getter]`
   - `_set_session()` — method
   - `config()` — `#[getter]`
   - `_cleanup_fns()` — `#[getter]`
   - `cleanup()` — async method
   - `to_dict()` — method

**Step 2: Remove any methods that should have been extracted but weren't**

If you find any of these methods STILL in `mod.rs`, they were missed during Tasks 2-3. Delete them now:

- `mount_points` getter/setter
- `mount()`, `get()`, `unmount()`
- `loader`, `set_loader`
- `approval_system`, `set_approval_system`
- `display_system`, `set_display_system`
- `hooks`, `cancellation`
- `request_cancel()`
- `reset_turn()`
- `get_current_turn_injections`, `set_current_turn_injections`
- `injection_budget_per_turn`, `injection_size_limit`
- `register_capability()`, `get_capability()`
- `register_cleanup()`
- `register_contributor()`, `collect_contributions()`
- `channels` getter

**Step 3: Remove unused imports from `mod.rs`**

After removing methods, some `use` statements in `mod.rs` will be unused. Run clippy to find them:

Run:
```bash
cd amplifier-core && cargo clippy -p amplifier-core-py -- -W clippy::all 2>&1
```

Look for warnings like `unused import: ...`. Remove each unused import.

The imports that should REMAIN in `mod.rs` (needed by the kept methods) are approximately:

```rust
use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
```

Plus whatever `crate::` imports are needed for:
- `PyCancellationToken` (used in struct definition)
- `PyHookRegistry` (used in `new()` to create the hooks instance)
- `wrap_future_as_coroutine` (used in `cleanup()`)
- `try_model_dump` (used in `new()` for config parsing)

**Step 4: Verify `cargo check` and `cargo clippy` both pass**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py 2>&1
```

Then:
```bash
cd amplifier-core && cargo clippy -p amplifier-core-py -- -W clippy::all 2>&1
```

Expected: Both pass with no errors. Clippy may produce warnings about unused code in OTHER files — that's fine. There should be NO errors and NO warnings from the `coordinator/` directory.

**Step 5: Verify the struct visibility is correct**

The struct itself needs to be `pub(crate)` so that `lib.rs` can re-export it. Verify the struct definition says:

```rust
#[pyclass(name = "RustCoordinator", subclass)]
pub(crate) struct PyCoordinator {
```

And that `lib.rs` has:
```rust
mod coordinator;
use coordinator::PyCoordinator;
```

(The `use` line should already exist from Phase 1. If it says `pub use`, that's fine too.)

**Step 6: Commit**

Run:
```bash
cd amplifier-core && git add bindings/python/src/coordinator/ && git commit -m "refactor(py): clean up coordinator/mod.rs after method extraction"
```

---

### Task 6: Final Verification

**What you're doing:** Running every verification command to confirm the refactor is behavior-preserving. This is the most important task. If ANY command fails, you must go back and fix the issue before proceeding.

**Files:** None modified. Read-only verification.

**Step 1: Cargo check**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py 2>&1
```

Expected: `Finished` with no errors.

**Step 2: Cargo clippy**

Run:
```bash
cd amplifier-core && cargo clippy -p amplifier-core-py -- -W clippy::all 2>&1
```

Expected: No errors, no warnings from `coordinator/` files.

**Step 3: Build the Python extension**

Run:
```bash
cd amplifier-core && maturin develop 2>&1
```

Expected: `Successfully installed amplifier-core-...`. This compiles the Rust crate and installs it as a Python package.

**Step 4: Run the full Python test suite**

Run:
```bash
cd amplifier-core && uv run pytest tests/ -q --tb=short -m "not slow" 2>&1
```

Expected: All tests pass. Zero failures. This is a mechanical refactor — no test should change behavior.

**Step 5: Run the bindings-specific tests**

Run:
```bash
cd amplifier-core && uv run pytest bindings/python/tests/ -q --tb=short 2>&1
```

Expected: All tests pass. These include:
- `test_switchover_coordinator.py` — exercises every `PyCoordinator` method
- `test_protocol_conformance.py` — verifies all import paths still work
- `test_switchover_session.py` — exercises `PySession` ↔ `PyCoordinator` interaction

**Step 6: Verify the file structure looks right**

Run:
```bash
find amplifier-core/bindings/python/src/coordinator/ -type f | sort
```

Expected output:
```
amplifier-core/bindings/python/src/coordinator/capabilities.rs
amplifier-core/bindings/python/src/coordinator/hook_dispatch.rs
amplifier-core/bindings/python/src/coordinator/mod.rs
amplifier-core/bindings/python/src/coordinator/mount_points.rs
```

**Step 7: Verify line counts are reasonable**

Run:
```bash
wc -l amplifier-core/bindings/python/src/coordinator/*.rs
```

Expected approximate line counts:
```
  ~250  mod.rs
  ~350  mount_points.rs
  ~200  capabilities.rs
   ~15  hook_dispatch.rs
  ~815  total
```

The total should be close to the original `coordinator.rs` line count (plus a few lines for module headers and import blocks in each file). If `mod.rs` is still 600+ lines, you forgot to extract some methods.

**Step 8: Final commit with all verification passing**

If any previous task commits were squashed or amended, do a final tag commit:

Run:
```bash
cd amplifier-core && git log --oneline -5
```

Verify you see commits for Tasks 1-5. The commit history should look like:
```
abc1234 refactor(py): clean up coordinator/mod.rs after method extraction
def5678 refactor(py): add placeholder coordinator/hook_dispatch.rs for Phase 3
ghi9012 refactor(py): extract capability methods into coordinator/capabilities.rs
jkl3456 refactor(py): extract mount-point methods into coordinator/mount_points.rs
mno7890 refactor(py): convert coordinator.rs to coordinator/mod.rs directory module
```

---

## Appendix A: Complete Import Reference

Here's a cheat sheet for which types live where, assuming Phase 1 extracted these modules from `lib.rs`. **You MUST verify these paths against the actual codebase before using them.** If Phase 1 kept everything in `lib.rs`, use `crate::TypeName` instead of `crate::module::TypeName`.

| Type / Function | Expected Location | Import Path |
|----------------|-------------------|-------------|
| `wrap_future_as_coroutine` | `helpers.rs` or `lib.rs` | `crate::helpers::wrap_future_as_coroutine` or `crate::wrap_future_as_coroutine` |
| `try_model_dump` | `helpers.rs` or `lib.rs` | `crate::helpers::try_model_dump` or `crate::try_model_dump` |
| `PyHookHandlerBridge` | `bridges.rs` or `lib.rs` | `crate::bridges::PyHookHandlerBridge` or `crate::PyHookHandlerBridge` |
| `PyApprovalProviderBridge` | `bridges.rs` or `lib.rs` | `crate::bridges::PyApprovalProviderBridge` or `crate::PyApprovalProviderBridge` |
| `PyDisplayServiceBridge` | `bridges.rs` or `lib.rs` | `crate::bridges::PyDisplayServiceBridge` or `crate::PyDisplayServiceBridge` |
| `PyHookRegistry` | `hooks.rs` or `lib.rs` | `crate::hooks::PyHookRegistry` or `crate::PyHookRegistry` |
| `PyCancellationToken` | `cancellation.rs` or `lib.rs` | `crate::cancellation::PyCancellationToken` or `crate::PyCancellationToken` |
| `PyCoordinator` | `coordinator/mod.rs` | `super::PyCoordinator` (from sub-modules) |

**How to check:** Run `grep -rn "pub.*struct PyApprovalProviderBridge" bindings/python/src/` to find where each type is defined.

## Appendix B: What NOT to Do

1. **Do NOT create new methods.** This is a mechanical refactor. If a method doesn't exist in the current `coordinator.rs`, don't invent it. The scope description mentions some aspirational methods (`get_tool()`, `mount_tool()`, `start_tool_tracking()`, etc.) that don't exist yet — those are future work, not this plan.

2. **Do NOT change method signatures.** Every method should have the EXACT same signature before and after the move. Don't "improve" anything.

3. **Do NOT change `lib.rs`.** The `mod coordinator;` and `use coordinator::PyCoordinator;` lines should work without modification. Rust auto-resolves `coordinator/mod.rs`.

4. **Do NOT change tests.** Zero test files should be modified. If a test fails, the refactor has a bug — fix the Rust code, not the test.

5. **Do NOT add `pub` to the sub-modules.** They should be `mod mount_points;` (private), not `pub mod mount_points;`. Only `PyCoordinator` itself is `pub(crate)`.

## Appendix C: Recovering from Common Mistakes

**"I moved a method but forgot to remove it from mod.rs"**
→ You'll get: `error[E0592]: duplicate definitions with name 'mount'`
→ Fix: Delete the method from `mod.rs`. It should only exist in one place.

**"I have the wrong import path"**
→ You'll get: `error[E0433]: failed to resolve: could not find 'helpers' in the crate root`
→ Fix: Run `grep -rn "pub.*fn wrap_future_as_coroutine" bindings/python/src/` to find where the function actually lives. Use that path.

**"cargo check passes but maturin develop fails"**
→ Likely a PyO3 codegen issue. Run `cargo clean -p amplifier-core-py && maturin develop` to force a clean rebuild.

**"Python tests fail with AttributeError: 'RustCoordinator' has no attribute 'mount'"**
→ The method was deleted from `mod.rs` but NOT added to `mount_points.rs` (or the `#[pymethods]` attribute is missing from the `impl` block in the new file).
→ Fix: Verify `mount_points.rs` has `#[pymethods]` on its `impl PyCoordinator` block and that the method is inside it.

**"Field access error: field 'inner' of struct 'PyCoordinator' is private"**
→ Fix: Go to `mod.rs` and change the field to `pub(crate) inner: ...`
