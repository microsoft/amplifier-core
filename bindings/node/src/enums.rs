// ---------------------------------------------------------------------------
// Enums — exported as TypeScript string unions via #[napi(string_enum)]
// ---------------------------------------------------------------------------

use amplifier_core::models as core_models;

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
// Bidirectional From conversions: ContextInjectionRole
// ---------------------------------------------------------------------------

impl From<core_models::ContextInjectionRole> for ContextInjectionRole {
    fn from(role: core_models::ContextInjectionRole) -> Self {
        match role {
            core_models::ContextInjectionRole::System => ContextInjectionRole::System,
            core_models::ContextInjectionRole::User => ContextInjectionRole::User,
            core_models::ContextInjectionRole::Assistant => ContextInjectionRole::Assistant,
        }
    }
}

impl From<ContextInjectionRole> for core_models::ContextInjectionRole {
    fn from(role: ContextInjectionRole) -> Self {
        match role {
            ContextInjectionRole::System => core_models::ContextInjectionRole::System,
            ContextInjectionRole::User => core_models::ContextInjectionRole::User,
            ContextInjectionRole::Assistant => core_models::ContextInjectionRole::Assistant,
        }
    }
}

// ---------------------------------------------------------------------------
// Bidirectional From conversions: UserMessageLevel
// ---------------------------------------------------------------------------

impl From<core_models::UserMessageLevel> for UserMessageLevel {
    fn from(level: core_models::UserMessageLevel) -> Self {
        match level {
            core_models::UserMessageLevel::Info => UserMessageLevel::Info,
            core_models::UserMessageLevel::Warning => UserMessageLevel::Warning,
            core_models::UserMessageLevel::Error => UserMessageLevel::Error,
        }
    }
}

impl From<UserMessageLevel> for core_models::UserMessageLevel {
    fn from(level: UserMessageLevel) -> Self {
        match level {
            UserMessageLevel::Info => core_models::UserMessageLevel::Info,
            UserMessageLevel::Warning => core_models::UserMessageLevel::Warning,
            UserMessageLevel::Error => core_models::UserMessageLevel::Error,
        }
    }
}

// ---------------------------------------------------------------------------
// Bidirectional From conversions: ApprovalDefault
// ---------------------------------------------------------------------------

impl From<core_models::ApprovalDefault> for ApprovalDefault {
    fn from(default: core_models::ApprovalDefault) -> Self {
        match default {
            core_models::ApprovalDefault::Allow => ApprovalDefault::Allow,
            core_models::ApprovalDefault::Deny => ApprovalDefault::Deny,
        }
    }
}

impl From<ApprovalDefault> for core_models::ApprovalDefault {
    fn from(default: ApprovalDefault) -> Self {
        match default {
            ApprovalDefault::Allow => core_models::ApprovalDefault::Allow,
            ApprovalDefault::Deny => core_models::ApprovalDefault::Deny,
        }
    }
}
