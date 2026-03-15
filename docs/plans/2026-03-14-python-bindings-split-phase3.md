# Phase 3: Thin the Python Layer — Implementation Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Move all behavioral logic from `_rust_wrappers.py` into the Rust binding layer, then delete `_rust_wrappers.py` entirely — making `RustCoordinator` a drop-in replacement for `ModuleCoordinator`.

**Architecture:** Add a `PyContextManagerBridge` (the one missing bridge), implement `process_hook_result()` as a `#[pymethods]` async method on `PyCoordinator` in `coordinator/hook_dispatch.rs`, upgrade `cleanup()` in `coordinator/mod.rs` with fatal-exception re-raise logic, then patch Python imports and delete the wrapper file. All changes are internal — the external API is unchanged.

**Tech Stack:** Rust (PyO3 0.28, pyo3-async-runtimes, tokio), Python (Pydantic, asyncio, pytest)

**Prerequisites:** Phase 1 (file split) and Phase 2 (coordinator decomposition) are complete. After those phases, the source layout is:

```
bindings/python/src/
  lib.rs                        # mod declarations + #[pymodule]
  helpers.rs                    # wrap_future_as_coroutine, try_model_dump
  bridges.rs                    # PyHookHandlerBridge, PyApprovalProviderBridge, PyDisplayServiceBridge
  session.rs                    # PySession
  hooks.rs                      # PyUnregisterFn, PyHookRegistry
  cancellation.rs               # PyCancellationToken
  coordinator/
    mod.rs                      # PyCoordinator struct + lifecycle
    mount_points.rs             # mount/get/unmount
    capabilities.rs             # register_capability, contributions
  errors.rs                     # PyProviderError
  retry.rs                      # PyRetryConfig
  module_resolver.rs            # resolve_module
  wasm.rs                       # PyWasm* structs
```

**Verification commands** (run after every task):

```bash
cd amplifier-core && cargo check -p amplifier-core-py
cd amplifier-core && cargo clippy -p amplifier-core-py -- -W clippy::all
```

**Full integration test** (run after Tasks 8-9, and at the end):

```bash
cd amplifier-core && maturin develop && uv run pytest tests/ -q --tb=short -m "not slow"
```

---

## Task 1: Add `PyContextManagerBridge` to `bridges.rs`

**Files:**
- Modify: `bindings/python/src/bridges.rs`

This is the one missing bridge. It follows the exact same pattern as `PyApprovalProviderBridge` and `PyDisplayServiceBridge` that already exist in the same file. It wraps a Python context manager object (which has an `add_message(message_dict)` method) so Rust code can call it.

**Step 1: Add the bridge struct and implementation**

Open `bindings/python/src/bridges.rs`. At the bottom, after the `PyDisplayServiceBridge` implementation, add:

```rust
// ---------------------------------------------------------------------------
// PyContextManagerBridge — wraps a Python ContextManager for Rust-side calls
// ---------------------------------------------------------------------------

/// Bridges a Python `ContextManager` object so Rust code can call `add_message()`.
///
/// The Python `ContextManager` protocol has:
///   `add_message(message: dict) -> None`  (may be sync or async)
///
/// This bridge is used by `process_hook_result()` in `coordinator/hook_dispatch.rs`
/// to inject context from hook results into the conversation.
pub(crate) struct PyContextManagerBridge {
    pub(crate) py_obj: Py<PyAny>,
}

// Safety: Py<PyAny> is Send+Sync (PyO3 handles GIL acquisition).
unsafe impl Send for PyContextManagerBridge {}
unsafe impl Sync for PyContextManagerBridge {}

impl PyContextManagerBridge {
    /// Call the Python context manager's `add_message(message)` method.
    ///
    /// `message` is a Python dict with keys: role, content, metadata.
    /// Handles both sync and async `add_message` implementations.
    pub(crate) async fn add_message(&self, message: Py<PyAny>) -> Result<(), PyErr> {
        // Step 1: Call add_message (inside GIL), check if result is a coroutine
        let (is_coro, call_result) =
            Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
                let result =
                    self.py_obj
                        .call_method(py, "add_message", (&message,), None)?;
                let bound = result.bind(py);

                let inspect = py.import("inspect")?;
                let is_coro: bool =
                    inspect.call_method1("iscoroutine", (bound,))?.extract()?;

                Ok((is_coro, result))
            })
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Failed to attach to Python runtime for context manager bridge",
                )
            })??;

        // Step 2: If coroutine, await it outside the GIL
        if is_coro {
            let future = Python::try_attach(|py| {
                pyo3_async_runtimes::tokio::into_future(call_result.into_bound(py))
            })
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Failed to attach for context manager coroutine conversion",
                )
            })??;

            future.await?;
        }

        Ok(())
    }
}
```

**Step 2: Verify it compiles**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py
```
Expected: compiles with no errors. You may see a warning about `PyContextManagerBridge` being unused — that's fine, Task 3 will use it.

**Step 3: Commit**

```bash
cd amplifier-core && git add bindings/python/src/bridges.rs && git commit -m "feat: add PyContextManagerBridge to bridges.rs"
```

---

## Task 2: Write tests for `PyContextManagerBridge`

**Files:**
- Create: `tests/test_context_manager_bridge.py`

This follows the exact pattern of the existing `tests/test_approval_provider_bridge.py` and `tests/test_display_service_bridge.py`. Those tests create a `RustCoordinator`, mount a fake Python object, and verify the bridge is reflected in `to_dict()`.

For the context manager bridge, we don't have a `to_dict()` field yet (we'll add that in Task 3 alongside the `process_hook_result` work). So this test focuses on verifying that `process_hook_result` with `action="inject_context"` actually calls the context manager's `add_message`. We write this test now so it **fails** — confirming `process_hook_result` doesn't exist on `RustCoordinator` yet.

**Step 1: Create the test file**

Create `tests/test_context_manager_bridge.py`:

```python
"""Tests for PyContextManagerBridge — verifies that process_hook_result
correctly routes inject_context actions to the mounted context manager."""

import pytest
from amplifier_core.models import HookResult


