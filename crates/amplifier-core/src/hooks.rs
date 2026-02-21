//! HookRegistry -- priority-ordered event dispatch pipeline.
//!
//! The hook system provides lifecycle event dispatch with deterministic
//! execution order and action precedence.
//!
//! # Dispatch Semantics
//!
//! Handlers execute **sequentially** by priority (lower number = higher
//! priority). Each handler returns a [`HookResult`] whose `action` field
//! determines how the pipeline continues:
//!
//! | Action          | Behaviour                                              |
//! |-----------------|--------------------------------------------------------|
//! | `Continue`      | Proceed to next handler                                |
//! | `Deny`          | **Short-circuit** -- stop immediately, return deny      |
//! | `Modify`        | Chain `modified_data` to the next handler               |
//! | `InjectContext`  | Collect; merge all at end                              |
//! | `AskUser`       | First one wins; collected for return                    |
//!
//! **Action precedence:** Deny > AskUser > InjectContext > Modify > Continue
//!
//! # Connections
//!
//! - [`HookHandler`](crate::traits::HookHandler) trait defines the handler contract.
//! - [`HookResult`] and [`HookAction`] from [`crate::models`] define results.
//! - Event names come from [`crate::events`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::models::{HookAction, HookResult};
use crate::traits::HookHandler;

// ---------------------------------------------------------------------------
// HandlerEntry -- internal storage for a registered handler
// ---------------------------------------------------------------------------

/// A registered handler with its priority and name.
struct HandlerEntry {
    handler: Arc<dyn HookHandler>,
    priority: i32,
    name: String,
    /// Unique ID for unregistration.
    id: u64,
}

// ---------------------------------------------------------------------------
// HookRegistry
// ---------------------------------------------------------------------------

/// Manages lifecycle hooks with deterministic execution.
///
/// Hooks execute sequentially by priority with short-circuit on deny.
///
/// # Example
///
/// ```rust
/// use amplifier_core::hooks::HookRegistry;
///
/// let registry = HookRegistry::new();
/// // register handlers, emit events ...
/// ```
pub struct HookRegistry {
    /// Handlers keyed by event name, sorted by priority within each event.
    /// Wrapped in `Arc` so unregister closures can safely hold a reference.
    handlers: Arc<Mutex<HashMap<String, Vec<HandlerEntry>>>>,
    /// Default fields merged into every `emit()` call.
    defaults: Mutex<Option<Value>>,
    /// Monotonically increasing ID for handler entries.
    next_id: Mutex<u64>,
}

