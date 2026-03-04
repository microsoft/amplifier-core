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

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction};

use amplifier_core::errors::HookError;
use amplifier_core::models as core_models;
use amplifier_core::models::HookResult;
use amplifier_core::traits::HookHandler;

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

// ---------------------------------------------------------------------------
// JsHookHandlerBridge — lets JS functions act as Rust HookHandler trait objects
// ---------------------------------------------------------------------------

/// Bridges a JS callback function to the Rust `HookHandler` trait via
/// `ThreadsafeFunction`. The callback receives `(event: string, data: string)`
/// and returns a JSON string representing a `HookResult`.
struct JsHookHandlerBridge {
    callback: ThreadsafeFunction<(String, String), ErrorStrategy::Fatal>,
}

// Safety: ThreadsafeFunction is designed for cross-thread use in napi-rs.
unsafe impl Send for JsHookHandlerBridge {}
unsafe impl Sync for JsHookHandlerBridge {}

impl HookHandler for JsHookHandlerBridge {
    fn handle(
        &self,
        event: &str,
        data: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<HookResult, HookError>> + Send + '_>> {
        let event = event.to_string();
        let data_str = serde_json::to_string(&data).unwrap_or_else(|e| {
            eprintln!(
                "amplifier-core-node: failed to serialize hook data to JSON: {e}. Defaulting to empty object."
            );
            "{}".to_string()
        });
        Box::pin(async move {
            let result_str: String = self
                .callback
                .call_async((event, data_str))
                .await
                .map_err(|e| HookError::HandlerFailed {
                    message: e.to_string(),
                    handler_name: None,
                })?;
            let hook_result: HookResult = serde_json::from_str(&result_str).unwrap_or_else(|e| {
                eprintln!(
                    "amplifier-core-node: failed to parse HookResult from JS handler: {e}. Defaulting to Continue."
                );
                HookResult::default()
            });
            Ok(hook_result)
        })
    }
}

// ---------------------------------------------------------------------------
// HookResult converter
// ---------------------------------------------------------------------------

fn hook_result_to_js(result: HookResult) -> JsHookResult {
    JsHookResult {
        action: result.action.into(),
        reason: result.reason,
        context_injection: result.context_injection,
        context_injection_role: Some(result.context_injection_role.into()),
        ephemeral: Some(result.ephemeral),
        suppress_output: Some(result.suppress_output),
        user_message: result.user_message,
        user_message_level: Some(result.user_message_level.into()),
        user_message_source: result.user_message_source,
        approval_prompt: result.approval_prompt,
        approval_timeout: Some(result.approval_timeout),
        approval_default: Some(result.approval_default.into()),
    }
}

// ---------------------------------------------------------------------------
// JsHookRegistry — wraps amplifier_core::HookRegistry for Node.js
// ---------------------------------------------------------------------------

/// Wraps `amplifier_core::HookRegistry` for Node.js.
///
/// Provides register/emit/listHandlers/setDefaultFields — the event backbone
/// of the kernel.
#[napi]
pub struct JsHookRegistry {
    pub(crate) inner: Arc<amplifier_core::HookRegistry>,
}

