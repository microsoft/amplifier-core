//! AmplifierSession — lifecycle management for agent sessions.
//!
//! The session is the top-level entry point: create → initialize → execute → cleanup.
//! It owns a [`Coordinator`] and manages session identity, status tracking,
//! and event emission.
//!
//! # Design
//!
//! The Python `AmplifierSession` handles both module loading (via `ModuleLoader`)
//! and runtime lifecycle. In Rust, module loading stays in Python (via the PyO3
//! bridge). The Rust session provides the runtime lifecycle after modules are
//! mounted externally.
//!
//! # Connections
//!
//! - Owns a [`Coordinator`](crate::coordinator::Coordinator) for module access.
//! - Emits lifecycle events via [`HookRegistry`](crate::hooks::HookRegistry).
//! - Tracks status via [`SessionState`](crate::models::SessionState).

use std::collections::HashMap;

use serde_json::Value;

use crate::coordinator::Coordinator;
use crate::errors::{AmplifierError, SessionError};
use crate::events;
use crate::models::SessionState;

// ---------------------------------------------------------------------------
// SessionConfig
// ---------------------------------------------------------------------------

/// Configuration for creating an `AmplifierSession`.
///
/// Mirrors the Python config dict with validation for required fields.
#[derive(Debug)]
pub struct SessionConfig {
    /// Full session configuration (the "mount plan").
    pub config: HashMap<String, Value>,
}