impl HookRegistry {
    /// Create an empty hook registry.
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(Mutex::new(HashMap::new())),
            defaults: Mutex::new(None),
            next_id: Mutex::new(0),
        }
    }

    /// Register a hook handler for an event.
    ///
    /// # Arguments
    ///
    /// * `event` -- Event name to hook into (e.g., `"tool:pre"`).
    /// * `handler` -- `Arc<dyn HookHandler>` that handles the event.
    /// * `priority` -- Execution priority (lower = earlier).
    /// * `name` -- Optional handler name for debugging.
    ///
    /// # Returns
    ///
    /// An unregister closure. Call it to remove this handler.
    pub fn register(
        &self,
        event: &str,
        handler: Arc<dyn HookHandler>,
        priority: i32,
        name: Option<String>,
    ) -> Box<dyn Fn() + Send + Sync> {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        let entry_name = name.unwrap_or_else(|| format!("handler-{id}"));

        let entry = HandlerEntry {
            handler,
            priority,
            name: entry_name,
            id,
        };

        {
            let mut handlers = self.handlers.lock().unwrap();
            let event_handlers = handlers.entry(event.to_string()).or_default();
            event_handlers.push(entry);
            // Keep sorted by priority (lower = higher priority)
            event_handlers.sort_by_key(|e| e.priority);
        }

        // The unregister closure holds an Arc clone of the handlers map,
        // so it can remove the entry even after the registry borrow ends.
        // This matches Python's pattern where the closure captures self._handlers.
        let event_key = event.to_string();
        let handlers_ref = self.handlers.clone();

        Box::new(move || {
            let mut handlers = handlers_ref.lock().unwrap();
            if let Some(event_handlers) = handlers.get_mut(&event_key) {
                event_handlers.retain(|e| e.id != id);
            }
        })
    }

    /// Set default fields merged into every `emit()` call.
    ///
    /// Defaults are merged with event data, with explicit event data taking
    /// precedence (matching Python's `{**defaults, **data}` pattern).
    pub fn set_default_fields(&self, defaults: Value) {
        *self.defaults.lock().unwrap() = Some(defaults);
    }

    /// Emit an event to all registered handlers.
    ///
    /// Handlers execute sequentially by priority with:
    /// - Short-circuit on `Deny`
    /// - Data modification chaining on `Modify`
    /// - Collection and merging on `InjectContext`
    /// - First-wins on `AskUser`
    ///
    /// Action precedence: Deny > AskUser > InjectContext > Modify > Continue
    pub async fn emit(&self, event: &str, data: Value) -> HookResult {
        // Snapshot handlers for this event (avoids holding the lock during async calls).
        let entries: Vec<(Arc<dyn HookHandler>, String)> = {
            let handlers = self.handlers.lock().unwrap();
            match handlers.get(event) {
                Some(entries) => entries
                    .iter()
                    .map(|e| (e.handler.clone(), e.name.clone()))
                    .collect(),
                None => {
                    return HookResult {
                        action: HookAction::Continue,
                        data: Some(value_to_map(&data)),
                        ..Default::default()
                    };
                }
            }
        };

        if entries.is_empty() {
            return HookResult {
                action: HookAction::Continue,
                data: Some(value_to_map(&data)),
                ..Default::default()
            };
        }

        // Merge default fields with event data (event data takes precedence).
        let mut current_data = {
            let defaults = self.defaults.lock().unwrap();
            match defaults.as_ref() {
                Some(defaults_val) => merge_json(defaults_val, &data),
                None => data,
            }
        };

        // Stamp infrastructure-owned timestamp (UTC ISO-8601).
        // Together with session_id (from defaults), forms the compound identity
        // key (session_id, timestamp) for event uniqueness and ordering.
        // Infrastructure-owned: always present, callers cannot omit or override.
        if let Value::Object(ref mut map) = current_data {
            map.insert(
                "timestamp".to_string(),
                Value::String(chrono::Utc::now().to_rfc3339()),
            );
        }

        // Track special actions
        let mut special_result: Option<HookResult> = None;
        let mut inject_context_results: Vec<HookResult> = Vec::new();

        for (handler, _name) in &entries {
            let result = match handler.handle(event, current_data.clone()).await {
                Ok(r) => r,
                Err(_e) => {
                    // Error in handler -- log and continue (matches Python behaviour).
                    continue;
                }
            };

            // Deny short-circuits immediately
            if result.action == HookAction::Deny {
                return result;
            }

            // Modify chains data to next handler
            if result.action == HookAction::Modify {
                if let Some(ref modified) = result.data {
                    current_data = serde_json::to_value(modified).unwrap_or(current_data);
                }
            }

            // Collect inject_context for merging at end
            if result.action == HookAction::InjectContext && result.context_injection.is_some() {
                inject_context_results.push(result.clone());
            }

            // Preserve ask_user (only first one -- can't merge approvals)
            if result.action == HookAction::AskUser && special_result.is_none() {
                special_result = Some(result);
            }
        }

        // Merge inject_context results if any
        if !inject_context_results.is_empty() {
            let merged_inject = merge_inject_context_results(&inject_context_results);
            if special_result.is_none() {
                // No ask_user captured -- inject_context wins
                special_result = Some(merged_inject);
            }
            // If ask_user already captured, it takes precedence (don't overwrite)
        }

        // Return special action if any hook requested it, otherwise continue
        if let Some(result) = special_result {
            return result;
        }

        // Return final result with potentially modified data
        HookResult {
            action: HookAction::Continue,
            data: Some(value_to_map(&current_data)),
            ..Default::default()
        }
    }

    /// Emit event and collect data from all handler responses.
    ///
    /// Unlike [`emit()`](Self::emit) which processes action semantics,
    /// this method simply collects `result.data` from all handlers for
    /// aggregation. Each handler is called with a timeout.
    ///
    /// Use for decision events where multiple hooks propose candidates
    /// (e.g., tool resolution, agent selection).
    pub async fn emit_and_collect(
        &self,
        event: &str,
        data: Value,
        timeout: Duration,
    ) -> Vec<HashMap<String, Value>> {
        // Snapshot handlers
        let entries: Vec<(Arc<dyn HookHandler>, String)> = {
            let handlers = self.handlers.lock().unwrap();
            match handlers.get(event) {
                Some(entries) => entries
                    .iter()
                    .map(|e| (e.handler.clone(), e.name.clone()))
                    .collect(),
                None => return Vec::new(),
            }
        };

        if entries.is_empty() {
            return Vec::new();
        }

        let mut responses = Vec::new();

        for (handler, _name) in &entries {
            let fut = handler.handle(event, data.clone());
            let result = match tokio::time::timeout(timeout, fut).await {
                Ok(Ok(r)) => r,
                Ok(Err(_e)) => {
                    // Handler error -- skip
                    continue;
                }
                Err(_) => {
                    // Timeout -- skip
                    continue;
                }
            };

            if let Some(d) = result.data {
                responses.push(d);
            }
        }

        responses
    }

    /// List registered handlers.
    ///
    /// If `event` is `Some`, only return handlers for that event.
    /// If `None`, return all handlers grouped by event.
    pub fn list_handlers(&self, event: Option<&str>) -> HashMap<String, Vec<String>> {
        let handlers = self.handlers.lock().unwrap();

        if let Some(evt) = event {
            let names = handlers
                .get(evt)
                .map(|entries| entries.iter().map(|e| e.name.clone()).collect())
                .unwrap_or_default();
            let mut result = HashMap::new();
            result.insert(evt.to_string(), names);
            result
        } else {
            handlers
                .iter()
                .map(|(evt, entries)| {
                    (
                        evt.clone(),
                        entries.iter().map(|e| e.name.clone()).collect(),
                    )
                })
                .collect()
        }
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Merge two JSON values: `base` is overridden by `overlay`.
/// Both should be objects; non-object values result in `overlay` winning.
fn merge_json(base: &Value, overlay: &Value) -> Value {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            let mut merged = base_map.clone();
            for (k, v) in overlay_map {
                merged.insert(k.clone(), v.clone());
            }
            Value::Object(merged)
        }
        _ => overlay.clone(),
    }
}

