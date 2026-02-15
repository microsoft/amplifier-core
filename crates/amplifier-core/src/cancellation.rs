//! Cancellation primitives for cooperative session cancellation.
//!
//! The kernel provides the MECHANISM (token with state).
//! The app layer provides the POLICY (when to cancel).
//!
//! # State Machine
//!
//! ```text
//! None ──→ Graceful ──→ Immediate
//!   │                       ↑
//!   └───────────────────────┘
//! ```
//!
//! - `None` → running normally
//! - `Graceful` → waiting for current tools to complete (1st Ctrl+C)
//! - `Immediate` → stop now, synthesise results (2nd Ctrl+C or timeout)
//!
//! # Connections
//!
//! - Lives inside `Coordinator` (future `crate::coordinator`).
//! - Orchestrators and tools check `is_cancelled` / `is_graceful` /
//!   `is_immediate` to decide how to respond.
//! - Child tokens propagate parent cancellation to forked sessions.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// CancellationState
// ---------------------------------------------------------------------------

/// Cancellation state machine states.
///
/// Matches Python's `CancellationState(Enum)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationState {
    /// Running normally.
    #[default]
    None,
    /// Waiting for current tools to complete (graceful shutdown).
    Graceful,
    /// Stop now, synthesise results for pending tools.
    Immediate,
}


// ---------------------------------------------------------------------------
// Callback type alias
// ---------------------------------------------------------------------------

/// An async cancellation callback: `() -> Future<Output = ()>`.
///
/// Stored in the token and triggered via [`CancellationToken::trigger_callbacks`].
pub type CancelCallback =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

// ---------------------------------------------------------------------------
// Inner state (behind Mutex)
// ---------------------------------------------------------------------------

/// Interior mutable state for [`CancellationToken`].
struct Inner {
    state: CancellationState,
    running_tools: HashSet<String>,
    running_tool_names: HashMap<String, String>,
    child_tokens: Vec<CancellationToken>,
    on_cancel_callbacks: Vec<CancelCallback>,
}

impl Inner {
    fn new() -> Self {
        Self {
            state: CancellationState::None,
            running_tools: HashSet::new(),
            running_tool_names: HashMap::new(),
            child_tokens: Vec::new(),
            on_cancel_callbacks: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// CancellationToken
// ---------------------------------------------------------------------------

/// Cancellation token for cooperative cancellation.
///
/// Lives in `ModuleCoordinator`. Orchestrators and tools check this
/// to determine if they should stop.
///
/// Thread-safe: all access goes through an `Arc<Mutex<Inner>>`, so
/// the token can be shared across `tokio::spawn` boundaries.
///
/// # Example
///
/// ```rust
/// use amplifier_core::cancellation::CancellationToken;
///
/// let token = CancellationToken::new();
/// assert!(!token.is_cancelled());
///
/// token.request_graceful();
/// assert!(token.is_graceful());
/// ```
#[derive(Clone)]
pub struct CancellationToken {
    inner: Arc<Mutex<Inner>>,
}

impl CancellationToken {
    /// Create a new token in the `None` state.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::new())),
        }
    }

    /// Current cancellation state.
    pub fn state(&self) -> CancellationState {
        self.inner.lock().unwrap().state
    }

    /// `true` if any cancellation requested (graceful or immediate).
    pub fn is_cancelled(&self) -> bool {
        self.inner.lock().unwrap().state != CancellationState::None
    }

    /// `true` if graceful cancellation (wait for tools).
    pub fn is_graceful(&self) -> bool {
        self.inner.lock().unwrap().state == CancellationState::Graceful
    }

    /// `true` if immediate cancellation (stop now).
    pub fn is_immediate(&self) -> bool {
        self.inner.lock().unwrap().state == CancellationState::Immediate
    }

    /// Request graceful cancellation. Waits for current tools to complete.
    ///
    /// Returns `true` if state changed, `false` if already cancelled.
    pub fn request_graceful(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.state == CancellationState::None {
            inner.state = CancellationState::Graceful;
            // Propagate to children while still holding lock on our state,
            // but we must clone children to avoid holding two locks.
            let children: Vec<CancellationToken> = inner.child_tokens.clone();
            drop(inner);
            for child in &children {
                child.request_graceful();
            }
            true
        } else {
            false
        }
    }

    /// Request immediate cancellation. Stops as soon as possible.
    ///
    /// Returns `true` if state changed.
    pub fn request_immediate(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.state != CancellationState::Immediate {
            inner.state = CancellationState::Immediate;
            let children: Vec<CancellationToken> = inner.child_tokens.clone();
            drop(inner);
            for child in &children {
                child.request_immediate();
            }
            true
        } else {
            false
        }
    }

    /// Reset cancellation state. Called when starting a new turn.
    ///
    /// Clears state and running tools but preserves child tokens and callbacks
    /// (those are session-level, matching Python behaviour).
    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.state = CancellationState::None;
        inner.running_tools.clear();
        inner.running_tool_names.clear();
    }