impl SessionConfig {
    /// Create a `SessionConfig` from a JSON value, validating required fields.
    ///
    /// Requires `session.orchestrator` and `session.context` to be present.
    pub fn from_value(value: Value) -> Result<Self, SessionError> {
        let obj = match value.as_object() {
            Some(o) => o,
            None => {
                return Err(SessionError::ConfigMissing {
                    field: "config must be a JSON object".into(),
                });
            }
        };

        let session = obj
            .get("session")
            .and_then(|v| v.as_object());

        let has_orchestrator = session
            .and_then(|s| s.get("orchestrator"))
            .is_some();

        if !has_orchestrator {
            return Err(SessionError::ConfigMissing {
                field: "session.orchestrator".into(),
            });
        }

        let has_context = session
            .and_then(|s| s.get("context"))
            .is_some();

        if !has_context {
            return Err(SessionError::ConfigMissing {
                field: "session.context".into(),
            });
        }

        let config: HashMap<String, Value> = obj
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Self { config })
    }

    /// Create a minimal config for testing.
    ///
    /// Sets `session.orchestrator` and `session.context` to the given values.
    pub fn minimal(orchestrator: &str, context: &str) -> Self {
        let mut config = HashMap::new();
        config.insert(
            "session".into(),
            serde_json::json!({
                "orchestrator": orchestrator,
                "context": context,
            }),
        );
        Self { config }
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// An Amplifier session managing the lifecycle of an agent execution.
///
/// # Lifecycle
///
/// 1. **Create** — `Session::new(config, session_id, parent_id)`
/// 2. **Mount modules** — caller mounts orchestrator, context, providers, tools
///    on `coordinator_mut()`
/// 3. **Mark initialized** — `set_initialized()` (or auto-init on execute)
/// 4. **Execute** — `execute(prompt)` runs the orchestrator loop
/// 5. **Cleanup** — `cleanup()` runs cleanup functions
///
/// # Example
///
/// ```rust
/// use amplifier_core::session::{Session, SessionConfig};
///
/// let config = SessionConfig::minimal("loop-basic", "context-simple");
/// let session = Session::new(config, None, None);
/// assert!(!session.session_id().is_empty());
/// ```
pub struct Session {
    session_id: String,
    parent_id: Option<String>,
    coordinator: Coordinator,
    initialized: bool,
    status: SessionState,
    is_resumed: bool,
}

impl Session {
    /// Create a new session.
    ///
    /// # Arguments
    ///
    /// * `config` — Session configuration (mount plan).
    /// * `session_id` — Optional session ID. If `None`, a UUID v4 is generated.
    /// * `parent_id` — Optional parent session ID (for child/forked sessions).
    pub fn new(
        config: SessionConfig,
        session_id: Option<String>,
        parent_id: Option<String>,
    ) -> Self {
        let id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let coordinator = Coordinator::new(config.config);

        // Set default fields for all hook events
        coordinator.hooks().set_default_fields(serde_json::json!({
            "session_id": id,
            "parent_id": parent_id,
        }));

        Self {
            session_id: id,
            parent_id,
            coordinator,
            initialized: false,
            status: SessionState::Running,
            is_resumed: false,
        }
    }

    /// Create a session that is marked as resumed (emits session:resume instead of session:start).
    pub fn new_resumed(
        config: SessionConfig,
        session_id: String,
        parent_id: Option<String>,
    ) -> Self {
        let mut session = Self::new(config, Some(session_id), parent_id);
        session.is_resumed = true;
        session
    }

    /// The session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// The parent session ID (if this is a child session).
    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    /// Current session status as a string (matching Python's status field).
    pub fn status(&self) -> &str {
        match &self.status {
            SessionState::Running => "running",
            SessionState::Completed => "completed",
            SessionState::Failed => "failed",
            SessionState::Cancelled => "cancelled",
        }
    }

    /// Current session state enum.
    pub fn state(&self) -> &SessionState {
        &self.status
    }

    /// Whether the session has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Immutable reference to the coordinator.
    pub fn coordinator(&self) -> &Coordinator {
        &self.coordinator
    }

    /// Mutable reference to the coordinator (for mounting modules).
    pub fn coordinator_mut(&mut self) -> &mut Coordinator {
        &mut self.coordinator
    }

    /// Mark the session as initialized.
    ///
    /// In the Rust kernel, module loading is done externally (by the Python
    /// bridge or test harness). This method marks the session ready for
    /// execution after modules have been mounted.
    pub fn set_initialized(&mut self) {
        self.initialized = true;
    }

    /// Clear the initialized flag (used during cleanup).
    ///
    /// After cleanup, the session is no longer ready for execution.
    pub fn clear_initialized(&mut self) {
        self.initialized = false;
    }

    /// Execute a prompt using the mounted orchestrator.
    ///
    /// Auto-emits `session:start` (or `session:resume`) event, then delegates
    /// to the orchestrator. Tracks status transitions on success, failure,
    /// or cancellation.
    ///
    /// # Errors
    ///
    /// - `SessionError::NotInitialized` if not initialized
    /// - `SessionError::Other("No orchestrator mounted")` if no orchestrator
    /// - `SessionError::Other("No context manager mounted")` if no context
    /// - `SessionError::Other("No providers mounted")` if providers map is empty
    /// - Any `AmplifierError` from the orchestrator
    pub async fn execute(&mut self, prompt: &str) -> Result<String, AmplifierError> {
        if !self.initialized {
            return Err(AmplifierError::Session(SessionError::NotInitialized));
        }

        // Emit lifecycle event
        let event = if self.is_resumed {
            events::SESSION_RESUME
        } else {
            events::SESSION_START
        };

        self.coordinator
            .hooks()
            .emit(
                event,
                serde_json::json!({
                    "session_id": self.session_id,
                    "parent_id": self.parent_id,
                }),
            )
            .await;

        // Get orchestrator
        let orchestrator = self.coordinator.orchestrator().ok_or_else(|| {
            AmplifierError::Session(SessionError::Other {
                message: "No orchestrator mounted".into(),
            })
        })?;

        // Get context
        let context = self.coordinator.context().ok_or_else(|| {
            AmplifierError::Session(SessionError::Other {
                message: "No context manager mounted".into(),
            })
        })?;

        // Get providers
        let providers = self.coordinator.providers();
        if providers.is_empty() {
            return Err(AmplifierError::Session(SessionError::Other {
                message: "No providers mounted".into(),
            }));
        }

        // Get tools
        let tools = self.coordinator.tools();

        // Execute orchestrator
        self.status = SessionState::Running;

        match orchestrator
            .execute(
                prompt.to_string(),
                context,
                providers,
                tools,
                serde_json::json!({}), // hooks placeholder (serialised)
                serde_json::json!({}), // coordinator placeholder (serialised)
            )
            .await
        {
            Ok(result) => {
                // Check cancellation
                if self.coordinator.cancellation().is_cancelled() {
                    self.status = SessionState::Cancelled;
                } else {
                    self.status = SessionState::Completed;
                }
                Ok(result)
            }
            Err(e) => {
                if self.coordinator.cancellation().is_cancelled() {
                    self.status = SessionState::Cancelled;
                } else {
                    self.status = SessionState::Failed;
                }
                Err(e)
            }
        }
    }

    /// Clean up session resources.
    ///
    /// Emits `session:end` event and runs all cleanup functions registered
    /// on the coordinator.
    pub async fn cleanup(&self) {
        // Emit session:end event
        self.coordinator
            .hooks()
            .emit(
                events::SESSION_END,
                serde_json::json!({
                    "session_id": self.session_id,
                    "status": self.status(),
                }),
            )
            .await;

        // Run coordinator cleanup
        self.coordinator.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::testing::{
        FakeContextManager, FakeHookHandler, FakeOrchestrator, FakeProvider, FakeTool,
    };

    // ---------------------------------------------------------------
    // SessionConfig validation
    // ---------------------------------------------------------------

    #[test]
    fn session_config_requires_orchestrator() {
        let config = serde_json::json!({
            "session": {
                "context": "context-simple"
            }
        });
        let err = SessionConfig::from_value(config).unwrap_err();
        assert!(err.to_string().contains("orchestrator"));
    }

    #[test]
    fn session_config_requires_context() {
        let config = serde_json::json!({
            "session": {
                "orchestrator": "loop-basic"
            }
        });
        let err = SessionConfig::from_value(config).unwrap_err();
        assert!(err.to_string().contains("context"));
    }

    #[test]
    fn session_config_valid() {
        let config = serde_json::json!({
            "session": {
                "orchestrator": "loop-basic",
                "context": "context-simple"
            }
        });
        let result = SessionConfig::from_value(config);
        assert!(result.is_ok());
    }

    // ---------------------------------------------------------------
    // Session creation
    // ---------------------------------------------------------------

    #[test]
    fn session_generates_uuid_if_not_provided() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let session = Session::new(config, None, None);
        assert!(!session.session_id().is_empty());
        // Should be valid UUID format
        assert!(uuid::Uuid::parse_str(session.session_id()).is_ok());
    }

    #[test]
    fn session_uses_provided_id() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let session = Session::new(config, Some("custom-id".into()), None);
        assert_eq!(session.session_id(), "custom-id");
    }

    #[test]
    fn session_with_parent_id() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let session = Session::new(config, None, Some("parent-123".into()));
        assert_eq!(session.parent_id(), Some("parent-123"));
    }

    #[test]
    fn session_without_parent_id() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let session = Session::new(config, None, None);
        assert_eq!(session.parent_id(), None);
    }

    #[test]
    fn session_initial_status_is_running() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let session = Session::new(config, None, None);
        assert_eq!(session.status(), "running");
        assert_eq!(*session.state(), SessionState::Running);
    }

    #[test]
    fn session_not_initialized_by_default() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let session = Session::new(config, None, None);
        assert!(!session.is_initialized());
    }

    // ---------------------------------------------------------------
    // Execute — gating checks
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn execute_fails_when_not_initialized() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);

        let result = session.execute("hello").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not initialized"));
    }

    #[tokio::test]
    async fn execute_fails_without_orchestrator() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);
        // Mount context and provider but NOT orchestrator
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));
        session.set_initialized();

        let result = session.execute("hello").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("orchestrator"));
    }

    #[tokio::test]
    async fn execute_fails_without_context() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);
        // Mount orchestrator and provider but NOT context
        session
            .coordinator_mut()
            .set_orchestrator(Arc::new(FakeOrchestrator::new("ok")));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));
        session.set_initialized();

        let result = session.execute("hello").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("context"));
    }

    #[tokio::test]
    async fn execute_fails_without_providers() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);
        // Mount orchestrator and context but NO providers
        session
            .coordinator_mut()
            .set_orchestrator(Arc::new(FakeOrchestrator::new("ok")));
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session.set_initialized();

        let result = session.execute("hello").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("provider") || err_msg.contains("No providers"),
            "Expected error about providers, got: {err_msg}"
        );
    }

    // ---------------------------------------------------------------
    // Execute — success path
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn execute_delegates_to_orchestrator() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);
        session
            .coordinator_mut()
            .set_orchestrator(Arc::new(FakeOrchestrator::new("orchestrated response")));
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));
        session.set_initialized();

        let result = session.execute("hello").await.unwrap();
        assert_eq!(result, "orchestrated response");
        assert_eq!(session.status(), "completed");
    }

    // ---------------------------------------------------------------
    // Status transitions
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn status_transitions_to_completed_on_success() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);
        session
            .coordinator_mut()
            .set_orchestrator(Arc::new(FakeOrchestrator::new("ok")));
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));
        session.set_initialized();

        let _ = session.execute("hello").await;
        assert_eq!(*session.state(), SessionState::Completed);
    }

    #[tokio::test]
    async fn status_transitions_to_cancelled_when_cancelled() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);
        session
            .coordinator_mut()
            .set_orchestrator(Arc::new(FakeOrchestrator::new("ok")));
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));
        session.set_initialized();

        // Request cancellation before execute
        session.coordinator().cancellation().request_graceful();

        let _ = session.execute("hello").await;
        assert_eq!(*session.state(), SessionState::Cancelled);
    }

    // ---------------------------------------------------------------
    // Hook events
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn execute_emits_session_start_event() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);
        session
            .coordinator_mut()
            .set_orchestrator(Arc::new(FakeOrchestrator::new("ok")));
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));

        // Register a hook handler to capture events
        let handler = Arc::new(FakeHookHandler::new());
        session.coordinator().hooks().register(
            events::SESSION_START,
            handler.clone(),
            0,
            Some("test-handler".into()),
        );

        session.set_initialized();
        let _ = session.execute("hello").await;

        let events = handler.recorded_events();
        assert!(
            events.iter().any(|(name, _)| name == events::SESSION_START),
            "Expected session:start event, got: {:?}",
            events.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn execute_emits_session_resume_for_resumed_session() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new_resumed(config, "resumed-id".into(), None);
        session
            .coordinator_mut()
            .set_orchestrator(Arc::new(FakeOrchestrator::new("ok")));
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));

        let handler = Arc::new(FakeHookHandler::new());
        session.coordinator().hooks().register(
            events::SESSION_RESUME,
            handler.clone(),
            0,
            Some("test-handler".into()),
        );

        session.set_initialized();
        let _ = session.execute("hello").await;

        let events = handler.recorded_events();
        assert!(
            events
                .iter()
                .any(|(name, _)| name == events::SESSION_RESUME),
            "Expected session:resume event, got: {:?}",
            events.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn cleanup_emits_session_end_event() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let session = Session::new(config, None, None);

        let handler = Arc::new(FakeHookHandler::new());
        session.coordinator().hooks().register(
            events::SESSION_END,
            handler.clone(),
            0,
            Some("test-handler".into()),
        );

        session.cleanup().await;

        let events = handler.recorded_events();
        assert!(
            events.iter().any(|(name, _)| name == events::SESSION_END),
            "Expected session:end event, got: {:?}",
            events.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
    }

    // ---------------------------------------------------------------
    // Coordinator access
    // ---------------------------------------------------------------

    #[test]
    fn coordinator_is_accessible() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);

        // Mount tool via coordinator
        session
            .coordinator_mut()
            .mount_tool("echo", Arc::new(FakeTool::new("echo", "echoes")));

        // Verify via immutable access
        let tools = session.coordinator().tools();
        assert_eq!(tools.len(), 1);
        assert!(tools.contains_key("echo"));
    }
}