#[napi]
impl JsHookRegistry {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(amplifier_core::HookRegistry::new()),
        }
    }

    /// Creates a new **detached** (empty) registry.
    ///
    /// Unlike `JsCancellationToken::from_inner`, HookRegistry cannot be cheaply
    /// cloned or wrapped from a reference, so this always creates an empty
    /// registry. When Coordinator manages ownership, this should accept
    /// `Arc<HookRegistry>` to share state.
    pub fn new_detached() -> Self {
        Self {
            inner: Arc::new(amplifier_core::HookRegistry::new()),
        }
    }

    #[napi]
    pub fn register(
        &self,
        event: String,
        handler: JsFunction,
        priority: i32,
        name: String,
    ) -> Result<()> {
        let tsfn: ThreadsafeFunction<(String, String), ErrorStrategy::Fatal> = handler
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<(String, String)>| {
                let event_str = ctx.env.create_string(&ctx.value.0)?;
                let data_str = ctx.env.create_string(&ctx.value.1)?;
                Ok(vec![event_str.into_unknown(), data_str.into_unknown()])
            })?;

        let bridge = JsHookHandlerBridge { callback: tsfn };
        self.inner
            .register(&event, Arc::new(bridge), priority, Some(name));
        Ok(())
    }

    #[napi]
    pub async fn emit(&self, event: String, data_json: String) -> Result<JsHookResult> {
        let data: serde_json::Value =
            serde_json::from_str(&data_json).map_err(|e| Error::from_reason(e.to_string()))?;
        let result = self.inner.emit(&event, data).await;
        Ok(hook_result_to_js(result))
    }

    #[napi]
    pub fn list_handlers(&self) -> HashMap<String, Vec<String>> {
        self.inner.list_handlers(None)
    }

    #[napi]
    pub fn set_default_fields(&self, defaults_json: String) -> Result<()> {
        let defaults: serde_json::Value = serde_json::from_str(&defaults_json)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        self.inner.set_default_fields(defaults);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// JsCoordinator — wraps amplifier_core::Coordinator for Node.js
// ---------------------------------------------------------------------------

/// Wraps `amplifier_core::Coordinator` for Node.js — the central hub holding
/// module mount points, capabilities, hook registry, cancellation token, and config.
///
/// Implements the hybrid coordinator pattern: JS-side storage for TS module
/// objects, Rust kernel for everything else.
#[napi]
pub struct JsCoordinator {
    pub(crate) inner: Arc<amplifier_core::Coordinator>,
}

#[napi]
impl JsCoordinator {
    #[napi(constructor)]
    pub fn new(config_json: String) -> Result<Self> {
        let config: HashMap<String, serde_json::Value> =
            serde_json::from_str(&config_json).map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(amplifier_core::Coordinator::new(config)),
        })
    }

    #[napi(getter)]
    pub fn tool_names(&self) -> Vec<String> {
        self.inner.tool_names()
    }

    #[napi(getter)]
    pub fn provider_names(&self) -> Vec<String> {
        self.inner.provider_names()
    }

    #[napi(getter)]
    pub fn has_orchestrator(&self) -> bool {
        self.inner.has_orchestrator()
    }

    #[napi(getter)]
    pub fn has_context(&self) -> bool {
        self.inner.has_context()
    }

    #[napi]
    pub fn register_capability(&self, name: String, value_json: String) -> Result<()> {
        let value: serde_json::Value = serde_json::from_str(&value_json)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        self.inner.register_capability(&name, value);
        Ok(())
    }

    #[napi]
    pub fn get_capability(&self, name: String) -> Option<String> {
        self.inner
            .get_capability(&name)
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "null".to_string()))
    }

    /// Returns a JsHookRegistry wrapper.
    ///
    /// TODO(task-6): This creates a separate (detached) HookRegistry because
    /// Coordinator owns HookRegistry by value, not behind Arc. When Session
    /// wires everything together in Task 6, this should share the coordinator's
    /// actual hook registry.
    #[napi(getter)]
    pub fn hooks(&self) -> JsHookRegistry {
        JsHookRegistry::new_detached()
    }

    #[napi(getter)]
    pub fn cancellation(&self) -> JsCancellationToken {
        JsCancellationToken::from_inner(self.inner.cancellation().clone())
    }

    #[napi(getter)]
    pub fn config(&self) -> Result<String> {
        serde_json::to_string(self.inner.config())
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    #[napi]
    pub fn reset_turn(&self) {
        self.inner.reset_turn();
    }

    #[napi]
    pub fn to_dict(&self) -> HashMap<String, serde_json::Value> {
        self.inner.to_dict()
    }

    #[napi]
    pub async fn cleanup(&self) -> Result<()> {
        self.inner.cleanup().await;
        Ok(())
    }
}