    // -- Tool tracking ---

    /// Register a tool as starting execution.
    pub fn register_tool_start(&self, tool_call_id: &str, tool_name: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.running_tools.insert(tool_call_id.to_string());
        inner
            .running_tool_names
            .insert(tool_call_id.to_string(), tool_name.to_string());
    }

    /// Register a tool as completed.
    pub fn register_tool_complete(&self, tool_call_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.running_tools.remove(tool_call_id);
        inner.running_tool_names.remove(tool_call_id);
    }

    /// Currently running tool call IDs (snapshot).
    pub fn running_tools(&self) -> HashSet<String> {
        self.inner.lock().unwrap().running_tools.clone()
    }

    /// Names of currently running tools (for display).
    pub fn running_tool_names(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap()
            .running_tool_names
            .values()
            .cloned()
            .collect()
    }

    // -- Child propagation ---

    /// Register a child session's token for propagation.
    ///
    /// If the parent is already cancelled, the child inherits that state
    /// immediately.
    pub fn register_child(&self, child: CancellationToken) {
        let inner = self.inner.lock().unwrap();
        let current_state = inner.state;
        drop(inner);

        // Propagate current state to new child
        match current_state {
            CancellationState::Graceful => {
                child.request_graceful();
            }
            CancellationState::Immediate => {
                child.request_immediate();
            }
            CancellationState::None => {}
        }

        self.inner.lock().unwrap().child_tokens.push(child);
    }

    /// Unregister a child session's token.
    pub fn unregister_child(&self, child: &CancellationToken) {
        let mut inner = self.inner.lock().unwrap();
        inner.child_tokens.retain(|c| !Arc::ptr_eq(&c.inner, &child.inner));
    }

    // -- Callbacks ---

    /// Register callback to be called on cancellation.
    pub fn on_cancel(&self, callback: CancelCallback) {
        self.inner.lock().unwrap().on_cancel_callbacks.push(callback);
    }