@pytest.mark.asyncio
async def test_inject_context_calls_add_message():
    """process_hook_result with inject_context should call context.add_message."""
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = RustCoordinator()

    class FakeContext:
        def __init__(self):
            self.messages = []

        async def add_message(self, message):
            self.messages.append(message)

    ctx = FakeContext()
    coord.mount("context", ctx)

    result = HookResult(
        action="inject_context",
        context_injection="Linter found error on line 42",
        context_injection_role="system",
    )

    processed = await coord.process_hook_result(result, "tool:end", "lint-hook")

    # Context manager should have received exactly one message
    assert len(ctx.messages) == 1
    msg = ctx.messages[0]
    assert msg["role"] == "system"
    assert "Linter found error on line 42" in msg["content"]
    assert msg["metadata"]["hook_name"] == "lint-hook"
    assert msg["metadata"]["event"] == "tool:end"
    assert msg["metadata"]["source"] == "hook"
    assert "timestamp" in msg["metadata"]


@pytest.mark.asyncio
async def test_inject_context_sync_add_message():
    """process_hook_result should handle sync add_message too."""
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = RustCoordinator()

    class SyncContext:
        def __init__(self):
            self.messages = []

        def add_message(self, message):
            self.messages.append(message)

    ctx = SyncContext()
    coord.mount("context", ctx)

    result = HookResult(
        action="inject_context",
        context_injection="Hello from sync",
    )

    await coord.process_hook_result(result, "tool:end", "sync-hook")
    assert len(ctx.messages) == 1


@pytest.mark.asyncio
async def test_inject_context_ephemeral_skips_add_message():
    """Ephemeral injections should NOT call add_message but still count tokens."""
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = RustCoordinator()

    class FakeContext:
        def __init__(self):
            self.messages = []

        async def add_message(self, message):
            self.messages.append(message)

    ctx = FakeContext()
    coord.mount("context", ctx)

    result = HookResult(
        action="inject_context",
        context_injection="Ephemeral content",
        ephemeral=True,
    )

    await coord.process_hook_result(result, "tool:end", "eph-hook")

    # add_message should NOT have been called
    assert len(ctx.messages) == 0
    # But token counter should have increased (len("Ephemeral content") // 4 = 4)
    assert coord._current_turn_injections > 0
```

**Step 2: Run the test to verify it fails**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/test_context_manager_bridge.py -v
```
Expected: FAIL — `RustCoordinator` has no `process_hook_result` method yet. That's correct.

**Step 3: Commit the failing test**

```bash
cd amplifier-core && git add tests/test_context_manager_bridge.py && git commit -m "test: add failing tests for PyContextManagerBridge via process_hook_result"
```

---

## Task 3: Implement `process_hook_result()` on `PyCoordinator`

**Files:**
- Create: `bindings/python/src/coordinator/hook_dispatch.rs`
- Modify: `bindings/python/src/coordinator/mod.rs` (add `mod hook_dispatch;`)

This is the big one. You are translating the Python logic from `_rust_wrappers.py` lines 61-247 into a Rust `#[pymethods]` block on `PyCoordinator`. The logic MUST be an exact behavioral match. Read `python/amplifier_core/_rust_wrappers.py` carefully before writing.

**Critical semantics to preserve:**
1. `ask_user` path **returns early** with a new `HookResult` — it short-circuits steps 3 and 4
2. `user_message` fires on `result.user_message` **field** being truthy, NOT on the action field
3. Size limit violation is a **hard error** (raises `ValueError`)
4. Budget overage is a **soft warning** (logs, but continues)
5. Context injection is **skipped** if `result.ephemeral` is `True`, but token counting still happens
6. `_handle_user_message` is **synchronous** (no await)
7. Token estimate: `len(content) // 4` (rough 4-chars-per-token heuristic)

**Step 1: Add the `hook_dispatch` module declaration**

In `bindings/python/src/coordinator/mod.rs`, find where the other sub-module declarations are (e.g., `mod mount_points;`, `mod capabilities;`). Add:

```rust
mod hook_dispatch;
```

**Step 2: Create `hook_dispatch.rs`**

Create `bindings/python/src/coordinator/hook_dispatch.rs`:

