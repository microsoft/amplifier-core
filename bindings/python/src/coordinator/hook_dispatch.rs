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

/// Truncate a string for safe inclusion in log messages and reason fields.
///
/// Prevents hook-controlled prompts from flooding logs or embedding sensitive
/// data in audit trails. Truncated strings are clearly marked with `...[truncated]`.
fn truncate_for_log(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...[truncated]", truncated)
    }
}

struct PreparedInjection {
    content: String,
    role: String,
    ephemeral: bool,
    append_to_last_tool_result: bool,
    hook_name: String,
    event: String,
    tokens: usize,
}

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
    /// ## Phase ordering
    ///
    /// Phases A–C run synchronously (content sanitization, validation, user_message,
    /// suppress_output). Phases D–E run in the async block (add_message, approval
    /// request). This ordering ensures all GIL-holding state mutations complete
    /// before any async I/O, so no mutable references need to cross the async boundary.
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

        if action == "inject_context" {
            return self.process_context_injections(py, result, event, hook_name);
        }

        // Read coordinator config
        let size_limit: Option<usize> = {
            let val = self.get_injection_size_limit(py)?;
            if val.bind(py).is_none() {
                None
            } else {
                Some(val.extract(py)?)
            }
        };
        let budget: Option<usize> = {
            let val = self.get_injection_budget_per_turn(py)?;
            if val.bind(py).is_none() {
                None
            } else {
                Some(val.extract(py)?)
            }
        };

        let hook_name_owned = hook_name.to_string();

        // -----------------------------------------------------------------------
        // Phase A (sync): context injection — sanitize content, validate
        //                 size/budget, update state
        //
        // All state mutations happen here (before the async block) so we never
        // need to capture mutable references in a Send + 'static future.
        //
        // Sanitization runs FIRST so size and budget limits are enforced against
        // the content that will actually be injected, not raw hook-provided data.
        // The budget counter is also incremented only after sanitization succeeds.
        // -----------------------------------------------------------------------
        //
        // `message_to_inject` is the pre-built message dict to pass into the
        // async block.  It is Some(_) when we should call add_message, None
        // when the injection is ephemeral or has no context to inject.
        let message_to_inject: Option<Py<PyAny>> = if action == "inject_context" {
            match context_injection.as_deref() {
                Some(content) if !content.is_empty() => {
                    // A.1. Sanitize content FIRST (fail closed if unavailable).
                    //
                    // Sanitization runs before size/budget checks so limits are
                    // enforced against the post-sanitized content that will actually
                    // be stored, not raw hook-provided data.
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

                    // A.2. Validate size limit against sanitized content (HARD ERROR — raises ValueError).
                    //
                    // Use char count (Unicode scalar values) not byte count so
                    // non-ASCII content (CJK, emoji, etc.) is measured the same
                    // way Python's len() measures strings.
                    let char_count = sanitized_content.chars().count();
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

                    // A.3. Check budget against sanitized content (SOFT WARNING — log but continue).
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

                    // A.4. Update turn injection counter.
                    //
                    // Incremented only after sanitization succeeds — failed
                    // sanitizations are rejected without charging the budget.
                    self.current_turn_injections += tokens;

                    // A.5. Build message dict for async injection (ONLY if not ephemeral).
                    let msg_opt = if !ephemeral {
                        let ctx_bound = self.mount_points.bind(py);
                        let ctx_item = ctx_bound.get_item("context")?;
                        let has_context = ctx_item.as_ref().is_some_and(|c| {
                            !c.is_none() && c.hasattr("add_message").unwrap_or(false)
                        });

                        if has_context {
                            let datetime_mod = py.import("datetime")?;
                            let utc = datetime_mod.getattr("timezone")?.getattr("utc")?;
                            let now = datetime_mod
                                .getattr("datetime")?
                                .call_method1("now", (utc,))?
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

                    // A.6. Audit log (always, even if ephemeral).
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
        // Phase B (sync): user_message — call display_system.show_message()
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
                        log::debug!(
                            "Hook message ({}): {} (hook={})",
                            user_message_level,
                            truncate_for_log(msg_text, 200),
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
        // Phase C (sync): suppress_output — log only
        // -----------------------------------------------------------------------
        if suppress_output && action != "ask_user" {
            log::debug!("Hook '{}' requested output suppression", hook_name_owned);
        }

        // -----------------------------------------------------------------------
        // Grab context object for Phase D async add_message call
        // -----------------------------------------------------------------------
        let context_obj: Py<PyAny> = {
            let mp = self.mount_points.bind(py);
            match mp.get_item("context")? {
                Some(c) if !c.is_none() => c.unbind(),
                _ => py.None(),
            }
        };

        // Grab approval system for Phase E ask_user
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
                // Phase D (async): context injection — call add_message
                // -------------------------------------------------------
                if let Some(message_py) = message_to_inject {
                    let bridge = PyContextManagerBridge {
                        py_obj: context_obj,
                    };
                    bridge.add_message(message_py).await?;
                }

                // -------------------------------------------------------
                // Phase E (async): approval request — call request_approval
                //                  (RETURNS EARLY)
                // -------------------------------------------------------
                if action == "ask_user" {
                    let prompt =
                        approval_prompt.unwrap_or_else(|| "Allow this operation?".to_string());

                    // NOTE: Custom approval_options from hooks must include an "allow"-family
                    // string (e.g., "Allow", "Allow once", "Allow always") for the approval
                    // to be granted. is_approval_granted() only matches these specific strings.
                    // This is fail-closed by design — unknown option strings are treated as denial.
                    let options = approval_options
                        .unwrap_or_else(|| vec!["Allow".to_string(), "Deny".to_string()]);

                    log::info!(
                        "Approval requested (hook={}, prompt={}, timeout={}, default={})",
                        hook_name_owned,
                        truncate_for_log(&prompt, 200),
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
                                    Some(&format!(
                                        "User denied: {}",
                                        truncate_for_log(&prompt, 200)
                                    )),
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

                                // Fail-closed: only explicit allow-family defaults grant access on timeout.
                                // "deny", "Deny", unexpected strings, empty → all denied.
                                let timeout_allows = is_approval_granted(&approval_default);
                                let timeout_result: Py<PyAny> = if timeout_allows {
                                    Self::make_hook_result(
                                        &hook_result_cls,
                                        "continue",
                                        None,
                                        "timeout continue",
                                    )?
                                } else {
                                    Self::make_hook_result(
                                        &hook_result_cls,
                                        "deny",
                                        Some(&format!(
                                            "Approval timeout - denied by default: {}",
                                            truncate_for_log(&prompt, 200)
                                        )),
                                        "timeout deny",
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
    fn process_context_injections<'py>(
        &mut self,
        py: Python<'py>,
        result: Bound<'py, PyAny>,
        event: String,
        hook_name: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let scalar_content: Option<String> = {
            let value = result.getattr("context_injection")?;
            (!value.is_none()).then(|| value.extract()).transpose()?
        };
        let scalar_role: String = result.getattr("context_injection_role")?.extract()?;
        let scalar_ephemeral: bool = result.getattr("ephemeral")?.extract()?;
        let scalar_append: bool = result.getattr("append_to_last_tool_result")?.extract()?;
        let suppress_output: bool = result.getattr("suppress_output")?.extract()?;
        let user_message: Option<String> = {
            let value = result.getattr("user_message")?;
            (!value.is_none()).then(|| value.extract()).transpose()?
        };
        let user_message_level: String = result.getattr("user_message_level")?.extract()?;
        let user_message_source: Option<String> = {
            let value = result.getattr("user_message_source")?;
            (!value.is_none()).then(|| value.extract()).transpose()?
        };

        let size_limit: Option<usize> = {
            let value = self.get_injection_size_limit(py)?;
            (!value.bind(py).is_none())
                .then(|| value.extract(py))
                .transpose()?
        };
        let budget: Option<usize> = {
            let value = self.get_injection_budget_per_turn(py)?;
            (!value.bind(py).is_none())
                .then(|| value.extract(py))
                .transpose()?
        };

        let items = result.getattr("context_injections")?;
        let mut raw_items = Vec::new();
        for item in items.try_iter()? {
            let item = item?;
            raw_items.push((
                item.getattr("content")?.extract::<String>()?,
                item.getattr("role")?.extract::<String>()?,
                item.getattr("ephemeral")?.extract::<bool>()?,
                item.getattr("append_to_last_tool_result")?
                    .extract::<bool>()?,
                item.getattr("hook_name")?.extract::<String>()?,
                item.getattr("event")?.extract::<String>()?,
            ));
        }

        // Scalar-only results predate the structured carrier. Normalize them
        // here for direct callers that have not passed through HookRegistry.
        if raw_items.is_empty() {
            if let Some(content) = scalar_content {
                raw_items.push((
                    content,
                    scalar_role,
                    scalar_ephemeral,
                    scalar_append,
                    hook_name.to_string(),
                    event.clone(),
                ));
            }
        }

        const CHARS_PER_TOKEN_ESTIMATE: usize = 4;
        let sanitize = py
            .import("amplifier_core.models")?
            .getattr("_sanitize_for_llm")?;
        let mut prepared = Vec::new();
        for (content, role, ephemeral, append, item_hook_name, item_event) in raw_items {
            let sanitized_content: String = sanitize
                .call1((content,))?
                .extract()
                .map_err(|error| {
                    log::error!(
                        "SECURITY: Sanitization unavailable for hook '{}' — rejecting injection: {error}",
                        item_hook_name
                    );
                    PyValueError::new_err(
                        "Context injection rejected: content sanitization function unavailable",
                    )
                })?;
            if sanitized_content.is_empty() {
                continue;
            }
            let char_count = sanitized_content.chars().count();
            if let Some(limit) = size_limit {
                if char_count > limit {
                    log::error!(
                        "Hook injection too large: {} (chars={}, limit={})",
                        item_hook_name,
                        char_count,
                        limit
                    );
                    return Err(PyValueError::new_err(format!(
                        "Context injection exceeds {} characters",
                        limit
                    )));
                }
            }
            prepared.push(PreparedInjection {
                content: sanitized_content,
                role,
                ephemeral,
                append_to_last_tool_result: append,
                hook_name: item_hook_name,
                event: item_event,
                tokens: char_count / CHARS_PER_TOKEN_ESTIMATE,
            });
        }

        // Every item is validated before the budget state is charged or any
        // context write begins. Budget is still advisory, as it was for the
        // legacy scalar path, but applies to the total item sequence.
        let mut running_total = self.current_turn_injections;
        for item in &prepared {
            if let Some(limit) = budget {
                if running_total + item.tokens > limit {
                    log::warn!(
                        "Warning: Hook injection budget exceeded \
                         (hook={}, current={}, attempted={}, budget={})",
                        item.hook_name,
                        running_total,
                        item.tokens,
                        limit
                    );
                }
            }
            running_total += item.tokens;
        }
        self.current_turn_injections = running_total;

        let context_obj: Py<PyAny> = {
            let mount_points = self.mount_points.bind(py);
            match mount_points.get_item("context")? {
                Some(context) if !context.is_none() => context.unbind(),
                _ => py.None(),
            }
        };
        let has_context = {
            let context = context_obj.bind(py);
            !context.is_none() && context.hasattr("add_message")?
        };

        let now = if has_context && prepared.iter().any(|item| !item.ephemeral) {
            let datetime = py.import("datetime")?;
            let utc = datetime.getattr("timezone")?.getattr("utc")?;
            Some(
                datetime
                    .getattr("datetime")?
                    .call_method1("now", (utc,))?
                    .call_method0("isoformat")?
                    .unbind(),
            )
        } else {
            None
        };

        let mut messages = Vec::new();
        let mut residual = Vec::new();
        for item in prepared {
            log::info!(
                "Hook context injection \
                 (hook={}, event={}, chars={}, role={}, tokens={}, ephemeral={})",
                item.hook_name,
                item.event,
                item.content.chars().count(),
                item.role,
                item.tokens,
                item.ephemeral
            );
            if item.ephemeral {
                residual.push(item);
                continue;
            }
            if has_context {
                let metadata = PyDict::new(py);
                metadata.set_item("source", "hook")?;
                metadata.set_item("hook_name", &item.hook_name)?;
                metadata.set_item("event", &item.event)?;
                metadata.set_item("timestamp", now.as_ref().unwrap().bind(py))?;
                metadata.set_item("requested_role", &item.role)?;
                metadata.set_item("ephemeral", false)?;
                metadata.set_item("persisted", true)?;

                let message = PyDict::new(py);
                message.set_item("role", &item.role)?;
                message.set_item("content", &item.content)?;
                message.set_item("metadata", metadata)?;
                messages.push(message.into_any().unbind());
            }
        }

        if let Some(message) = user_message.filter(|message| !message.is_empty()) {
            let source = user_message_source.as_deref().unwrap_or(hook_name);
            let display = self.display_system_obj.bind(py);
            if display.is_none() {
                log::debug!(
                    "Hook message ({}): {} (hook={})",
                    user_message_level,
                    truncate_for_log(&message, 200),
                    source
                );
            } else if let Err(error) = display.call_method(
                "show_message",
                (&message, &user_message_level, format!("hook:{source}")),
                None,
            ) {
                log::error!("Error calling display_system: {error}");
            }
        }
        if suppress_output {
            log::debug!("Hook '{}' requested output suppression", hook_name);
        }

        let result_py = result.unbind();
        let hook_result_cls = py
            .import("amplifier_core.models")?
            .getattr("HookResult")?
            .unbind();
        wrap_future_as_coroutine(
            py,
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let bridge = PyContextManagerBridge {
                    py_obj: context_obj,
                };
                for message in messages {
                    bridge.add_message(message).await?;
                }
                Self::make_residual_hook_result(&hook_result_cls, &result_py, &residual)
            }),
        )
    }

    fn make_residual_hook_result(
        hook_result_cls: &Py<PyAny>,
        original: &Py<PyAny>,
        residual: &[PreparedInjection],
    ) -> PyResult<Py<PyAny>> {
        Python::try_attach(|py| -> PyResult<Py<PyAny>> {
            let values = original.call_method0(py, "model_dump")?;
            let updates = PyDict::new(py);
            let injections = pyo3::types::PyList::empty(py);
            for item in residual {
                let injection = PyDict::new(py);
                injection.set_item("content", &item.content)?;
                injection.set_item("role", &item.role)?;
                injection.set_item("ephemeral", item.ephemeral)?;
                injection.set_item(
                    "append_to_last_tool_result",
                    item.append_to_last_tool_result,
                )?;
                injection.set_item("hook_name", &item.hook_name)?;
                injection.set_item("event", &item.event)?;
                injections.append(injection)?;
            }
            updates.set_item("context_injections", injections)?;
            if residual.is_empty() {
                updates.set_item("action", "continue")?;
                updates.set_item("context_injection", py.None())?;
                updates.set_item("context_injection_role", "system")?;
                updates.set_item("ephemeral", false)?;
                updates.set_item("append_to_last_tool_result", false)?;
            } else {
                updates.set_item("action", "inject_context")?;
                updates.set_item(
                    "context_injection",
                    residual
                        .iter()
                        .map(|item| item.content.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                )?;
                updates.set_item("context_injection_role", &residual[0].role)?;
                updates.set_item("ephemeral", true)?;
                updates.set_item(
                    "append_to_last_tool_result",
                    residual.iter().any(|item| item.append_to_last_tool_result),
                )?;
            }
            values.call_method1(py, "update", (updates,))?;
            hook_result_cls.call_method1(py, "model_validate", (values,))
        })
        .ok_or_else(|| PyRuntimeError::new_err("Failed to create residual HookResult"))?
    }

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
