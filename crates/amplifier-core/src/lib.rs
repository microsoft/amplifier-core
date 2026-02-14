//! amplifier-core: Pure Rust kernel for modular AI agent orchestration.
//!
//! This crate contains the core coordination engine for the Amplifier
//! ecosystem. It has ZERO Python dependency — it can be consumed from
//! any language via bindings.
//!
//! # Crate Organization
//!
//! - `events` — Canonical event name constants
//! - `errors` — All error types (AmplifierError, ProviderError, etc.)
//! - `models` — Core data models (HookResult, ToolResult, ModelInfo, etc.)
//! - `messages` — Chat protocol models (ChatRequest, ChatResponse, Message, etc.)
//! - `traits` — Module contracts (Tool, Provider, Orchestrator, etc.)
//! - `cancellation` — CancellationToken state machine
//! - `hooks` — HookRegistry event dispatch pipeline
//! - `coordinator` — ModuleCoordinator mount points and capabilities
//! - `session` — AmplifierSession lifecycle management

pub mod events;
pub mod errors;
pub mod models;
pub mod messages;
pub mod traits;
pub mod testing;
pub mod cancellation;
pub mod hooks;
pub mod coordinator;
pub mod session;

// ---------------------------------------------------------------------------
// Re-exports — consumers write `use amplifier_core::Tool`, not
// `use amplifier_core::traits::Tool`.
// ---------------------------------------------------------------------------

// Traits (module contracts)
pub use traits::{ApprovalProvider, ContextManager, HookHandler, Orchestrator, Provider, Tool};

// Error types
pub use errors::{AmplifierError, ContextError, HookError, ProviderError, SessionError, ToolError};

// Core data models
pub use models::{
    ApprovalDefault, ApprovalRequest, ApprovalResponse, ConfigField, ConfigFieldType,
    ContextInjectionRole, HookAction, HookResult, ModelInfo, ModuleInfo, ModuleType, ProviderInfo,
    SessionState, SessionStatus, ToolResult, UserMessageLevel,
};

// Chat protocol models
pub use messages::{
    ChatRequest, ChatResponse, ContentBlock, ContentBlockType, Degradation, Message,
    MessageContent, ResponseFormat, Role, ToolCall, ToolChoice, ToolSpec, Usage, Visibility,
};

// Cancellation
pub use cancellation::{CancellationState, CancellationToken};

// Hooks
pub use hooks::HookRegistry;

// Coordinator
pub use coordinator::Coordinator;

// Session
pub use session::{Session, SessionConfig};

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        assert!(true);
    }

    /// Verify all key types are accessible at the crate root via re-exports.
    ///
    /// Consumers should write `use amplifier_core::Tool`, not
    /// `use amplifier_core::traits::Tool`.
    #[test]
    fn reexports_available_at_crate_root() {
        // Traits
        fn _tool(_: std::sync::Arc<dyn crate::Tool>) {}
        fn _provider(_: std::sync::Arc<dyn crate::Provider>) {}
        fn _orchestrator(_: std::sync::Arc<dyn crate::Orchestrator>) {}
        fn _context(_: std::sync::Arc<dyn crate::ContextManager>) {}
        fn _hook(_: std::sync::Arc<dyn crate::HookHandler>) {}
        fn _approval(_: std::sync::Arc<dyn crate::ApprovalProvider>) {}

        // Error types
        let _: fn() -> crate::AmplifierError = || {
            crate::AmplifierError::Session(crate::SessionError::NotInitialized)
        };
        let _: fn() -> crate::ProviderError = || crate::ProviderError::Timeout {
            message: "t".into(),
            provider: None,
        };
        let _: fn() -> crate::ToolError = || crate::ToolError::Other {
            message: "e".into(),
        };
        let _: fn() -> crate::HookError = || crate::HookError::Other {
            message: "e".into(),
        };
        let _: fn() -> crate::ContextError = || crate::ContextError::Other {
            message: "e".into(),
        };

        // Models
        let _ = crate::HookResult::default();
        let _ = crate::ToolResult::default();
        let _ = crate::HookAction::Continue;

        // Messages
        let _ = crate::Role::User;

        // Events
        let _ = crate::events::SESSION_START;
    }
}