```rust
//! Hook result dispatch — routes HookResult actions to appropriate subsystems.
//!
//! This is the Rust equivalent of `_rust_wrappers.py`'s `process_hook_result()`,
//! `_handle_context_injection()`, `_handle_approval_request()`, and
//! `_handle_user_message()`. Moving this logic into Rust eliminates the need
//! for the Python wrapper subclass entirely.

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::bridges::PyContextManagerBridge;
use crate::helpers::wrap_future_as_coroutine;

use super::PyCoordinator;

#[pymethods]
impl PyCoordinator {
    /// Process a HookResult and route actions to appropriate subsystems.
    ///
    /// This is the Rust replacement for `_rust_wrappers.ModuleCoordinator.process_hook_result()`.
    ///
    /// Handles:
    /// 1. `inject_context` action → validate size/budget, call context.add_message()
    /// 2. `ask_user` action → call approval_system.request_approval() (RETURNS EARLY)
    /// 3. `user_message` field (truthy) → call display_system.show_message()
    /// 4. `suppress_output` → log only
    ///
    /// Args:
    ///     result: HookResult (Pydantic model) from hook execution
    ///     event: Event name that triggered the hook
    ///     hook_name: Name of the hook for logging/audit
    ///
    /// Returns:
    ///     Processed HookResult (may be replaced by approval flow)
    #[pyo3(signature = (result, event, hook_name="unknown"))]
    fn process_hook_result<'py>(
        &mut self,
        py: Python<'py>,
        result: Bound<'py, PyAny>,
        event: String,
        hook_name: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Snapshot all fields we need from the Python HookResult object while
        // we hold the GIL. This avoids repeated GIL reacquisition in the async block.
        let action: String = result.getattr("action")?.extract()?;
        let context_injection: Option<String> = {
            let val = result.getattr("context_injection")?;
            if val.is_none() { None } else { Some(val.extract()?) }
        };
        let context_injection_role: String =
            result.getattr("context_injection_role")?.extract()?;
        let ephemeral: bool = result.getattr("ephemeral")?.extract()?;
        let suppress_output: bool = result.getattr("suppress_output")?.extract()?;
        let user_message: Option<String> = {
            let val = result.getattr("user_message")?;
            if val.is_none() { None } else { Some(val.extract()?) }
        };
        let user_message_level: String =
            result.getattr("user_message_level")?.extract()?;
        let user_message_source: Option<String> = {
            let val = result.getattr("user_message_source")?;
            if val.is_none() { None } else { Some(val.extract()?) }
        };
        let approval_prompt: Option<String> = {
            let val = result.getattr("approval_prompt")?;
            if val.is_none() { None } else { Some(val.extract()?) }
        };
        let approval_options: Option<Vec<String>> = {
            let val = result.getattr("approval_options")?;
            if val.is_none() { None } else { Some(val.extract()?) }
        };
        let approval_timeout: f64 = result.getattr("approval_timeout")?.extract()?;
        let approval_default: String =
            result.getattr("approval_default")?.extract()?;

        // Read coordinator state we'll need in the async block
        let size_limit: Option<usize> = {
            let val = self.injection_size_limit_raw(py)?;
            if val.bind(py).is_none() { None } else { Some(val.extract(py)?) }
        };
        let budget: Option<usize> = {
            let val = self.injection_budget_raw(py)?;
            if val.bind(py).is_none() { None } else { Some(val.extract(py)?) }
        };
        let current_injections = self.current_turn_injections;

        // Get context manager from mount_points (for inject_context)
        let context_obj: Py<PyAny> = {
            let mp = self.mount_points.bind(py);
            let ctx = mp.get_item("context")?;
            match ctx {
                Some(c) if !c.is_none() => c.unbind(),
                _ => py.None(),
            }
        };

        // Get approval system (for ask_user)
        let approval_obj = self.approval_system_obj.clone_ref(py);

        // Get display system (for user_message)
        let display_obj = self.display_system_obj.clone_ref(py);

        // Keep the original result to return (for non-ask_user paths)
        let result_py = result.unbind();

        // HookResult class for constructing new results
        let hook_result_cls: Py<PyAny> = {
            let models = py.import("amplifier_core.models")?;
            models.getattr("HookResult")?.unbind()
        };

        // ApprovalTimeoutError for catching timeouts
        let timeout_err_cls: Py<PyAny> = {
            let approval_mod = py.import("amplifier_core.approval")?;
            approval_mod.getattr("ApprovalTimeoutError")?.unbind()
        };

        let hook_name_owned = hook_name.to_string();

        // We need a &mut self reference for updating current_turn_injections.
        // Capture it as a raw pointer that we update inside the async block.
        // This is safe because PyO3 guarantees we hold the GIL when re-attaching.
        let turn_injections_ptr = &mut self.current_turn_injections as *mut usize;

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // -------------------------------------------------------
                // 1. Handle context injection
                // -------------------------------------------------------
                if action == "inject_context" {
                    if let Some(ref content) = context_injection {
                        if !content.is_empty() {
                            // 1a. Validate size limit (HARD ERROR)
                            if let Some(limit) = size_limit {
                                if content.len() > limit {
                                    log::error!(
                                        "Hook injection too large: {} (size={}, limit={})",
                                        hook_name_owned,
                                        content.len(),
                                        limit
                                    );
                                    return Err(PyErr::new::<PyValueError, _>(format!(
                                        "Context injection exceeds {} bytes",
                                        limit
                                    )));
                                }
                            }

                            // 1b. Check budget (SOFT WARNING — log but continue)
                            let tokens = content.len() / 4; // rough 4-chars-per-token
                            if let Some(budget_val) = budget {
                                if current_injections + tokens > budget_val {
                                    log::warn!(
                                        "Warning: Hook injection budget exceeded \
                                         (hook={}, current={}, attempted={}, budget={})",
                                        hook_name_owned,
                                        current_injections,
                                        tokens,
                                        budget_val
                                    );
                                }
                            }

                            // 1c. Update turn injection counter
                            // Safety: we only write this inside Python::try_attach
                            // which holds the GIL, and PyCoordinator is not Sync.
                            Python::try_attach(|_py| {
                                unsafe { *turn_injections_ptr = current_injections + tokens; }
                            });

                            // 1d. Add to context (ONLY if not ephemeral)
                            if !ephemeral {
                                let ctx_is_valid = Python::try_attach(|py| -> bool {
                                    let bound = context_obj.bind(py);
                                    !bound.is_none()
                                        && bound.hasattr("add_message").unwrap_or(false)
                                })
                                .unwrap_or(false);

                                if ctx_is_valid {
                                    // Build the message dict
                                    let message_py: Py<PyAny> =
                                        Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                                            let datetime = py.import("datetime")?;
                                            let now = datetime
                                                .getattr("datetime")?
                                                .call_method0("now")?
                                                .call_method0("isoformat")?;

                                            let metadata = PyDict::new(py);
                                            metadata.set_item("source", "hook")?;
                                            metadata.set_item(
                                                "hook_name",
                                                &hook_name_owned,
                                            )?;
                                            metadata.set_item("event", &event)?;
                                            metadata.set_item("timestamp", &now)?;

                                            let msg = PyDict::new(py);
                                            msg.set_item("role", &context_injection_role)?;
                                            msg.set_item("content", content)?;
                                            msg.set_item("metadata", metadata)?;

                                            Ok(msg.into_any().unbind())
                                        })
                                        .ok_or_else(|| {
                                            PyErr::new::<PyRuntimeError, _>(
                                                "Failed to attach to Python runtime \
                                                 for message construction",
                                            )
                                        })??;

                                    // Call add_message via the bridge
                                    let bridge = PyContextManagerBridge {
                                        py_obj: context_obj.clone(),
                                    };
                                    bridge.add_message(message_py).await?;
                                }
                            }

                            // 1e. Audit log (always, even if ephemeral)
                            let tokens = content.len() / 4;
                            log::info!(
                                "Hook context injection \
                                 (hook={}, event={}, size={}, role={}, tokens={}, ephemeral={})",
                                hook_name_owned,
                                event,
                                content.len(),
                                context_injection_role,
                                tokens,
                                ephemeral
                            );
                        }
                    }
                }

                // -------------------------------------------------------
                // 2. Handle approval request (RETURNS EARLY)
                // -------------------------------------------------------
                if action == "ask_user" {
                    let prompt = approval_prompt
                        .unwrap_or_else(|| "Allow this operation?".to_string());
                    let options = approval_options
                        .unwrap_or_else(|| vec!["Allow".to_string(), "Deny".to_string()]);

                    log::info!(
                        "Approval requested (hook={}, prompt={}, timeout={}, default={})",
                        hook_name_owned,
                        prompt,
                        approval_timeout,
                        approval_default
                    );

                    // Check if approval system is available
                    let has_approval = Python::try_attach(|py| -> bool {
                        !approval_obj.bind(py).is_none()
                    })
                    .unwrap_or(false);

                    if !has_approval {
                        log::error!(
                            "Approval requested but no approval system provided (hook={})",
                            hook_name_owned
                        );
                        let deny_result: Py<PyAny> =
                            Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                                let kwargs = PyDict::new(py);
                                kwargs.set_item("action", "deny")?;
                                kwargs.set_item(
                                    "reason",
                                    "No approval system available",
                                )?;
                                let r = hook_result_cls.call(py, (), Some(&kwargs))?;
                                Ok(r)
                            })
                            .ok_or_else(|| {
                                PyErr::new::<PyRuntimeError, _>(
                                    "Failed to create deny HookResult",
                                )
                            })??;
                        return Ok(deny_result);
                    }

                    // Call approval_system.request_approval(...)
                    let approval_result = Self::call_approval_system(
                        &approval_obj,
                        &prompt,
                        &options,
                        approval_timeout,
                        &approval_default,
                        &timeout_err_cls,
                    )
                    .await;

                    match approval_result {
                        Ok(decision) => {
                            log::info!(
                                "Approval decision (hook={}, decision={})",
                                hook_name_owned,
                                decision
                            );

                            let new_result: Py<PyAny> = if decision == "Deny" {
                                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                                    let kwargs = PyDict::new(py);
                                    kwargs.set_item("action", "deny")?;
                                    kwargs.set_item(
                                        "reason",
                                        format!("User denied: {}", prompt),
                                    )?;
                                    hook_result_cls.call(py, (), Some(&kwargs))
                                })
                                .ok_or_else(|| {
                                    PyErr::new::<PyRuntimeError, _>(
                                        "Failed to create deny result",
                                    )
                                })??
                            } else {
                                // "Allow once" or "Allow always" -> continue
                                Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                                    let kwargs = PyDict::new(py);
                                    kwargs.set_item("action", "continue")?;
                                    hook_result_cls.call(py, (), Some(&kwargs))
                                })
                                .ok_or_else(|| {
                                    PyErr::new::<PyRuntimeError, _>(
                                        "Failed to create continue result",
                                    )
                                })??
                            };
                            return Ok(new_result);
                        }
                        Err(e) => {
                            // Check if it's an ApprovalTimeoutError
                            let is_timeout = Python::try_attach(|py| -> bool {
                                e.is_instance(py, timeout_err_cls.bind(py))
                            })
                            .unwrap_or(false);

                            if is_timeout {
                                log::warn!(
                                    "Approval timeout (hook={}, default={})",
                                    hook_name_owned,
                                    approval_default
                                );

                                let timeout_result: Py<PyAny> =
                                    if approval_default == "deny" {
                                        Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                                            let kwargs = PyDict::new(py);
                                            kwargs.set_item("action", "deny")?;
                                            kwargs.set_item(
                                                "reason",
                                                format!(
                                                    "Approval timeout - denied by default: {}",
                                                    prompt
                                                ),
                                            )?;
                                            hook_result_cls.call(py, (), Some(&kwargs))
                                        })
                                        .ok_or_else(|| {
                                            PyErr::new::<PyRuntimeError, _>(
                                                "Failed to create timeout deny result",
                                            )
                                        })??
                                    } else {
                                        Python::try_attach(|py| -> PyResult<Py<PyAny>> {
                                            let kwargs = PyDict::new(py);
                                            kwargs.set_item("action", "continue")?;
                                            hook_result_cls.call(py, (), Some(&kwargs))
                                        })
                                        .ok_or_else(|| {
                                            PyErr::new::<PyRuntimeError, _>(
                                                "Failed to create timeout continue result",
                                            )
                                        })??
                                    };
                                return Ok(timeout_result);
                            }

                            // Not a timeout — re-raise
                            return Err(e);
                        }
                    }
                }

                // -------------------------------------------------------
                // 3. Handle user message (fires on truthy field, NOT action)
                // -------------------------------------------------------
                if let Some(ref msg_text) = user_message {
                    if !msg_text.is_empty() {
                        let source_name = user_message_source
                            .as_deref()
                            .unwrap_or(&hook_name_owned);

                        let has_display = Python::try_attach(|py| -> bool {
                            !display_obj.bind(py).is_none()
                        })
                        .unwrap_or(false);

                        if !has_display {
                            log::info!(
                                "Hook message ({}): {} (hook={})",
                                user_message_level,
                                msg_text,
                                source_name
                            );
                        } else {
                            // Synchronous call — _handle_user_message is sync in Python
                            Python::try_attach(|py| -> PyResult<()> {
                                let source_str = format!("hook:{}", source_name);
                                display_obj.call_method(
                                    py,
                                    "show_message",
                                    (msg_text, &user_message_level, &source_str),
                                    None,
                                )?;
                                Ok(())
                            })
                            .unwrap_or(Some(Ok(())))
                            .unwrap_or_else(|e| {
                                log::error!("Error calling display_system: {e}");
                            });
                        }
                    }
                }

                // -------------------------------------------------------
                // 4. Output suppression (just log)
                // -------------------------------------------------------
                if suppress_output {
                    log::debug!(
                        "Hook '{}' requested output suppression",
                        hook_name_owned
                    );
                }

                // Return original result unchanged
                Ok(result_py)
            }),
        )
    }
}

impl PyCoordinator {
    /// Call the Python approval system's request_approval method.
    ///
    /// Handles both sync and async implementations.
    /// Returns Ok(decision_string) or Err(PyErr).
    async fn call_approval_system(
        approval_obj: &Py<PyAny>,
        prompt: &str,
        options: &[String],
        timeout: f64,
        default: &str,
        _timeout_err_cls: &Py<PyAny>,
    ) -> Result<String, PyErr> {
        let prompt = prompt.to_string();
        let options: Vec<String> = options.to_vec();
        let default = default.to_string();
        let approval = approval_obj.clone();

        // Call request_approval (may return coroutine)
        let (is_coro, call_result) =
            Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
                let opts_list = pyo3::types::PyList::new(
                    py,
                    options.iter().map(|s| s.as_str()),
                )?;
                let result = approval.call_method(
                    py,
                    "request_approval",
                    (&prompt, opts_list, timeout, &default),
                    None,
                )?;
                let bound = result.bind(py);
                let inspect = py.import("inspect")?;
                let is_coro: bool =
                    inspect.call_method1("iscoroutine", (bound,))?.extract()?;
                Ok((is_coro, result))
            })
            .ok_or_else(|| {
                PyErr::new::<PyRuntimeError, _>(
                    "Failed to attach to Python runtime for approval call",
                )
            })??;

        // Await if coroutine
        let py_result = if is_coro {
            let future = Python::try_attach(|py| {
                pyo3_async_runtimes::tokio::into_future(call_result.into_bound(py))
            })
            .ok_or_else(|| {
                PyErr::new::<PyRuntimeError, _>(
                    "Failed to convert approval coroutine",
                )
            })??;
            future.await?
        } else {
            call_result
        };

        // Extract string result
        let decision: String = Python::try_attach(|py| py_result.extract(py))
            .ok_or_else(|| {
                PyErr::new::<PyRuntimeError, _>(
                    "Failed to extract approval decision",
                )
            })??;

        Ok(decision)
    }
}
```