    /// Trigger all registered cancellation callbacks.
    ///
    /// Errors (including panics) in one callback do not prevent subsequent
    /// callbacks from executing, matching the Python behaviour.
    pub async fn trigger_callbacks(&self) {
        // Take a snapshot of callbacks to avoid holding the lock during async calls.
        let callbacks: Vec<_> = {
            let inner = self.inner.lock().unwrap();
            inner
                .on_cancel_callbacks
                .iter()
                .map(|cb| cb())
                .collect()
        };

        for fut in callbacks {
            // Catch panics so one failing callback doesn't prevent others.
            let result = tokio::task::spawn(fut).await;
            if let Err(e) = result {
                // Log but continue — matches Python's `except Exception: pass`
                eprintln!("Error in cancellation callback: {e}");
            }
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // ---------------------------------------------------------------
    // State machine basics
    // ---------------------------------------------------------------

    #[test]
    fn initial_state_is_none() {
        let token = CancellationToken::new();
        assert_eq!(token.state(), CancellationState::None);
        assert!(!token.is_cancelled());
        assert!(!token.is_graceful());
        assert!(!token.is_immediate());
    }

    #[test]
    fn graceful_transitions_from_none() {
        let token = CancellationToken::new();
        assert!(token.request_graceful());
        assert_eq!(token.state(), CancellationState::Graceful);
        assert!(token.is_cancelled());
        assert!(token.is_graceful());
        assert!(!token.is_immediate());
    }

    #[test]
    fn graceful_is_noop_when_already_graceful() {
        let token = CancellationToken::new();
        assert!(token.request_graceful());
        // Second call returns false — no state change
        assert!(!token.request_graceful());
        assert_eq!(token.state(), CancellationState::Graceful);
    }

    #[test]
    fn immediate_transitions_from_graceful() {
        let token = CancellationToken::new();
        token.request_graceful();
        assert!(token.request_immediate());
        assert_eq!(token.state(), CancellationState::Immediate);
        assert!(token.is_cancelled());
        assert!(token.is_immediate());
    }

    #[test]
    fn immediate_transitions_from_none() {
        let token = CancellationToken::new();
        assert!(token.request_immediate());
        assert_eq!(token.state(), CancellationState::Immediate);
        assert!(token.is_cancelled());
        assert!(token.is_immediate());
    }

    #[test]
    fn immediate_is_noop_when_already_immediate() {
        let token = CancellationToken::new();
        token.request_immediate();
        assert!(!token.request_immediate());
    }

    // ---------------------------------------------------------------
    // Reset
    // ---------------------------------------------------------------

    #[test]
    fn reset_returns_to_none() {
        let token = CancellationToken::new();
        token.request_graceful();
        token.reset();
        assert_eq!(token.state(), CancellationState::None);
        assert!(!token.is_cancelled());
    }

    #[test]
    fn reset_clears_running_tools() {
        let token = CancellationToken::new();
        token.register_tool_start("tc_1", "bash");
        assert!(!token.running_tools().is_empty());
        token.reset();
        assert!(token.running_tools().is_empty());
        assert!(token.running_tool_names().is_empty());
    }

    // ---------------------------------------------------------------
    // Tool tracking
    // ---------------------------------------------------------------

    #[test]
    fn tool_tracking() {
        let token = CancellationToken::new();
        token.register_tool_start("tc_1", "bash");
        assert!(token.running_tools().contains("tc_1"));
        assert!(token.running_tool_names().contains(&"bash".to_string()));

        token.register_tool_complete("tc_1");
        assert!(token.running_tools().is_empty());
        assert!(token.running_tool_names().is_empty());
    }

    #[test]
    fn complete_unknown_tool_is_noop() {
        let token = CancellationToken::new();
        // Should not panic
        token.register_tool_complete("nonexistent");
    }

    // ---------------------------------------------------------------
    // Child propagation
    // ---------------------------------------------------------------

    #[test]
    fn child_propagation_graceful() {
        let parent = CancellationToken::new();
        let child = CancellationToken::new();
        parent.register_child(child.clone());

        parent.request_graceful();
        assert!(child.is_graceful());
    }

    #[test]
    fn child_propagation_immediate() {
        let parent = CancellationToken::new();
        let child = CancellationToken::new();
        parent.register_child(child.clone());

        parent.request_immediate();
        assert!(child.is_immediate());
    }

    #[test]
    fn child_inherits_current_state_on_register() {
        let parent = CancellationToken::new();
        parent.request_graceful();

        let child = CancellationToken::new();
        parent.register_child(child.clone());
        // Child should inherit parent's current state
        assert!(child.is_graceful());
    }

    #[test]
    fn unregister_child_stops_propagation() {
        let parent = CancellationToken::new();
        let child = CancellationToken::new();
        parent.register_child(child.clone());
        parent.unregister_child(&child);

        parent.request_graceful();
        assert!(!child.is_cancelled()); // Not propagated
    }

    // ---------------------------------------------------------------
    // Cancellation callbacks
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn cancellation_callbacks_fire() {
        let token = CancellationToken::new();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        token.on_cancel(Box::new(move || {
            let c = called_clone.clone();
            Box::pin(async move {
                c.store(true, Ordering::SeqCst);
            })
        }));
        token.request_graceful();
        token.trigger_callbacks().await;
        assert!(called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn callback_errors_do_not_prevent_others() {
        let token = CancellationToken::new();

        // First callback panics
        token.on_cancel(Box::new(|| {
            Box::pin(async {
                panic!("callback error");
            })
        }));

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        token.on_cancel(Box::new(move || {
            let c = called_clone.clone();
            Box::pin(async move {
                c.store(true, Ordering::SeqCst);
            })
        }));

        token.request_graceful();
        token.trigger_callbacks().await;
        // Second callback should still run
        assert!(called.load(Ordering::SeqCst));
    }

    // ---------------------------------------------------------------
    // Thread safety
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn concurrent_access_is_safe() {
        let token = CancellationToken::new();
        let mut handles = Vec::new();

        // Spawn multiple tasks that read state concurrently
        for _ in 0..10 {
            let t = token.clone();
            handles.push(tokio::spawn(async move {
                let _ = t.is_cancelled();
                let _ = t.state();
            }));
        }

        // Request cancellation from another task
        let t = token.clone();
        handles.push(tokio::spawn(async move {
            t.request_graceful();
        }));

        for h in handles {
            h.await.unwrap();
        }

        // Token should be in graceful state
        assert!(token.is_cancelled());
    }
}
