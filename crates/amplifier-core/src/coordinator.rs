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
use crate::traits::{ContextManager, Orchestrator, Provider, Tool};

// ---------------------------------------------------------------------------
// Type aliases for cleanup and contributor callbacks
// ---------------------------------------------------------------------------

/// An async cleanup function: `() -> Future<Output = ()>`.
pub type CleanupFn = Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// An async contributor callback: `() -> Future<Output = Result<Value, ...>>`.
pub type ContributorCallback = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Value, Box<dyn std::error::Error + Send + Sync>>> + Send>>
        + Send
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
    hooks: HookRegistry,
    cancellation: CancellationToken,

    // -- Capabilities & contributions --
    capabilities: Mutex<HashMap<String, Value>>,
    channels: Mutex<HashMap<String, Vec<ContributorEntry>>>,

    // -- Cleanup --
    cleanup_functions: Mutex<Vec<CleanupFn>>,

    // -- Config --
    config: HashMap<String, Value>,

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
            hooks: HookRegistry::new(),
            cancellation: CancellationToken::new(),
            capabilities: Mutex::new(HashMap::new()),
            channels: Mutex::new(HashMap::new()),
            cleanup_functions: Mutex::new(Vec::new()),
            config,
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
        self.tools
            .lock()
            .unwrap()
            .insert(name.to_string(), tool);
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

    // -- Subsystem accessors --

    /// Reference to the hook registry.
    pub fn hooks(&self) -> &HookRegistry {
        &self.hooks
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
    pub fn register_contributor(
        &self,
        channel: &str,
        name: &str,
        callback: ContributorCallback,
    ) {
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
        for (_name, fut) in entries {
            match fut.await {
                Ok(value) => results.push(value),
                Err(_e) => {
                    // Log and skip, matching Python behaviour
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
    use crate::testing::{
        FakeContextManager, FakeOrchestrator, FakeProvider, FakeTool,
    };

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
            Box::new(|| {
                Box::pin(async {
                    Err("contributor failed".into())
                })
            }),
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
}
