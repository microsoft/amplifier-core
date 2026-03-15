//! Hook result dispatch — routes HookResult actions to appropriate subsystems.
//!
//! This is the Rust equivalent of `_rust_wrappers.py`'s `process_hook_result()`,
//! `_handle_context_injection()`, `_handle_approval_request()`, and
//! `_handle_user_message()`. Moving this logic into Rust eliminates the need
//! for the Python wrapper subclass entirely.
//!
//! ## Design: sync-before-async
//!
//! All state mutations (`current_turn_injections`, audit logging) happen
//! **synchronously** in the function body before the async block.  Only
//! I/O calls (`add_message`, `request_approval`) remain in the async block.
//! This avoids capturing raw pointers in a `Send + 'static` future.

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::bridges::PyContextManagerBridge;
use crate::helpers::{is_approval_granted, wrap_future_as_coroutine};

use super::PyCoordinator;

#[pymethods]
impl PyCoordinator {
    /// Process a HookResult and route actions to appropriate subsystems.
    ///
    /// This is the Rust replacement for `_rust_wrappers.ModuleCoordinator.process_hook_result()`.
    ///
    /// Handles:
    /// 1. `inject_context` action → validate size/budget (sync), call context.add_message() (async)
    /// 2. `ask_user` action → call approval_system.request_approval() (RETURNS EARLY)
    /// 3. `user_message` field (truthy) → call display_system.show_message() (sync)
    /// 4. `suppress_output` → log only (sync)
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
        // we hold the GIL.
        let action: String = result.getattr("action")?.extract()?;
        let context_injection: Option<String> = {
            let val = result.getattr("context_injection")?;
            if val.is_none() {
                None
            } else {
                Some(val.extract()?)
            }
        };
        let context_injection_role: String = result.getattr("context_injection_role")?.extract()?;
        let ephemeral: bool = result.getattr("ephemeral")?.extract()?;
        let suppress_output: bool = result.getattr("suppress_output")?.extract()?;
        let user_message: Option<String> = {
            let val = result.getattr("user_message")?;
            if val.is_none() {
                None
            } else {
                Some(val.extract()?)
            }
        };
        let user_message_level: String = result.getattr("user_message_level")?.extract()?;
        let user_message_source: Option<String> = {
            let val = result.getattr("user_message_source")?;
            if val.is_none() {
                None
            } else {
                Some(val.extract()?)
            }
        };
        let approval_prompt: Option<String> = {
            let val = result.getattr("approval_prompt")?;
            if val.is_none() {
                None
            } else {
                Some(val.extract()?)
            }
        };
        let approval_options: Option<Vec<String>> = {
            let val = result.getattr("approval_options")?;
            if val.is_none() {
                None
            } else {
                Some(val.extract()?)
            }
        };
        let approval_timeout: f64 = result.getattr("approval_timeout")?.extract()?;
        let approval_default: String = result.getattr("approval_default")?.extract()?;

        // Read coordinator config
        let size_limit: Option<usize> = {
            let val = self.injection_size_limit_raw(py)?;
            if val.bind(py).is_none() {
                None
            } else {
                Some(val.extract(py)?)
            }
        };
        let budget: Option<usize> = {
            let val = self.injection_budget_raw(py)?;
            if val.bind(py).is_none() {
                None
            } else {
                Some(val.extract(py)?)
            }
        };

        let hook_name_owned = hook_name.to_string();