**Step 3: Add helper methods to `coordinator/mod.rs`**

The `hook_dispatch.rs` code calls `self.injection_size_limit_raw(py)` and `self.injection_budget_raw(py)`. These are internal helpers that return `Py<PyAny>` (not the `#[getter]` versions). Add these to the `impl PyCoordinator` block in `coordinator/mod.rs`:

```rust
    /// Internal: get raw injection_size_limit as Py<PyAny> (None or int).
    /// Used by hook_dispatch to read the value without going through the Python getter.
    pub(crate) fn injection_size_limit_raw(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config_dict.bind(py);
        let session = config.call_method1("get", ("session",))?;
        if session.is_none() {
            return Ok(py.None());
        }
        let val = session.call_method1("get", ("injection_size_limit",))?;
        if val.is_none() {
            Ok(py.None())
        } else {
            Ok(val.unbind())
        }
    }

    /// Internal: get raw injection_budget_per_turn as Py<PyAny> (None or int).
    pub(crate) fn injection_budget_raw(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config_dict.bind(py);
        let session = config.call_method1("get", ("session",))?;
        if session.is_none() {
            return Ok(py.None());
        }
        let val = session.call_method1("get", ("injection_budget_per_turn",))?;
        if val.is_none() {
            Ok(py.None())
        } else {
            Ok(val.unbind())
        }
    }
```

