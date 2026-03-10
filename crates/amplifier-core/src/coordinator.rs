//! ModuleCoordinator — central coordination hub for the Amplifier kernel.
//!
//! The coordinator holds mount points for all module types, a capability
//! registry for inter-module communication, contribution channels for
//! data aggregation, cleanup functions, and the hook/cancellation subsystems.
//!
//! # Design
//!
//! The Python `ModuleCoordinator` uses dynamic typing extensively. In Rust
//! we use typed fields for the four primary module slots (orchestrator,
//! context, providers, tools) and typed accessor methods. Capabilities
//! are stored as `serde_json::Value` for maximum flexibility.
//!
//! # Connections
//!
//! - Holds a [`HookRegistry`](crate::hooks::HookRegistry) for event dispatch.
//! - Holds a [`CancellationToken`](crate::cancellation::CancellationToken)
//!   for cooperative cancellation.
//! - Stores modules as `Arc<dyn Trait>` from [`crate::traits`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::cancellation::CancellationToken;
use crate::hooks::HookRegistry;
use crate::traits::{ApprovalProvider, ContextManager, DisplayService, Orchestrator, Provider, Tool};

// ---------------------------------------------------------------------------
// Type aliases for cleanup and contributor callbacks
// ---------------------------------------------------------------------------

/// An async cleanup function: `() -> Future<Output = ()>`.
pub type CleanupFn = Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// An async contributor callback: `() -> Future<Output = Result<Value, ...>>`.
pub type ContributorCallback = Box<
    dyn Fn() -> Pin<
            Box<
                dyn Future<Output = Result<Value, Box<dyn std::error::Error + Send + Sync>>> + Send,
            >,
        > + Send
        + Sync,
>;

/// A registered contributor with name and callback.
struct ContributorEntry {
    name: String,
    callback: ContributorCallback,
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

/// Central coordination hub for module mount points, capabilities, and services.
///
/// Holds the four primary module slots (orchestrator, context manager,
/// providers, tools), plus the hook registry and cancellation token.
///
/// # Example
///
/// ```rust
/// use amplifier_core::coordinator::Coordinator;
///
/// let coord = Coordinator::new(Default::default());
/// assert!(coord.tools().is_empty());
/// ```
pub struct Coordinator {
    // -- Module mount points (typed) --
    orchestrator: Mutex<Option<Arc<dyn Orchestrator>>>,
    context: Mutex<Option<Arc<dyn ContextManager>>>,
    providers: Mutex<HashMap<String, Arc<dyn Provider>>>,
    tools: Mutex<HashMap<String, Arc<dyn Tool>>>,

    // -- Subsystems --
    hooks: Arc<HookRegistry>,
    cancellation: CancellationToken,

    // -- Capabilities & contributions --
    capabilities: Mutex<HashMap<String, Value>>,
    channels: Mutex<HashMap<String, Vec<ContributorEntry>>>,

    // -- Cleanup --
    cleanup_functions: Mutex<Vec<CleanupFn>>,

    // -- Config --
    config: HashMap<String, Value>,

    // -- App-layer services --
    approval_provider: Mutex<Option<Arc<dyn ApprovalProvider>>>,
    display_service: Mutex<Option<Arc<dyn DisplayService>>>,

    // -- Turn tracking --
    current_turn_injections: Mutex<usize>,
}

impl Coordinator {
    /// Create a new coordinator with the given session config.
    pub fn new(config: HashMap<String, Value>) -> Self {
        Self {
            orchestrator: Mutex::new(None),
            context: Mutex::new(None),
            providers: Mutex::new(HashMap::new()),
            tools: Mutex::new(HashMap::new()),
            hooks: Arc::new(HookRegistry::new()),
            cancellation: CancellationToken::new(),
            capabilities: Mutex::new(HashMap::new()),
            channels: Mutex::new(HashMap::new()),
            cleanup_functions: Mutex::new(Vec::new()),
            config,
            approval_provider: Mutex::new(None),
            display_service: Mutex::new(None),
            current_turn_injections: Mutex::new(0),
        }
    }

    /// Create a coordinator with empty config (convenience for tests).
    pub fn new_for_test() -> Self {
        Self::new(HashMap::new())
    }

    // -- Module mount/get: Orchestrator --

    /// Set the orchestrator module (single slot).
    pub fn set_orchestrator(&self, orchestrator: Arc<dyn Orchestrator>) {
        *self.orchestrator.lock().unwrap() = Some(orchestrator);
    }

