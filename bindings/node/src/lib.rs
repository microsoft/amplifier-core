//! # amplifier-core Node.js bindings (Napi-RS)
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

impl From<amplifier_core::models::HookAction> for HookAction {
    fn from(action: amplifier_core::models::HookAction) -> Self {
        match action {
            amplifier_core::models::HookAction::Continue => HookAction::Continue,
            amplifier_core::models::HookAction::Deny => HookAction::Deny,
            amplifier_core::models::HookAction::Modify => HookAction::Modify,
            amplifier_core::models::HookAction::InjectContext => HookAction::InjectContext,
            amplifier_core::models::HookAction::AskUser => HookAction::AskUser,
        }
    }
}

impl From<HookAction> for amplifier_core::models::HookAction {
    fn from(action: HookAction) -> Self {
        match action {
            HookAction::Continue => amplifier_core::models::HookAction::Continue,
            HookAction::Deny => amplifier_core::models::HookAction::Deny,
            HookAction::Modify => amplifier_core::models::HookAction::Modify,
            HookAction::InjectContext => amplifier_core::models::HookAction::InjectContext,
            HookAction::AskUser => amplifier_core::models::HookAction::AskUser,
        }
    }
}

// ---------------------------------------------------------------------------
// Bidirectional From conversions: SessionState <-> amplifier_core::models::SessionState
// ---------------------------------------------------------------------------

impl From<amplifier_core::models::SessionState> for SessionState {
    fn from(state: amplifier_core::models::SessionState) -> Self {
        match state {
            amplifier_core::models::SessionState::Running => SessionState::Running,
            amplifier_core::models::SessionState::Completed => SessionState::Completed,
            amplifier_core::models::SessionState::Failed => SessionState::Failed,
            amplifier_core::models::SessionState::Cancelled => SessionState::Cancelled,
        }
    }
}

impl From<SessionState> for amplifier_core::models::SessionState {
    fn from(state: SessionState) -> Self {
        match state {
            SessionState::Running => amplifier_core::models::SessionState::Running,
            SessionState::Completed => amplifier_core::models::SessionState::Completed,
            SessionState::Failed => amplifier_core::models::SessionState::Failed,
            SessionState::Cancelled => amplifier_core::models::SessionState::Cancelled,
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