Also ensure the `PyCoordinator` struct fields that `hook_dispatch.rs` accesses are `pub(crate)`:
- `current_turn_injections`
- `mount_points`
- `approval_system_obj`
- `display_system_obj`
- `config_dict`

These should already be `pub(crate)` from Phase 2, but verify.

**Step 4: Verify it compiles**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py
```
Expected: compiles with no errors. Fix any issues before proceeding.

**Step 5: Run the Task 2 tests to verify they pass**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/test_context_manager_bridge.py -v
```
Expected: all 3 tests PASS.

**Step 6: Commit**

```bash
cd amplifier-core && git add bindings/python/src/coordinator/ && git commit -m "feat: implement process_hook_result() in Rust hook_dispatch.rs"
```

---

## Task 4: Write tests for `process_hook_result()` action branches

**Files:**
- Create: `tests/test_process_hook_result.py`

These tests cover every branch of `process_hook_result`. Write them all in one file since they're testing different facets of the same method.

**Step 1: Create the test file**

Create `tests/test_process_hook_result.py`:

```python
"""Tests for process_hook_result on RustCoordinator — covers all action branches."""

import types

import pytest
from amplifier_core.models import HookResult


def _make_coordinator(
    *,
    injection_size_limit=None,
    injection_budget_per_turn=None,
):
    """Create a RustCoordinator with optional session config."""
    from amplifier_core._engine import RustCoordinator

    session_config = {}
    if injection_size_limit is not None:
        session_config["injection_size_limit"] = injection_size_limit
    if injection_budget_per_turn is not None:
        session_config["injection_budget_per_turn"] = injection_budget_per_turn

    fake_session = types.SimpleNamespace(
        session_id="test-session",
        parent_id=None,
        config={"session": session_config} if session_config else {},
    )
    return RustCoordinator(session=fake_session)


# --- inject_context tests ---


@pytest.mark.asyncio
async def test_inject_context_size_limit_exceeded():
    """Exceeding injection_size_limit should raise ValueError."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator(injection_size_limit=10)

    class FakeCtx:
        async def add_message(self, msg):
            pass

    coord.mount("context", FakeCtx())
    result = HookResult(action="inject_context", context_injection="x" * 20)

    with pytest.raises(ValueError, match="exceeds 10 bytes"):
        await coord.process_hook_result(result, "tool:end", "big-hook")


@pytest.mark.asyncio
async def test_inject_context_budget_exceeded_logs_warning(caplog):
    """Exceeding injection budget should log a warning but continue."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator(injection_budget_per_turn=1)

    class FakeCtx:
        def __init__(self):
            self.messages = []

        async def add_message(self, msg):
            self.messages.append(msg)

    ctx = FakeCtx()
    coord.mount("context", ctx)

    # 40 chars = 10 tokens, budget is 1 → exceeds
    result = HookResult(action="inject_context", context_injection="x" * 40)

    # Should NOT raise — budget is soft warning
    processed = await coord.process_hook_result(result, "tool:end", "budget-hook")

    # Message should still have been added (soft warning, not hard error)
    assert len(ctx.messages) == 1


# --- ask_user tests ---


@pytest.mark.asyncio
async def test_ask_user_approved():
    """ask_user with approval returns HookResult(action='continue')."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator()

    class FakeApproval:
        async def request_approval(self, prompt, options, timeout, default):
            return "Allow once"

    coord.approval_system = FakeApproval()

    result = HookResult(action="ask_user", approval_prompt="Allow write?")
    processed = await coord.process_hook_result(result, "tool:start", "perm-hook")
    assert processed.action == "continue"


@pytest.mark.asyncio
async def test_ask_user_denied():
    """ask_user with denial returns HookResult(action='deny')."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator()

    class FakeApproval:
        async def request_approval(self, prompt, options, timeout, default):
            return "Deny"

    coord.approval_system = FakeApproval()

    result = HookResult(action="ask_user", approval_prompt="Allow delete?")
    processed = await coord.process_hook_result(result, "tool:start", "deny-hook")
    assert processed.action == "deny"
    assert "User denied" in processed.reason


@pytest.mark.asyncio
async def test_ask_user_no_approval_system():
    """ask_user with no approval system returns deny."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator()
    # No approval system set

    result = HookResult(action="ask_user", approval_prompt="Allow?")
    processed = await coord.process_hook_result(result, "tool:start", "no-approval")
    assert processed.action == "deny"
    assert "No approval system" in processed.reason


@pytest.mark.asyncio
async def test_ask_user_timeout_deny_default():
    """ask_user with timeout and deny default returns deny."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.approval import ApprovalTimeoutError

    coord = _make_coordinator()

    class TimeoutApproval:
        async def request_approval(self, prompt, options, timeout, default):
            raise ApprovalTimeoutError("timed out")

    coord.approval_system = TimeoutApproval()

    result = HookResult(
        action="ask_user",
        approval_prompt="Allow?",
        approval_default="deny",
    )
    processed = await coord.process_hook_result(result, "tool:start", "timeout-hook")
    assert processed.action == "deny"
    assert "timeout" in processed.reason.lower()


# --- user_message tests ---


@pytest.mark.asyncio
async def test_user_message_with_display_system():
    """user_message field should call display_system.show_message."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator()

    class FakeDisplay:
        def __init__(self):
            self.messages = []

        def show_message(self, message, level, source):
            self.messages.append((message, level, source))

    display = FakeDisplay()
    coord.display_system = display

    result = HookResult(
        action="continue",
        user_message="Found 3 issues",
        user_message_level="warning",
    )
    await coord.process_hook_result(result, "tool:end", "lint-hook")

    assert len(display.messages) == 1
    msg, level, source = display.messages[0]
    assert msg == "Found 3 issues"
    assert level == "warning"
    assert "hook:lint-hook" in source


@pytest.mark.asyncio
async def test_user_message_no_display_falls_back_to_log():
    """user_message with no display system should not crash (falls back to log)."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator()
    # No display system

    result = HookResult(action="continue", user_message="Heads up!")
    # Should not raise
    await coord.process_hook_result(result, "tool:end", "quiet-hook")


# --- continue / None tests ---


@pytest.mark.asyncio
async def test_continue_returns_result_unchanged():
    """action='continue' returns the original result unchanged."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = _make_coordinator()
    result = HookResult(action="continue")
    processed = await coord.process_hook_result(result, "tool:end", "noop-hook")
    assert processed.action == "continue"
```