    /// Get the orchestrator module, if mounted.
    pub fn orchestrator(&self) -> Option<Arc<dyn Orchestrator>> {
        self.orchestrator.lock().unwrap().clone()
    }

    // -- Module mount/get: ContextManager --

    /// Set the context manager module (single slot).
    pub fn set_context(&self, context: Arc<dyn ContextManager>) {
        *self.context.lock().unwrap() = Some(context);
    }

    /// Get the context manager module, if mounted.
    pub fn context(&self) -> Option<Arc<dyn ContextManager>> {
        self.context.lock().unwrap().clone()
    }

    // -- Module mount/get: Providers --

    /// Mount a provider by name.
    pub fn mount_provider(&self, name: &str, provider: Arc<dyn Provider>) {
        self.providers
            .lock()
            .unwrap()
            .insert(name.to_string(), provider);
    }

    /// Get a single provider by name.
    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn Provider>> {
        self.providers.lock().unwrap().get(name).cloned()
    }

    /// Get all mounted providers as a snapshot.
    pub fn providers(&self) -> HashMap<String, Arc<dyn Provider>> {
        self.providers.lock().unwrap().clone()
    }

    /// Unmount a provider by name. Returns `true` if it was present.
    pub fn unmount_provider(&self, name: &str) -> bool {
        self.providers.lock().unwrap().remove(name).is_some()
    }

    // -- Module mount/get: Tools --

    /// Mount a tool by name.
    pub fn mount_tool(&self, name: &str, tool: Arc<dyn Tool>) {
        self.tools.lock().unwrap().insert(name.to_string(), tool);
    }

    /// Get a single tool by name.
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.lock().unwrap().get(name).cloned()
    }

    /// Get all mounted tools as a snapshot.
    pub fn tools(&self) -> HashMap<String, Arc<dyn Tool>> {
        self.tools.lock().unwrap().clone()
    }

    /// Unmount a tool by name. Returns `true` if it was present.
    pub fn unmount_tool(&self, name: &str) -> bool {
        self.tools.lock().unwrap().remove(name).is_some()
    }

    // -- Read-only accessor methods (for to_dict / introspection) --