        // -----------------------------------------------------------------------
        // SYNCHRONOUS: Section 1 — context injection validation + state update
        //
        // All state mutations happen here (before the async block) so we never
        // need to capture mutable references in a Send + 'static future.
        // -----------------------------------------------------------------------
        //
        // `message_to_inject` is the pre-built message dict to pass into the
        // async block.  It is Some(_) when we should call add_message, None
        // when the injection is ephemeral or has no context to inject.
        let message_to_inject: Option<Py<PyAny>> = if action == "inject_context" {
            match context_injection.as_deref() {
                Some(content) if !content.is_empty() => {
                    // 1a. Validate size limit (HARD ERROR — raises ValueError)
                    //
                    // Use char count (Unicode scalar values) not byte count so
                    // non-ASCII content (CJK, emoji, etc.) is measured the same
                    // way Python's len() measures strings.
                    let char_count = content.chars().count();
                    if let Some(limit) = size_limit {
                        if char_count > limit {
                            log::error!(
                                "Hook injection too large: {} (chars={}, limit={})",
                                hook_name_owned,
                                char_count,
                                limit
                            );
                            return Err(PyErr::new::<PyValueError, _>(format!(
                                "Context injection exceeds {} characters",
                                limit
                            )));
                        }
                    }

                    // 1b. Check budget (SOFT WARNING — log but continue)
                    //
                    // Token estimate uses char count (matches Python len() semantics).
                    const CHARS_PER_TOKEN_ESTIMATE: usize = 4;
                    let tokens = char_count / CHARS_PER_TOKEN_ESTIMATE;
                    if let Some(budget_val) = budget {
                        if self.current_turn_injections + tokens > budget_val {
                            log::warn!(
                                "Warning: Hook injection budget exceeded \
                                     (hook={}, current={}, attempted={}, budget={})",
                                hook_name_owned,
                                self.current_turn_injections,
                                tokens,
                                budget_val
                            );
                        }
                    }

                    // 1c. Update turn injection counter (synchronous, no async needed)
                    self.current_turn_injections += tokens;

                    // 1d. Sanitize content before injecting (mirrors ToolResult._sanitize_for_llm).
                    // Falls back to raw content with a warning if the Python function is unavailable.
                    let sanitized_content: String = match py
                        .import("amplifier_core.models")
                        .and_then(|m| m.getattr("_sanitize_for_llm"))
                        .and_then(|f| f.call1((content,)))
                        .and_then(|r| r.extract::<String>())
                    {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!(
                                "SECURITY: Sanitization unavailable for hook '{}' — rejecting injection: {e}",
                                hook_name_owned
                            );
                            return Err(pyo3::exceptions::PyValueError::new_err(
                                "Context injection rejected: content sanitization function unavailable"
                            ));
                        }
                    };

                    // 1e. Build message dict for async injection (ONLY if not ephemeral)
                    let msg_opt = if !ephemeral {
                        let ctx_bound = self.mount_points.bind(py);
                        let ctx_item = ctx_bound.get_item("context")?;
                        let has_context = ctx_item.as_ref().is_some_and(|c| {
                            !c.is_none() && c.hasattr("add_message").unwrap_or(false)
                        });

                        if has_context {
                            let datetime = py.import("datetime")?;
                            let now = datetime
                                .getattr("datetime")?
                                .call_method0("now")?
                                .call_method0("isoformat")?;

                            let metadata = PyDict::new(py);
                            metadata.set_item("source", "hook")?;
                            metadata.set_item("hook_name", &hook_name_owned)?;
                            metadata.set_item("event", &event)?;
                            metadata.set_item("timestamp", &now)?;

                            let msg = PyDict::new(py);
                            msg.set_item("role", &context_injection_role)?;
                            msg.set_item("content", &sanitized_content)?;
                            msg.set_item("metadata", metadata)?;

                            Some(msg.into_any().unbind())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // 1f. Audit log (always, even if ephemeral).
                    // Reports char count to match Python len() semantics.
                    log::info!(
                        "Hook context injection \
                             (hook={}, event={}, chars={}, role={}, tokens={}, ephemeral={})",
                        hook_name_owned,
                        event,
                        char_count,
                        context_injection_role,
                        tokens,
                        ephemeral
                    );

                    msg_opt
                }
                _ => None,
            }
        } else {
            None
        };

        // -----------------------------------------------------------------------
        // SYNCHRONOUS: Section 3 — user_message (synchronous show_message call)
        //
        // Fires on result.user_message FIELD being truthy, NOT on action field.
        // Done synchronously to avoid capturing display_system_obj in the future.
        // -----------------------------------------------------------------------
        if action != "ask_user" {
            if let Some(ref msg_text) = user_message {
                if !msg_text.is_empty() {
                    let source_name = user_message_source.as_deref().unwrap_or(&hook_name_owned);

                    let display_bound = self.display_system_obj.bind(py);
                    if display_bound.is_none() {
                        log::info!(
                            "Hook message ({}): {} (hook={})",
                            user_message_level,
                            msg_text,
                            source_name
                        );
                    } else {
                        let source_str = format!("hook:{}", source_name);
                        if let Err(e) = display_bound.call_method(
                            "show_message",
                            (msg_text, &user_message_level, &source_str),
                            None,
                        ) {
                            log::error!("Error calling display_system: {e}");
                        }
                    }
                }
            }
        }

        // -----------------------------------------------------------------------
        // SYNCHRONOUS: Section 4 — suppress_output (just log)
        // -----------------------------------------------------------------------
        if suppress_output && action != "ask_user" {
            log::debug!("Hook '{}' requested output suppression", hook_name_owned);
        }

        // -----------------------------------------------------------------------
        // Grab context object for async add_message call (section 1d async part)
        // -----------------------------------------------------------------------
        let context_obj: Py<PyAny> = {
            let mp = self.mount_points.bind(py);
            match mp.get_item("context")? {
                Some(c) if !c.is_none() => c.unbind(),
                _ => py.None(),
            }
        };

        // Grab approval system for ask_user section
        let approval_obj = self.approval_system_obj.clone_ref(py);

        // Keep the original result to return (for non-ask_user paths)
        let result_py = result.unbind();

        // HookResult class for constructing new results in approval flow
        let hook_result_cls: Py<PyAny> = {
            let models = py.import("amplifier_core.models")?;
            models.getattr("HookResult")?.unbind()
        };

        // ApprovalTimeoutError for catching timeouts
        let timeout_err_cls: Py<PyAny> = {
            let approval_mod = py.import("amplifier_core.approval")?;
            approval_mod.getattr("ApprovalTimeoutError")?.unbind()
        };

        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // -------------------------------------------------------
                // 1d (async). Call add_message on context manager
                // -------------------------------------------------------
                if let Some(message_py) = message_to_inject {
                    let bridge = PyContextManagerBridge {
                        py_obj: context_obj,
                    };
                    bridge.add_message(message_py).await?;
                }

                // -------------------------------------------------------
                // 2. Handle approval request (RETURNS EARLY)
                // -------------------------------------------------------
                if action == "ask_user" {
                    let prompt =
                        approval_prompt.unwrap_or_else(|| "Allow this operation?".to_string());
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
                    let has_approval =
                        Python::try_attach(|py| -> bool { !approval_obj.bind(py).is_none() })
                            .unwrap_or(false);

                    if !has_approval {
                        log::error!(
                            "Approval requested but no approval system provided (hook={})",
                            hook_name_owned
                        );
                        return Self::make_hook_result(
                            &hook_result_cls,
                            "deny",
                            Some("No approval system available"),
                            "no-approval deny",
                        );
                    }

                    // Call approval_system.request_approval(...)
                    let approval_result = Self::call_approval_system(
                        &approval_obj,
                        &prompt,
                        &options,
                        approval_timeout,
                        &approval_default,
                    )
                    .await;

                    match approval_result {
                        Ok(decision) => {
                            log::info!(
                                "Approval decision (hook={}, decision={})",
                                hook_name_owned,
                                decision
                            );

                            // Fail-closed: only explicit allow-family strings are
                            // accepted. Any unexpected value is treated as denial
                            // and a warning is emitted so operators can diagnose
                            // a misbehaving approval system.
                            let new_result: Py<PyAny> = if is_approval_granted(&decision) {
                                Self::make_hook_result(
                                    &hook_result_cls,
                                    "continue",
                                    None,
                                    "allow continue",
                                )?
                            } else {
                                if !decision.eq_ignore_ascii_case("deny") {
                                    log::warn!(
                                        "Approval system returned unexpected decision '{}' \
                                         for hook '{}' — treating as deny (fail-closed)",
                                        decision,
                                        hook_name_owned
                                    );
                                }
                                Self::make_hook_result(
                                    &hook_result_cls,
                                    "deny",
                                    Some(&format!("User denied: {}", prompt)),
                                    "user deny",
                                )?
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

                                let timeout_result: Py<PyAny> = if approval_default == "deny" {
                                    Self::make_hook_result(
                                        &hook_result_cls,
                                        "deny",
                                        Some(&format!(
                                            "Approval timeout - denied by default: {}",
                                            prompt
                                        )),
                                        "timeout deny",
                                    )?
                                } else {
                                    Self::make_hook_result(
                                        &hook_result_cls,
                                        "continue",
                                        None,
                                        "timeout continue",
                                    )?
                                };
                                return Ok(timeout_result);
                            }

                            // Not a timeout — re-raise
                            return Err(e);
                        }
                    }
                }

                // Return original result unchanged for non-ask_user paths
                Ok(result_py)
            }),
        )
    }
}