**Step 2: Run the tests**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/test_process_hook_result.py -v
```
Expected: all tests PASS. If any fail, fix the Rust implementation in `hook_dispatch.rs` before continuing.

**Step 3: Commit**

```bash
cd amplifier-core && git add tests/test_process_hook_result.py && git commit -m "test: add comprehensive tests for process_hook_result action branches"
```

---

## Task 5: Move `cleanup()` fatal-exception logic to `coordinator/mod.rs`

**Files:**
- Modify: `bindings/python/src/coordinator/mod.rs`

The current `cleanup()` in `coordinator/mod.rs` logs all errors but does NOT re-raise fatal ones. The Python `_rust_wrappers.py` override (lines 36-59) adds: "track the first `BaseException` that is NOT a regular `Exception`, and re-raise it after all cleanup runs." You need to add this behavior.

**Step 1: Modify `cleanup()` in `coordinator/mod.rs`**

Find the existing `cleanup()` method. It currently does `log::error!("Error during cleanup: {e}")` for every error. You need to change it so that:

1. For each cleanup function error, check if it's a "fatal" Python exception (a `BaseException` that is NOT an `Exception` — i.e., `KeyboardInterrupt`, `SystemExit`, `asyncio.CancelledError`).
2. Track the first fatal exception.
3. After all cleanups run, if there was a fatal exception, re-raise it.

In the async block where cleanup errors are caught, replace the simple `log::error!` with this pattern:

```rust
// Track the first fatal exception (BaseException that is NOT Exception).
// Fatal = KeyboardInterrupt, SystemExit, CancelledError, etc.
let mut first_fatal: Option<PyErr> = None;

// ... in each error handler:
// (where `e` is a PyErr from a cleanup function)
log::error!("Error during cleanup: {e}");
if first_fatal.is_none() {
    let is_non_fatal = Python::try_attach(|py| -> bool {
        e.is_instance_of::<pyo3::exceptions::PyException>(py)
    })
    .unwrap_or(true); // If we can't check, assume non-fatal

    if !is_non_fatal {
        // It's a BaseException but NOT Exception → fatal
        first_fatal = Some(e);
    }
}

// ... after the loop:
if let Some(fatal) = first_fatal {
    return Err(fatal);
}
Ok(())
```

The key insight: in Python's exception hierarchy, `Exception` inherits from `BaseException`. Things like `KeyboardInterrupt` and `SystemExit` inherit directly from `BaseException` but NOT from `Exception`. In PyO3, `e.is_instance_of::<pyo3::exceptions::PyException>(py)` returns `true` for regular exceptions and `false` for fatal ones.

**Step 2: Verify it compiles**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py
```
Expected: compiles.

**Step 3: Run existing tests**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/ -q --tb=short -m "not slow" -x
```
Expected: all existing tests still pass.

**Step 4: Commit**

```bash
cd amplifier-core && git add bindings/python/src/coordinator/mod.rs && git commit -m "feat: add fatal-exception re-raise to cleanup() in coordinator/mod.rs"
```

---

## Task 6: Patch `PySession::new()` to stop importing `_rust_wrappers`

**Files:**
- Modify: `bindings/python/src/session.rs`

Currently `PySession::new()` does this (around what was originally line 501-518 of `lib.rs`, now in `session.rs`):

```rust
let wrappers = py.import("amplifier_core._rust_wrappers")?;
let coord_cls = wrappers.getattr("ModuleCoordinator")?;
```

This creates the Python wrapper subclass. Now that `RustCoordinator` has `process_hook_result()` and `cleanup()` with fatal-exception logic built in, we can construct `RustCoordinator` directly.

**Step 1: Replace the coordinator construction**

Find the coordinator construction block in `session.rs`. Replace:

```rust
        // ---- Create the coordinator ----
        // Use the Python ModuleCoordinator wrapper (from _rust_wrappers.py)
        // which adds process_hook_result on top of the Rust PyCoordinator.
        // This is critical: orchestrators call coordinator.process_hook_result()
        // which only exists on the Python wrapper, not on raw RustCoordinator.
        let coord_any: Py<PyAny> = {
            let wrappers = py.import("amplifier_core._rust_wrappers")?;
            let coord_cls = wrappers.getattr("ModuleCoordinator")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("session", fake_session.clone())?;
            if let Some(ref approval) = approval_system {
                kwargs.set_item("approval_system", approval)?;
            }
            if let Some(ref display) = display_system {
                kwargs.set_item("display_system", display)?;
            }
            let coord = coord_cls.call((), Some(&kwargs))?;
            coord.unbind()
        };