    /// Names of all mounted tools.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.lock().unwrap().keys().cloned().collect()
    }

    /// Names of all mounted providers.
    pub fn provider_names(&self) -> Vec<String> {
        self.providers.lock().unwrap().keys().cloned().collect()
    }

    /// Whether an orchestrator is mounted.
    pub fn has_orchestrator(&self) -> bool {
        self.orchestrator.lock().unwrap().is_some()
    }

    /// Whether a context manager is mounted.
    pub fn has_context(&self) -> bool {
        self.context.lock().unwrap().is_some()
    }

    // -- App-layer service: ApprovalProvider --

    /// Set the approval provider (single slot).
    pub fn set_approval_provider(&self, provider: Arc<dyn ApprovalProvider>) {
        *self.approval_provider.lock().unwrap() = Some(provider);
    }

    /// Clear the approval provider.
    pub fn clear_approval_provider(&self) {
        *self.approval_provider.lock().unwrap() = None;
    }

    /// Get the approval provider, if mounted.
    pub fn approval_provider(&self) -> Option<Arc<dyn ApprovalProvider>> {
        self.approval_provider.lock().unwrap().clone()
    }

    /// Whether an approval provider is mounted.
    pub fn has_approval_provider(&self) -> bool {
        self.approval_provider.lock().unwrap().is_some()
    }

    // -- App-layer service: DisplayService --

    /// Set the display service (single slot).
    pub fn set_display_service(&self, service: Arc<dyn DisplayService>) {
        *self.display_service.lock().unwrap() = Some(service);
    }

    /// Get the display service, if mounted.
    pub fn display_service(&self) -> Option<Arc<dyn DisplayService>> {
        self.display_service.lock().unwrap().clone()
    }

    /// Whether a display service is mounted.
    pub fn has_display_service(&self) -> bool {
        self.display_service.lock().unwrap().is_some()
    }

    /// Names of all registered capabilities.
    pub fn capability_names(&self) -> Vec<String> {
        self.capabilities.lock().unwrap().keys().cloned().collect()
    }

    /// Return a JSON-compatible dict of all coordinator state for serialization/introspection.
    ///
    /// Returns a `HashMap` with keys: `tools`, `providers`, `has_orchestrator`,
    /// `has_context`, `capabilities`, `has_approval_provider` — matching the
    /// universal Coordinator API.
    pub fn to_dict(&self) -> HashMap<String, serde_json::Value> {
        let mut dict = HashMap::new();
        dict.insert("tools".to_string(), serde_json::json!(self.tool_names()));
        dict.insert(
            "providers".to_string(),
            serde_json::json!(self.provider_names()),
        );
        dict.insert(
            "has_orchestrator".to_string(),
            serde_json::json!(self.has_orchestrator()),
        );
        dict.insert(
            "has_context".to_string(),
            serde_json::json!(self.has_context()),
        );
        dict.insert(
            "capabilities".to_string(),
            serde_json::json!(self.capability_names()),
        );
        dict.insert(
            "has_approval_provider".to_string(),
            serde_json::json!(self.has_approval_provider()),
        );
        dict.insert(
            "has_display_service".to_string(),
            serde_json::json!(self.has_display_service()),
        );
        dict
    }

    // -- Subsystem accessors --

    /// Reference to the hook registry.
    pub fn hooks(&self) -> &HookRegistry {
        &self.hooks
    }

    /// Shared ownership of the hook registry.
    ///
    /// Returns a clone of the `Arc<HookRegistry>`, enabling binding layers
    /// (Node, Go, etc.) to hold long-lived shared references to the same
    /// registry instance that the Coordinator uses internally.
    ///
    /// The existing [`hooks()`](Self::hooks) method continues to return
    /// `&HookRegistry` via `Arc::Deref` — all existing call sites are
    /// unchanged.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::sync::Arc;
    /// use amplifier_core::coordinator::Coordinator;
    ///
    /// let coord = Coordinator::new_for_test();
    /// let shared: Arc<amplifier_core::hooks::HookRegistry> = coord.hooks_shared();
    ///
    /// // Both point to the same registry
    /// assert_eq!(coord.hooks().list_handlers(None).len(), shared.list_handlers(None).len());
    /// ```
    pub fn hooks_shared(&self) -> Arc<HookRegistry> {
        Arc::clone(&self.hooks)
    }

    /// Reference to the cancellation token.
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    // -- Config --

    /// Session configuration.
    pub fn config(&self) -> &HashMap<String, Value> {
        &self.config
    }

    // -- Capabilities --

    /// Register a capability (inter-module communication).
    pub fn register_capability(&self, name: &str, value: Value) {
        self.capabilities
            .lock()
            .unwrap()
            .insert(name.to_string(), value);
    }

    /// Get a registered capability.
    pub fn get_capability(&self, name: &str) -> Option<Value> {
        self.capabilities.lock().unwrap().get(name).cloned()
    }

    // -- Contribution channels --

    /// Register a contributor to a named channel.
    ///
    /// # Arguments
    ///
    /// * `channel` — Channel name (e.g., `"observability.events"`).
    /// * `name` — Module name for debugging.
    /// * `callback` — Async callback that returns a `Value` contribution.
    pub fn register_contributor(&self, channel: &str, name: &str, callback: ContributorCallback) {
        let entry = ContributorEntry {
            name: name.to_string(),
            callback,
        };
        self.channels
            .lock()
            .unwrap()
            .entry(channel.to_string())
            .or_default()
            .push(entry);
    }

    /// Collect contributions from a channel.
    ///
    /// Calls each registered contributor and returns non-error results.
    /// Errors in individual contributors are logged and skipped.
    pub async fn collect_contributions(&self, channel: &str) -> Vec<Value> {
        // Snapshot callbacks to avoid holding lock during async calls
        let entries: Vec<(String, _)> = {
            let channels = self.channels.lock().unwrap();
            match channels.get(channel) {
                Some(entries) => entries
                    .iter()
                    .map(|e| {
                        let fut = (e.callback)();
                        (e.name.clone(), fut)
                    })
                    .collect(),
                None => return Vec::new(),
            }
        };

        let mut results = Vec::new();
        for (name, fut) in entries {
            match fut.await {
                Ok(value) => results.push(value),
                Err(e) => {
                    log::warn!("Contributor '{name}' failed: {e}");
                    continue;
                }
            }
        }
        results
    }

    // -- Cleanup --

    /// Register a cleanup function to be called on shutdown.
    pub fn register_cleanup(&self, cleanup_fn: CleanupFn) {
        self.cleanup_functions.lock().unwrap().push(cleanup_fn);
    }

    /// Run all cleanup functions in reverse registration order.
    ///
    /// Errors in one cleanup function do not prevent subsequent functions
    /// from running (matching Python behaviour).
    pub async fn cleanup(&self) {
        // Take functions out to avoid holding lock during async calls
        let functions: Vec<_> = {
            let mut fns = self.cleanup_functions.lock().unwrap();
            let taken: Vec<_> = fns.drain(..).collect();
            taken
        };

        // Execute in reverse order
        for cleanup_fn in functions.iter().rev() {
            let fut = cleanup_fn();
            if let Err(e) = tokio::task::spawn(fut).await {
                eprintln!("Error during cleanup: {e}");
            }
        }
    }

    // -- Turn management --

    /// Reset per-turn tracking. Call at turn boundaries.
    pub fn reset_turn(&self) {
        *self.current_turn_injections.lock().unwrap() = 0;
        // Note: cancellation is NOT reset here (persists across turns)
    }

    /// Current injection count for this turn.
    pub fn current_turn_injections(&self) -> usize {
        *self.current_turn_injections.lock().unwrap()
    }

    /// Increment the injection counter.
    pub fn increment_injections(&self, count: usize) {
        *self.current_turn_injections.lock().unwrap() += count;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeContextManager, FakeOrchestrator, FakeProvider, FakeTool};

    // ---------------------------------------------------------------
    // Tool mount/get
    // ---------------------------------------------------------------

    #[test]
    fn mount_and_get_tool() {
        let coord = Coordinator::new_for_test();
        let tool = Arc::new(FakeTool::new("echo", "echoes"));
        coord.mount_tool("echo", tool);
        let retrieved = coord.get_tool("echo").unwrap();
        assert_eq!(retrieved.name(), "echo");
    }

    #[test]
    fn get_tool_returns_none_when_missing() {
        let coord = Coordinator::new_for_test();
        assert!(coord.get_tool("nonexistent").is_none());
    }

    #[test]
    fn get_all_tools_returns_correct_map() {
        let coord = Coordinator::new_for_test();
        let t1 = Arc::new(FakeTool::new("echo", "echoes"));
        let t2 = Arc::new(FakeTool::new("bash", "runs bash"));
        coord.mount_tool("echo", t1);
        coord.mount_tool("bash", t2);

        let all = coord.tools();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("echo"));
        assert!(all.contains_key("bash"));
    }

    #[test]
    fn unmount_removes_tool() {
        let coord = Coordinator::new_for_test();
        let tool = Arc::new(FakeTool::new("echo", "echoes"));
        coord.mount_tool("echo", tool);
        assert!(coord.get_tool("echo").is_some());

        let removed = coord.unmount_tool("echo");
        assert!(removed);
        assert!(coord.get_tool("echo").is_none());
    }

    #[test]
    fn unmount_nonexistent_returns_false() {
        let coord = Coordinator::new_for_test();
        assert!(!coord.unmount_tool("nonexistent"));
    }

    #[test]
    fn tools_empty_initially() {
        let coord = Coordinator::new_for_test();
        assert!(coord.tools().is_empty());
    }

    // ---------------------------------------------------------------
    // Provider mount/get
    // ---------------------------------------------------------------

    #[test]
    fn mount_and_get_provider() {
        let coord = Coordinator::new_for_test();
        let provider = Arc::new(FakeProvider::new("test", "hi"));
        coord.mount_provider("test", provider);
        let retrieved = coord.get_provider("test").unwrap();
        assert_eq!(retrieved.name(), "test");
    }

    #[test]
    fn get_all_providers() {
        let coord = Coordinator::new_for_test();
        let p1 = Arc::new(FakeProvider::new("openai", "hi"));
        let p2 = Arc::new(FakeProvider::new("anthropic", "hello"));
        coord.mount_provider("openai", p1);
        coord.mount_provider("anthropic", p2);

        let all = coord.providers();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn unmount_provider() {
        let coord = Coordinator::new_for_test();
        let provider = Arc::new(FakeProvider::new("test", "hi"));
        coord.mount_provider("test", provider);
        assert!(coord.unmount_provider("test"));
        assert!(coord.get_provider("test").is_none());
    }

    // ---------------------------------------------------------------
    // Orchestrator and ContextManager (single-slot)
    // ---------------------------------------------------------------

    #[test]
    fn orchestrator_none_initially() {
        let coord = Coordinator::new_for_test();
        assert!(coord.orchestrator().is_none());
    }

    #[test]
    fn set_and_get_orchestrator() {
        let coord = Coordinator::new_for_test();
        let orch = Arc::new(FakeOrchestrator::new("ok"));
        coord.set_orchestrator(orch);
        assert!(coord.orchestrator().is_some());
    }

    #[test]
    fn context_none_initially() {
        let coord = Coordinator::new_for_test();
        assert!(coord.context().is_none());
    }

    #[test]
    fn set_and_get_context() {
        let coord = Coordinator::new_for_test();
        let ctx = Arc::new(FakeContextManager::new());
        coord.set_context(ctx);
        assert!(coord.context().is_some());
    }

    // ---------------------------------------------------------------
    // Config
    // ---------------------------------------------------------------

    #[test]
    fn config_access() {
        let mut config = HashMap::new();
        config.insert(
            "session".into(),
            serde_json::json!({"orchestrator": "loop-basic"}),
        );
        let coord = Coordinator::new(config);
        assert_eq!(
            coord.config().get("session"),
            Some(&serde_json::json!({"orchestrator": "loop-basic"}))
        );
    }

    // ---------------------------------------------------------------
    // Capabilities
    // ---------------------------------------------------------------

    #[test]
    fn capability_registration_and_retrieval() {
        let coord = Coordinator::new_for_test();
        coord.register_capability("feature-x", serde_json::json!(true));
        assert_eq!(
            coord.get_capability("feature-x"),
            Some(serde_json::json!(true))
        );
    }

    #[test]
    fn get_capability_returns_none_when_missing() {
        let coord = Coordinator::new_for_test();
        assert_eq!(coord.get_capability("nonexistent"), None);
    }

    // ---------------------------------------------------------------
    // Contribution channels
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn contribution_channels() {
        let coord = Coordinator::new_for_test();
        coord.register_contributor(
            "events",
            "mod-a",
            Box::new(|| Box::pin(async { Ok(serde_json::json!(["event1", "event2"])) })),
        );
        coord.register_contributor(
            "events",
            "mod-b",
            Box::new(|| Box::pin(async { Ok(serde_json::json!(["event3"])) })),
        );
        let results = coord.collect_contributions("events").await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn contribution_empty_channel() {
        let coord = Coordinator::new_for_test();
        let results = coord.collect_contributions("nonexistent").await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn contribution_error_skipped() {
        let coord = Coordinator::new_for_test();
        coord.register_contributor(
            "events",
            "failing",
            Box::new(|| Box::pin(async { Err("contributor failed".into()) })),
        );
        coord.register_contributor(
            "events",
            "succeeding",
            Box::new(|| Box::pin(async { Ok(serde_json::json!("ok")) })),
        );
        let results = coord.collect_contributions("events").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], serde_json::json!("ok"));
    }

    // ---------------------------------------------------------------
    // Cleanup
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn cleanup_runs_in_reverse_order() {
        let order = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let coord = Coordinator::new_for_test();

        let o1 = order.clone();
        coord.register_cleanup(Box::new(move || {
            let o = o1.clone();
            Box::pin(async move {
                o.lock().await.push(1);
            })
        }));
        let o2 = order.clone();
        coord.register_cleanup(Box::new(move || {
            let o = o2.clone();
            Box::pin(async move {
                o.lock().await.push(2);
            })
        }));

        coord.cleanup().await;
        assert_eq!(*order.lock().await, vec![2, 1]); // Reverse order
    }

    // ---------------------------------------------------------------
    // Turn management
    // ---------------------------------------------------------------

    #[test]
    fn reset_turn_resets_injection_count() {
        let coord = Coordinator::new_for_test();
        coord.increment_injections(10);
        assert_eq!(coord.current_turn_injections(), 10);
        coord.reset_turn();
        assert_eq!(coord.current_turn_injections(), 0);
    }

    // ---------------------------------------------------------------
    // Hooks and cancellation accessible
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn hooks_accessible() {
        let coord = Coordinator::new_for_test();
        // Emit on hooks — should return Continue with no handlers
        let result = coord
            .hooks()
            .emit("test:event", serde_json::json!({}))
            .await;
        assert_eq!(result.action, crate::models::HookAction::Continue);
    }

    #[test]
    fn cancellation_token_accessible() {
        let coord = Coordinator::new_for_test();
        assert!(!coord.cancellation().is_cancelled());
        coord.cancellation().request_graceful();
        assert!(coord.cancellation().is_graceful());
    }

    #[test]
    fn to_dict_includes_all_mount_points() {
        let coord = Coordinator::new_for_test();
        let dict = coord.to_dict();
        assert!(dict.contains_key("tools"));
        assert!(dict.contains_key("providers"));
        assert!(dict.contains_key("has_orchestrator"));
        assert!(dict.contains_key("has_context"));
        assert!(dict.contains_key("capabilities"));
    }

    #[test]
    fn to_dict_reflects_mounted_state() {
        let coord = Coordinator::new_for_test();
        let tool = Arc::new(FakeTool::new("echo", "echoes"));
        coord.mount_tool("echo", tool);
        coord.register_capability("streaming", serde_json::json!(true));
        let dict = coord.to_dict();
        let tools = dict["tools"].as_array().unwrap();
        assert!(tools.contains(&serde_json::json!("echo")));
        let caps = dict["capabilities"].as_array().unwrap();
        assert!(caps.contains(&serde_json::json!("streaming")));
    }

    #[tokio::test]
    async fn collect_contributions_logs_on_contributor_error() {
        let coord = Coordinator::new_for_test();

        // Register a contributor that always errors
        coord.register_contributor(
            "test_channel",
            "failing_contributor",
            Box::new(|| Box::pin(async { Err("simulated contributor failure".into()) })),
        );

        // Register a succeeding contributor
        coord.register_contributor(
            "test_channel",
            "good_contributor",
            Box::new(|| Box::pin(async { Ok(serde_json::json!({"key": "value"})) })),
        );

        let results = coord.collect_contributions("test_channel").await;
        // Only the good contributor's result should be present
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], serde_json::json!({"key": "value"}));
    }

    #[tokio::test]
    async fn hooks_shared_returns_arc_to_same_registry() {
        let coord = Coordinator::new_for_test();

        // Obtain shared Arc to the hook registry
        let shared_hooks = coord.hooks_shared();

        // Register a handler on the shared clone
        let handler = Arc::new(crate::testing::FakeHookHandler::new());
        let _ = shared_hooks.register(
            "test:shared",
            handler.clone(),
            0,
            Some("shared-handler".into()),
        );

        // Emit via the original coordinator's hooks() — the handler MUST fire
        // because hooks_shared() returns the same registry, not a copy.
        coord
            .hooks()
            .emit("test:shared", serde_json::json!({"from": "coordinator"}))
            .await;

        let events = handler.recorded_events();
        assert_eq!(
            events.len(),
            1,
            "handler registered on hooks_shared() clone must fire when emitting via hooks()"
        );
        assert_eq!(events[0].0, "test:shared");
    }

    // ---------------------------------------------------------------
    // ApprovalProvider get/set
    // ---------------------------------------------------------------

    #[test]
    fn approval_provider_none_initially() {
        let coord = Coordinator::new_for_test();
        assert!(coord.approval_provider().is_none());
    }

    #[test]
    fn set_and_get_approval_provider() {
        let coord = Coordinator::new_for_test();
        let provider = Arc::new(crate::testing::FakeApprovalProvider::approving());
        coord.set_approval_provider(provider);
        assert!(coord.approval_provider().is_some());
    }

    #[test]
    fn to_dict_includes_has_approval_provider() {
        let coord = Coordinator::new_for_test();
        let dict = coord.to_dict();
        assert_eq!(dict["has_approval_provider"], serde_json::json!(false));

        let provider = Arc::new(crate::testing::FakeApprovalProvider::approving());
        coord.set_approval_provider(provider);
        let dict = coord.to_dict();
        assert_eq!(dict["has_approval_provider"], serde_json::json!(true));
    }

    // ---------------------------------------------------------------
    // DisplayService get/set
    // ---------------------------------------------------------------

    #[test]
    fn display_service_none_initially() {
        let coord = Coordinator::new_for_test();
        assert!(coord.display_service().is_none());
    }

    #[test]
    fn set_and_get_display_service() {
        let coord = Coordinator::new_for_test();
        let display = Arc::new(crate::testing::FakeDisplayService::new());
        coord.set_display_service(display);
        assert!(coord.display_service().is_some());
    }

    #[tokio::test]
    async fn display_service_records_messages() {
        let display = Arc::new(crate::testing::FakeDisplayService::new());
        display.show_message("hello", "info", "test").await.unwrap();
        let messages = display.recorded_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], ("hello".to_string(), "info".to_string(), "test".to_string()));
    }

    #[test]
    fn to_dict_includes_has_display_service() {
        let coord = Coordinator::new_for_test();
        let dict = coord.to_dict();
        assert_eq!(dict["has_display_service"], serde_json::json!(false));

        let display = Arc::new(crate::testing::FakeDisplayService::new());
        coord.set_display_service(display);
        let dict = coord.to_dict();
        assert_eq!(dict["has_display_service"], serde_json::json!(true));
    }
}