impl PyCoordinator {
    /// Construct a HookResult Python object with the given action and optional reason.
    ///
    /// Centralises the `Python::try_attach` + `PyDict` + `hook_result_cls.call` pattern
    /// that would otherwise appear at every early-return site in the approval flow.
    ///
    /// # Arguments
    /// * `hook_result_cls` – the `HookResult` class imported from `amplifier_core.models`
    /// * `action`          – value for the `action` field (e.g. `"deny"`, `"continue"`)
    /// * `reason`          – optional value for the `reason` field
    /// * `context`         – short label used in the error message if construction fails
    fn make_hook_result(
        hook_result_cls: &Py<PyAny>,
        action: &str,
        reason: Option<&str>,
        context: &str,
    ) -> PyResult<Py<PyAny>> {
        Python::try_attach(|py| -> PyResult<Py<PyAny>> {
            let kwargs = PyDict::new(py);
            kwargs.set_item("action", action)?;
            if let Some(r) = reason {
                kwargs.set_item("reason", r)?;
            }
            hook_result_cls.call(py, (), Some(&kwargs))
        })
        .ok_or_else(|| {
            PyErr::new::<PyRuntimeError, _>(format!("Failed to create {} HookResult", context))
        })?
    }

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
    ) -> Result<String, PyErr> {
        let prompt = prompt.to_string();
        let options: Vec<String> = options.to_vec();
        let default = default.to_string();
        // Call request_approval (may return coroutine)
        let (is_coro, call_result) = Python::try_attach(|py| -> PyResult<(bool, Py<PyAny>)> {
            let opts_list = pyo3::types::PyList::new(py, options.iter().map(|s| s.as_str()))?;
            let result = approval_obj.call_method(
                py,
                "request_approval",
                (&prompt, opts_list, timeout, &default),
                None,
            )?;
            let bound = result.bind(py);
            let inspect = py.import("inspect")?;
            let is_coro: bool = inspect.call_method1("iscoroutine", (bound,))?.extract()?;
            Ok((is_coro, result))
        })
        .ok_or_else(|| {
            PyErr::new::<PyRuntimeError, _>("Failed to attach to Python runtime for approval call")
        })??;

        // Await if coroutine
        let py_result = if is_coro {
            let future = Python::try_attach(|py| {
                pyo3_async_runtimes::tokio::into_future(call_result.into_bound(py))
            })
            .ok_or_else(|| {
                PyErr::new::<PyRuntimeError, _>("Failed to convert approval coroutine")
            })??;
            future.await?
        } else {
            call_result
        };

        // Extract string result
        let decision: String =
            Python::try_attach(|py| py_result.extract(py)).ok_or_else(|| {
                PyErr::new::<PyRuntimeError, _>("Failed to extract approval decision")
            })??;

        Ok(decision)
    }
}