```

with:

```rust
        // ---- Create the coordinator ----
        // RustCoordinator now has process_hook_result() and cleanup()
        // with fatal-exception logic built in — no Python wrapper needed.
        let coord_any: Py<PyAny> = {
            let engine = py.import("amplifier_core._engine")?;
            let coord_cls = engine.getattr("RustCoordinator")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("session", fake_session.clone())?;
            if let Some(ref approval) = approval_system {
                kwargs.set_item("approval_system", approval)?;
            }
            if let Some(ref display) = display_system {
                kwargs.set_item("display_system", display)?;
            }
            let coord = coord_cls.call((), Some(&kwargs))?;
            coord.unbind()
        };
```

**Step 2: Verify it compiles**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py
```
Expected: compiles.

**Step 3: Run tests**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/ -q --tb=short -m "not slow" -x
```
Expected: all tests pass. If any test relies on the Python `ModuleCoordinator` wrapper having specific behavior that `RustCoordinator` doesn't have yet, you'll catch it here.

**Step 4: Commit**

```bash
cd amplifier-core && git add bindings/python/src/session.rs && git commit -m "feat: construct RustCoordinator directly in PySession::new(), remove _rust_wrappers import"
```

---

## Task 7: Delete `_rust_wrappers.py`

**Files:**
- Delete: `python/amplifier_core/_rust_wrappers.py`

**Step 1: Verify nothing else imports it**

Run:
```bash
cd amplifier-core && grep -r "_rust_wrappers" python/ tests/ --include="*.py" | grep -v "__pycache__"
```
Expected output should show only:
- `python/amplifier_core/__init__.py` (line 17) — we'll fix in Task 8
- `python/amplifier_core/coordinator.py` (line 8) — we'll fix in Task 9
- `python/amplifier_core/_rust_wrappers.py` itself

If anything else imports it, fix those first.

**Step 2: Delete the file**

```bash
rm amplifier-core/python/amplifier_core/_rust_wrappers.py
```

Do NOT run tests yet — `__init__.py` and `coordinator.py` still import from it. Fix those next.

**Step 3: Commit**

```bash
cd amplifier-core && git add -u python/amplifier_core/_rust_wrappers.py && git commit -m "chore: delete _rust_wrappers.py (247 lines, logic now in Rust)"
```

---

## Task 8: Update `__init__.py`

**Files:**
- Modify: `python/amplifier_core/__init__.py`

**Step 1: Change the import**

In `python/amplifier_core/__init__.py`, find line 17:

```python
from ._rust_wrappers import ModuleCoordinator  # RustCoordinator + process_hook_result
```

Replace with:

```python
from ._engine import RustCoordinator as ModuleCoordinator  # process_hook_result now in Rust
```

**Step 2: Run import smoke test**

Run:
```bash
cd amplifier-core && maturin develop && uv run python -c "from amplifier_core import ModuleCoordinator; print(ModuleCoordinator)"
```
Expected: prints something like `<class 'amplifier_core._engine.RustCoordinator'>`. No ImportError.

**Step 3: Commit**

```bash
cd amplifier-core && git add python/amplifier_core/__init__.py && git commit -m "chore: update __init__.py to import ModuleCoordinator from _engine"
```

---

## Task 9: Update `coordinator.py` re-export stub

**Files:**
- Modify: `python/amplifier_core/coordinator.py`

**Step 1: Change the import**

In `python/amplifier_core/coordinator.py`, find line 8:

```python
from amplifier_core._rust_wrappers import ModuleCoordinator
```

Replace the entire file content with:

```python
"""Module coordinator for mount points and capabilities.

The coordinator implementation lives in the Rust kernel. This module
re-exports for backward compatibility with:
    from amplifier_core.coordinator import ModuleCoordinator
"""

from amplifier_core._engine import RustCoordinator as ModuleCoordinator

__all__ = ["ModuleCoordinator"]
```

**Step 2: Run tests**

Now that all imports are updated and `_rust_wrappers.py` is gone, run the full suite:

```bash
cd amplifier-core && maturin develop && uv run pytest tests/ -q --tb=short -m "not slow"
```
Expected: all tests pass.

**Step 3: Commit**

```bash
cd amplifier-core && git add python/amplifier_core/coordinator.py && git commit -m "chore: update coordinator.py to re-export from _engine"
```

---

## Task 10: Consolidate mount-point validation in `_session_exec.py`

**Files:**
- Modify: `python/amplifier_core/_session_exec.py`

The function `run_orchestrator()` in `_session_exec.py` currently does mount-point presence checks (lines 33-43) that are **duplicated** in Rust's `PySession::execute()`. The Rust side already validates these before calling `run_orchestrator()`. Remove the redundant checks to make the Python side thinner.

**Step 1: Thin out `run_orchestrator()`**

In `python/amplifier_core/_session_exec.py`, replace the `run_orchestrator` function:

```python
async def run_orchestrator(coordinator: Any, prompt: str) -> str:
    """Call the mounted orchestrator's execute() method.

    This is the Python boundary call. Rust handles everything else
    (initialization check, event emission, cancellation, errors,
    and mount-point validation).

    Args:
        coordinator: The coordinator with mounted modules.
        prompt: User input prompt.

    Returns:
        Final response string from the orchestrator.
    """
    # Mount-point presence is validated by Rust PySession::execute()
    # before this function is called. We just retrieve and call.
    orchestrator = coordinator.get("orchestrator")
    context = coordinator.get("context")
    providers = coordinator.get("providers") or {}
    tools = coordinator.get("tools") or {}
    hooks = coordinator.hooks

    logger.debug(f"Passing providers to orchestrator: {list(providers.keys())}")
    for name, provider in providers.items():
        logger.debug(f"  Provider '{name}': type={type(provider).__name__}")

    result = await orchestrator.execute(
        prompt=prompt,
        context=context,
        providers=providers,
        tools=tools,
        hooks=hooks,
        coordinator=coordinator,
    )

    return result
