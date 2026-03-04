//! # amplifier-core Node.js bindings (Napi-RS)
//!
//! This module defines the FFI type contract between Rust and Node.js.
//! The enums and structs here are the authoritative boundary types — keep
//! the `From` impls in sync whenever upstream `amplifier_core::models` changes.
//!
//! Planned classes:
//!
//! | Rust struct       | JS class             |
//! |-------------------|----------------------|
//! | Session           | JsSession            |
//! | HookRegistry      | JsHookRegistry       |
//! | CancellationToken | JsCancellationToken  |
//! | Coordinator       | JsCoordinator        |

#[macro_use]
extern crate napi_derive;

use amplifier_core::models as core_models;

#[napi]
pub fn hello() -> String {
    "Hello from amplifier-core native addon!".to_string()
}

// ---------------------------------------------------------------------------
// Enums — exported as TypeScript string unions via #[napi(string_enum)]
// ---------------------------------------------------------------------------

#[napi(string_enum)]
pub enum HookAction {
    Continue,
    Deny,
    Modify,
    InjectContext,
    AskUser,
}

#[napi(string_enum)]
pub enum SessionState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[napi(string_enum)]
pub enum ContextInjectionRole {
    System,
    User,
    Assistant,
}

#[napi(string_enum)]
pub enum ApprovalDefault {
    Allow,
    Deny,
}

#[napi(string_enum)]
pub enum UserMessageLevel {
    Info,
    Warning,
    Error,
}

#[napi(string_enum)]
pub enum Role {
    System,
    Developer,
    User,
    Assistant,
    Function,
    Tool,
}

// ---------------------------------------------------------------------------
// Bidirectional From conversions: HookAction <-> amplifier_core::models::HookAction
// ---------------------------------------------------------------------------

impl From<core_models::HookAction> for HookAction {
    fn from(action: core_models::HookAction) -> Self {
        match action {
            core_models::HookAction::Continue => HookAction::Continue,
            core_models::HookAction::Deny => HookAction::Deny,
            core_models::HookAction::Modify => HookAction::Modify,
            core_models::HookAction::InjectContext => HookAction::InjectContext,
            core_models::HookAction::AskUser => HookAction::AskUser,
        }
    }
}

impl From<HookAction> for core_models::HookAction {
    fn from(action: HookAction) -> Self {
        match action {
            HookAction::Continue => core_models::HookAction::Continue,
            HookAction::Deny => core_models::HookAction::Deny,
            HookAction::Modify => core_models::HookAction::Modify,
            HookAction::InjectContext => core_models::HookAction::InjectContext,
            HookAction::AskUser => core_models::HookAction::AskUser,
        }
    }
}

// ---------------------------------------------------------------------------
// Bidirectional From conversions: SessionState <-> amplifier_core::models::SessionState
// ---------------------------------------------------------------------------

impl From<core_models::SessionState> for SessionState {
    fn from(state: core_models::SessionState) -> Self {
        match state {
            core_models::SessionState::Running => SessionState::Running,
            core_models::SessionState::Completed => SessionState::Completed,
            core_models::SessionState::Failed => SessionState::Failed,
            core_models::SessionState::Cancelled => SessionState::Cancelled,
        }
    }
}

impl From<SessionState> for core_models::SessionState {
    fn from(state: SessionState) -> Self {
        match state {
            SessionState::Running => core_models::SessionState::Running,
            SessionState::Completed => core_models::SessionState::Completed,
            SessionState::Failed => core_models::SessionState::Failed,
            SessionState::Cancelled => core_models::SessionState::Cancelled,
        }
    }
}

// ---------------------------------------------------------------------------
// Structs — exported as TypeScript interfaces via #[napi(object)]
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct JsToolResult {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[napi(object)]
pub struct JsToolSpec {
    pub name: String,
    pub description: Option<String>,
    pub parameters_json: String,
}

#[napi(object)]
pub struct JsHookResult {
    pub action: HookAction,
    pub reason: Option<String>,
    pub context_injection: Option<String>,
    pub context_injection_role: Option<ContextInjectionRole>,
    pub ephemeral: Option<bool>,
    pub suppress_output: Option<bool>,
    pub user_message: Option<String>,
    pub user_message_level: Option<UserMessageLevel>,
    pub user_message_source: Option<String>,
    pub approval_prompt: Option<String>,
    pub approval_timeout: Option<f64>,
    pub approval_default: Option<ApprovalDefault>,
}

#[napi(object)]
pub struct JsSessionConfig {
    pub config_json: String,
}

// ---------------------------------------------------------------------------
// Classes — exported as TypeScript classes via #[napi]
// ---------------------------------------------------------------------------

/// Wraps `amplifier_core::CancellationToken` for Node.js.
///
/// State machine: None → Graceful → Immediate, with reset back to None.
#[napi]
pub struct JsCancellationToken {
    inner: amplifier_core::CancellationToken,
}

#[napi]
impl JsCancellationToken {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: amplifier_core::CancellationToken::new(),
        }
    }

    /// Internal factory for wrapping an existing kernel token.
    pub fn from_inner(inner: amplifier_core::CancellationToken) -> Self {
        Self { inner }
    }

    #[napi(getter)]
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    #[napi(getter)]
    pub fn is_graceful(&self) -> bool {
        self.inner.is_graceful()
    }

    #[napi(getter)]
    pub fn is_immediate(&self) -> bool {
        self.inner.is_immediate()
    }

    #[napi]
    pub fn request_graceful(&self, _reason: Option<String>) {
        self.inner.request_graceful();
    }

    #[napi]
    pub fn request_immediate(&self, _reason: Option<String>) {
        self.inner.request_immediate();
    }

    #[napi]
    pub fn reset(&self) {
        self.inner.reset();
    }
}