/// Convert a JSON Value to HashMap<String, Value>.
fn value_to_map(value: &Value) -> HashMap<String, Value> {
    match value {
        Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    }
}

/// Merge multiple inject_context HookResults into a single result.
///
/// Combines injections with `"\n\n"` separator, preserving settings from
/// the first result (role, ephemeral, suppress_output).
fn merge_inject_context_results(results: &[HookResult]) -> HookResult {
    if results.is_empty() {
        return HookResult::default();
    }

    if results.len() == 1 {
        return results[0].clone();
    }

    // Combine all injections
    let combined_content: String = results
        .iter()
        .filter_map(|r| r.context_injection.as_deref())
        .collect::<Vec<_>>()
        .join("\n\n");

    // Use settings from first result
    let first = &results[0];

    HookResult {
        action: HookAction::InjectContext,
        context_injection: Some(combined_content),
        context_injection_role: first.context_injection_role.clone(),
        ephemeral: first.ephemeral,
        suppress_output: first.suppress_output,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::HookError;
    use crate::models::{HookAction, HookResult};
    use crate::traits::HookHandler;
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ---------------------------------------------------------------
    // Test helpers -- minimal handler implementations
    // ---------------------------------------------------------------

    /// Handler that returns a fixed HookResult.
    struct SimpleHandler(HookResult);

    impl HookHandler for SimpleHandler {
        fn handle(
            &self,
            _event: &str,
            _data: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
            let result = self.0.clone();
            Box::pin(async move { Ok(result) })
        }
    }

    /// Handler that counts how many times it's called.
    struct CountingHandler {
        count: AtomicUsize,
    }

    impl CountingHandler {
        fn new() -> Self {
            Self {
                count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    impl HookHandler for CountingHandler {
        fn handle(
            &self,
            _event: &str,
            _data: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(HookResult::default()) })
        }
    }

    /// Handler that logs its label into a shared Vec for ordering verification.
    struct LoggingHandler {
        label: &'static str,
        log: Arc<tokio::sync::Mutex<Vec<&'static str>>>,
    }

    impl HookHandler for LoggingHandler {
        fn handle(
            &self,
            _event: &str,
            _data: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
            let label = self.label;
            let log = self.log.clone();
            Box::pin(async move {
                log.lock().await.push(label);
                Ok(HookResult::default())
            })
        }
    }

    /// Handler that modifies event data by inserting a key-value pair.
    struct ModifyHandler {
        key: &'static str,
        value: &'static str,
    }

    impl HookHandler for ModifyHandler {
        fn handle(
            &self,
            _event: &str,
            data: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
            let key = self.key.to_string();
            let value = self.value.to_string();
            Box::pin(async move {
                let mut map: HashMap<String, serde_json::Value> =
                    serde_json::from_value(data).unwrap_or_default();
                map.insert(key, serde_json::json!(value));
                Ok(HookResult {
                    action: HookAction::Modify,
                    data: Some(map),
                    ..Default::default()
                })
            })
        }
    }

    /// Handler that captures the data it receives for later inspection.
    struct CaptureHandler {
        captured: tokio::sync::Mutex<Option<serde_json::Value>>,
    }

    impl CaptureHandler {
        fn new() -> Self {
            Self {
                captured: tokio::sync::Mutex::new(None),
            }
        }

        async fn last_data(&self) -> serde_json::Value {
            self.captured
                .lock()
                .await
                .clone()
                .unwrap_or(serde_json::json!(null))
        }
    }

    impl HookHandler for CaptureHandler {
        fn handle(
            &self,
            _event: &str,
            data: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
            let captured = &self.captured;
            Box::pin(async move {
                *captured.lock().await = Some(data);
                Ok(HookResult::default())
            })
        }
    }

    /// Handler that always returns an error.
    struct FailingHandler;

    impl HookHandler for FailingHandler {
        fn handle(
            &self,
            _event: &str,
            _data: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
            Box::pin(async {
                Err(HookError::Other {
                    message: "handler failed".into(),
                })
            })
        }
    }

    /// Handler that returns data (for emit_and_collect testing).
    struct DataHandler(serde_json::Value);

    impl HookHandler for DataHandler {
        fn handle(
            &self,
            _event: &str,
            _data: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
            let value = self.0.clone();
            Box::pin(async move {
                let mut map = HashMap::new();
                map.insert("result".to_string(), value);
                Ok(HookResult {
                    data: Some(map),
                    ..Default::default()
                })
            })
        }
    }

    // ---------------------------------------------------------------
    // emit() basic
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn emit_with_no_handlers_returns_continue() {
        let registry = HookRegistry::new();
        let result = registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(result.action, HookAction::Continue);
    }

    #[tokio::test]
    async fn register_and_emit() {
        let registry = HookRegistry::new();
        let handler = Arc::new(SimpleHandler(HookResult::default()));
        let _unregister =
            registry.register("test:event", handler, 0, Some("test-handler".into()));
        let result = registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(result.action, HookAction::Continue);
    }

    // ---------------------------------------------------------------
    // Priority ordering
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn priority_ordering() {
        let registry = HookRegistry::new();
        let log = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        let log1 = log.clone();
        let h1 = Arc::new(LoggingHandler {
            label: "high",
            log: log1,
        });
        let log2 = log.clone();
        let h2 = Arc::new(LoggingHandler {
            label: "low",
            log: log2,
        });

        // Register low priority first, high priority second -- should execute
        // high first because lower number = higher priority.
        registry.register("test:event", h2, 10, Some("low-priority".into()));
        registry.register("test:event", h1, 5, Some("high-priority".into()));

        registry.emit("test:event", serde_json::json!({})).await;
        let order = log.lock().await;
        assert_eq!(*order, vec!["high", "low"]);
    }

    // ---------------------------------------------------------------
    // Deny short-circuits
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn deny_short_circuits() {
        let registry = HookRegistry::new();
        let deny_handler = Arc::new(SimpleHandler(HookResult {
            action: HookAction::Deny,
            reason: Some("blocked".into()),
            ..Default::default()
        }));
        let never_called = Arc::new(CountingHandler::new());

        registry.register("test:event", deny_handler, 0, Some("denier".into()));
        registry.register(
            "test:event",
            never_called.clone(),
            10,
            Some("after-deny".into()),
        );

        let result = registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(result.action, HookAction::Deny);
        assert_eq!(result.reason.as_deref(), Some("blocked"));
        assert_eq!(never_called.call_count(), 0);
    }

    // ---------------------------------------------------------------
    // Action precedence: ask_user > inject_context
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn ask_user_takes_precedence_over_inject_context() {
        let registry = HookRegistry::new();
        let inject = Arc::new(SimpleHandler(HookResult {
            action: HookAction::InjectContext,
            context_injection: Some("injected".into()),
            ..Default::default()
        }));
        let ask = Arc::new(SimpleHandler(HookResult {
            action: HookAction::AskUser,
            approval_prompt: Some("approve?".into()),
            ..Default::default()
        }));

        // inject runs first (priority 0), ask runs second (priority 10)
        registry.register("test:event", inject, 0, None);
        registry.register("test:event", ask, 10, None);

        let result = registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(result.action, HookAction::AskUser);
    }

    // ---------------------------------------------------------------
    // Data modification chains
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn data_modification_chains() {
        let registry = HookRegistry::new();
        let modifier = Arc::new(ModifyHandler {
            key: "added",
            value: "true",
        });
        registry.register("test:event", modifier, 0, None);

        let result = registry
            .emit("test:event", serde_json::json!({"original": true}))
            .await;
        // Result should contain both original and added data
        let data = result.data.unwrap();
        assert_eq!(data["original"], serde_json::json!(true));
        assert_eq!(data["added"], serde_json::json!("true"));
    }

    #[tokio::test]
    async fn multiple_modifiers_chain() {
        let registry = HookRegistry::new();
        let m1 = Arc::new(ModifyHandler {
            key: "first",
            value: "1",
        });
        let m2 = Arc::new(ModifyHandler {
            key: "second",
            value: "2",
        });

        registry.register("test:event", m1, 0, None);
        registry.register("test:event", m2, 10, None);

        let result = registry.emit("test:event", serde_json::json!({})).await;
        let data = result.data.unwrap();
        assert_eq!(data["first"], serde_json::json!("1"));
        assert_eq!(data["second"], serde_json::json!("2"));
    }

    // ---------------------------------------------------------------
    // InjectContext collects from multiple handlers
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn inject_context_merges_multiple() {
        let registry = HookRegistry::new();
        let i1 = Arc::new(SimpleHandler(HookResult {
            action: HookAction::InjectContext,
            context_injection: Some("first injection".into()),
            ..Default::default()
        }));
        let i2 = Arc::new(SimpleHandler(HookResult {
            action: HookAction::InjectContext,
            context_injection: Some("second injection".into()),
            ..Default::default()
        }));

        registry.register("test:event", i1, 0, None);
        registry.register("test:event", i2, 10, None);

        let result = registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(result.action, HookAction::InjectContext);
        // Merged with "\n\n" separator per Python behaviour
        let injection = result.context_injection.unwrap();
        assert!(injection.contains("first injection"));
        assert!(injection.contains("second injection"));
    }

    // ---------------------------------------------------------------
    // Unregister
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn unregister_removes_handler() {
        let registry = HookRegistry::new();
        let handler = Arc::new(CountingHandler::new());
        let unregister = registry.register("test:event", handler.clone(), 0, None);

        registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(handler.call_count(), 1);

        unregister();
        registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(handler.call_count(), 1); // Not called again
    }

    // ---------------------------------------------------------------
    // Default fields
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn default_fields_merged_into_events() {
        let registry = HookRegistry::new();
        registry.set_default_fields(serde_json::json!({
            "session_id": "test-123"
        }));

        let capture = Arc::new(CaptureHandler::new());
        registry.register("test:event", capture.clone(), 0, None);

        registry
            .emit("test:event", serde_json::json!({"custom": true}))
            .await;
        let captured = capture.last_data().await;
        assert_eq!(captured["session_id"], "test-123");
        assert_eq!(captured["custom"], true);
    }

    #[tokio::test]
    async fn event_data_overrides_defaults() {
        let registry = HookRegistry::new();
        registry.set_default_fields(serde_json::json!({
            "key": "default"
        }));

        let capture = Arc::new(CaptureHandler::new());
        registry.register("test:event", capture.clone(), 0, None);

        registry
            .emit("test:event", serde_json::json!({"key": "override"}))
            .await;
        let captured = capture.last_data().await;
        assert_eq!(captured["key"], "override");
    }

    // ---------------------------------------------------------------
    // Handler errors continue to next
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn handler_error_continues_to_next() {
        let registry = HookRegistry::new();
        let failing = Arc::new(FailingHandler);
        let succeeding = Arc::new(CountingHandler::new());

        registry.register("test:event", failing, 0, None);
        registry.register("test:event", succeeding.clone(), 10, None);

        let result = registry.emit("test:event", serde_json::json!({})).await;
        assert_eq!(result.action, HookAction::Continue);
        assert_eq!(succeeding.call_count(), 1); // Still called
    }

    // ---------------------------------------------------------------
    // emit_and_collect
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn emit_and_collect_gathers_data() {
        let registry = HookRegistry::new();
        let h1 = Arc::new(DataHandler(serde_json::json!("result-1")));
        let h2 = Arc::new(DataHandler(serde_json::json!("result-2")));

        registry.register("test:event", h1, 0, None);
        registry.register("test:event", h2, 10, None);

        let results = registry
            .emit_and_collect(
                "test:event",
                serde_json::json!({}),
                std::time::Duration::from_secs(1),
            )
            .await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn emit_and_collect_empty_with_no_handlers() {
        let registry = HookRegistry::new();
        let results = registry
            .emit_and_collect(
                "test:event",
                serde_json::json!({}),
                std::time::Duration::from_secs(1),
            )
            .await;
        assert!(results.is_empty());
    }

    // ---------------------------------------------------------------
    // list_handlers
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn list_handlers_returns_names() {
        let registry = HookRegistry::new();
        let h = Arc::new(SimpleHandler(HookResult::default()));
        registry.register("tool:pre", h.clone(), 0, Some("my-hook".into()));
        registry.register("tool:post", h, 0, Some("other-hook".into()));

        let handlers = registry.list_handlers(None);
        assert!(handlers.contains_key("tool:pre"));
        assert!(handlers["tool:pre"].contains(&"my-hook".to_string()));
        assert!(handlers.contains_key("tool:post"));
    }

    #[tokio::test]
    async fn list_handlers_filters_by_event() {
        let registry = HookRegistry::new();
        let h = Arc::new(SimpleHandler(HookResult::default()));
        registry.register("tool:pre", h.clone(), 0, Some("my-hook".into()));
        registry.register("tool:post", h, 0, Some("other-hook".into()));

        let handlers = registry.list_handlers(Some("tool:pre"));
        assert!(handlers.contains_key("tool:pre"));
        assert!(!handlers.contains_key("tool:post"));
    }

    // ---------------------------------------------------------------
    // Event timestamp stamping
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_emit_stamps_timestamp() {
        let registry = HookRegistry::new();
        let capture = Arc::new(CaptureHandler::new());
        registry.register("test:event", capture.clone(), 0, None);

        registry
            .emit("test:event", serde_json::json!({"key": "value"}))
            .await;

        let captured = capture.last_data().await;
        // Must have a "timestamp" key
        let ts = captured["timestamp"]
            .as_str()
            .expect("timestamp must be a string");
        // Must parse as a valid RFC 3339 / ISO-8601 timestamp
        chrono::DateTime::parse_from_rfc3339(ts)
            .expect("timestamp must be valid ISO-8601 / RFC 3339");
    }

    #[tokio::test]
    async fn test_emit_timestamp_is_infrastructure_owned() {
        let registry = HookRegistry::new();
        let capture = Arc::new(CaptureHandler::new());
        registry.register("test:event", capture.clone(), 0, None);

        // Caller tries to supply their own timestamp — infrastructure must overwrite it
        registry
            .emit(
                "test:event",
                serde_json::json!({"timestamp": "user-provided"}),
            )
            .await;

        let captured = capture.last_data().await;
        let ts = captured["timestamp"]
            .as_str()
            .expect("timestamp must be a string");
        assert_ne!(ts, "user-provided", "infrastructure must overwrite caller timestamp");
        // Must still be valid ISO-8601
        chrono::DateTime::parse_from_rfc3339(ts)
            .expect("overwritten timestamp must be valid ISO-8601");
    }

    #[tokio::test]
    async fn test_emit_and_collect_does_not_stamp_timestamp() {
        let registry = HookRegistry::new();
        let capture = Arc::new(CaptureHandler::new());
        registry.register("test:event", capture.clone(), 0, None);

        registry
            .emit_and_collect(
                "test:event",
                serde_json::json!({"key": "value"}),
                std::time::Duration::from_secs(1),
            )
            .await;

        let captured = capture.last_data().await;
        // emit_and_collect must NOT stamp a timestamp
        assert!(
            captured.get("timestamp").is_none()
                || captured["timestamp"].is_null(),
            "emit_and_collect must not add a timestamp"
        );
    }

    // ---------------------------------------------------------------
    // Events only dispatch to registered event handlers
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn handlers_only_called_for_registered_event() {
        let registry = HookRegistry::new();
        let counter = Arc::new(CountingHandler::new());
        registry.register("tool:pre", counter.clone(), 0, None);

        // Emit a different event
        registry.emit("tool:post", serde_json::json!({})).await;
        assert_eq!(counter.call_count(), 0);

        // Emit the registered event
        registry.emit("tool:pre", serde_json::json!({})).await;
        assert_eq!(counter.call_count(), 1);
    }
}