```

**Step 2: Run tests**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/ -q --tb=short -m "not slow"
```
Expected: all tests pass. The Rust-side validation catches the same errors before Python ever runs.

**Step 3: Commit**

```bash
cd amplifier-core && git add python/amplifier_core/_session_exec.py && git commit -m "chore: remove duplicated mount-point checks from _session_exec.py"
```

---

## Task 11: Write backward compatibility tests

**Files:**
- Create: `tests/test_backward_compat_phase3.py`

These tests verify that the external contract surface is intact after all the refactoring.

**Step 1: Create the test file**

Create `tests/test_backward_compat_phase3.py`:

```python
"""Backward compatibility tests — verify the external contract surface
is unchanged after Phase 3 (thinning the Python layer)."""

import types

import pytest


def test_hook_result_import_path_top_level():
    """HookResult is importable from amplifier_core."""
    from amplifier_core import HookResult

    assert HookResult is not None
    r = HookResult(action="continue")
    assert r.action == "continue"


def test_hook_result_import_path_models():
    """HookResult is importable from amplifier_core.models."""
    from amplifier_core.models import HookResult

    assert HookResult is not None


def test_hook_result_import_path_hooks():
    """HookResult is importable from amplifier_core.hooks."""
    from amplifier_core.hooks import HookResult

    assert HookResult is not None


def test_hook_result_all_paths_same_type():
    """All 3 HookResult import paths resolve to the same type."""
    from amplifier_core import HookResult as HR1
    from amplifier_core.hooks import HookResult as HR2
    from amplifier_core.models import HookResult as HR3

    assert HR1 is HR2
    assert HR2 is HR3


def test_module_coordinator_import_top_level():
    """ModuleCoordinator importable from amplifier_core."""
    from amplifier_core import ModuleCoordinator

    coord = ModuleCoordinator()
    assert hasattr(coord, "mount")
    assert hasattr(coord, "get")
    assert hasattr(coord, "hooks")
    assert hasattr(coord, "session_state")
    assert hasattr(coord, "process_hook_result")


def test_module_coordinator_import_from_coordinator_module():
    """ModuleCoordinator importable from amplifier_core.coordinator."""
    from amplifier_core.coordinator import ModuleCoordinator

    assert ModuleCoordinator is not None
    coord = ModuleCoordinator()
    assert hasattr(coord, "process_hook_result")


def test_rust_wrappers_no_longer_exists():
    """_rust_wrappers.py should be deleted."""
    with pytest.raises(ImportError):
        import amplifier_core._rust_wrappers  # noqa: F401


def test_session_config_mutability():
    """session.config['key'] = value should reflect in coordinator."""
    try:
        from amplifier_core._engine import RustSession
    except ImportError:
        pytest.skip("Rust engine not available")

    config = {
        "session": {
            "orchestrator": "test-orch",
            "context": "test-ctx",
        }
    }
    session = RustSession(config)

    # Mutate session config
    session.config["new_key"] = "new_value"

    # Verify it reflects
    assert session.config.get("new_key") == "new_value"
```

**Step 2: Run the tests**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/test_backward_compat_phase3.py -v
```
Expected: all tests PASS.

**Step 3: Commit**

```bash
cd amplifier-core && git add tests/test_backward_compat_phase3.py && git commit -m "test: add backward compatibility tests for Phase 3"
```

---

## Task 12: Final verification

**Files:** None (verification only)

**Step 1: Verify `_rust_wrappers.py` is gone**

Run:
```bash
test ! -f amplifier-core/python/amplifier_core/_rust_wrappers.py && echo "DELETED" || echo "STILL EXISTS"
```
Expected: `DELETED`

**Step 2: Verify no Python file imports `_rust_wrappers`**

Run:
```bash
cd amplifier-core && grep -r "_rust_wrappers" python/ tests/ --include="*.py" | grep -v "__pycache__"
```
Expected: zero matches (except possibly the backward compat test that verifies the ImportError).

**Step 3: Run Rust checks**

Run:
```bash
cd amplifier-core && cargo check -p amplifier-core-py && cargo clippy -p amplifier-core-py -- -W clippy::all
```
Expected: no errors, no warnings (or only pre-existing warnings unrelated to Phase 3).

**Step 4: Run the full test suite**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/ -q --tb=short -m "not slow"
```
Expected: all tests pass (517+ existing tests + the new tests from Tasks 2, 4, 11).

**Step 5: Commit final state**

If there are any uncommitted changes from test fixes:
```bash
cd amplifier-core && git add -A && git commit -m "chore: Phase 3 final verification — all tests passing"
```

---

## Summary: What Changed

| Category | Before Phase 3 | After Phase 3 |
|----------|----------------|---------------|
| `_rust_wrappers.py` | 247 lines, subclasses `RustCoordinator` | **Deleted** |
| `process_hook_result()` | Python method on wrapper | Rust `#[pymethods]` in `hook_dispatch.rs` |
| `cleanup()` fatal logic | Python override in wrapper | Rust `coordinator/mod.rs` with `PyErr` type checking |
| `PyContextManagerBridge` | Did not exist | New bridge in `bridges.rs` (~60 lines) |
| `PySession::new()` | Imports `_rust_wrappers.ModuleCoordinator` | Constructs `RustCoordinator` directly |
| `__init__.py` line 17 | `from ._rust_wrappers import ModuleCoordinator` | `from ._engine import RustCoordinator as ModuleCoordinator` |
| `coordinator.py` | Re-exports from `_rust_wrappers` | Re-exports from `_engine` |
| `_session_exec.py` | 3 redundant mount-point checks | Removed (Rust validates) |
| New test files | — | `test_context_manager_bridge.py`, `test_process_hook_result.py`, `test_backward_compat_phase3.py` |

**Net Python deletions:** ~262 lines (247 from `_rust_wrappers.py` + 15 from `_session_exec.py`)
**Net Rust additions:** ~350 lines (`hook_dispatch.rs` ~250 + `PyContextManagerBridge` ~60 + cleanup logic ~40)