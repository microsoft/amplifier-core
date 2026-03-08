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

use napi::bindgen_prelude::Promise;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction};
use tokio::sync::Mutex;

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

impl Default for JsCancellationToken {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn request_graceful(&self) {
        self.inner.request_graceful();
    }

    #[napi]
    pub fn request_immediate(&self) {
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
            let result_str: String =
                self.callback
                    .call_async((event, data_str))
                    .await
                    .map_err(|e| HookError::HandlerFailed {
                        message: e.to_string(),
                        handler_name: None,
                    })?;
            let hook_result: HookResult = serde_json::from_str(&result_str).unwrap_or_else(|e| {
                log::error!(
                    "SECURITY: Hook handler returned unparseable result — failing closed (Deny): {e} — json: {result_str}"
                );
                HookResult {
                    action: core_models::HookAction::Deny,
                    reason: Some("Hook handler returned invalid response".to_string()),
                    ..Default::default()
                }
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

impl Default for JsHookRegistry {
    fn default() -> Self {
        Self::new()
    }
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

    /// Register a hook handler for the given event name.
    ///
    /// ## Handler signature
    ///
    /// The `handler` callback receives two string arguments and must return a
    /// JSON-serialized `HookResult` (or a `Promise` that resolves to one):
    ///
    /// ```ts
    /// (event: string, dataJson: string) => string | Promise<string>
    /// ```
    ///
    /// Where the return value is a JSON string matching the `JsHookResult`
    /// shape, e.g. `'{"action":"Continue"}'`.  If the handler returns an
    /// invalid JSON string, the kernel fails closed and treats it as `Deny`.
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
        // HandlerId unused — unregister not yet exposed to JS
        let _ = self
            .inner
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
        let defaults: serde_json::Value =
            serde_json::from_str(&defaults_json).map_err(|e| Error::from_reason(e.to_string()))?;
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
        let value: serde_json::Value =
            serde_json::from_str(&value_json).map_err(|e| Error::from_reason(e.to_string()))?;
        self.inner.register_capability(&name, value);
        Ok(())
    }

    #[napi]
    pub fn get_capability(&self, name: String) -> Result<Option<String>> {
        match self.inner.get_capability(&name) {
            Some(v) => serde_json::to_string(&v)
                .map(Some)
                .map_err(|e| Error::from_reason(e.to_string())),
            None => Ok(None),
        }
    }

    /// Creates a new **detached** (empty) JsHookRegistry.
    ///
    /// ⚠️  **Each call returns a brand-new, empty registry** — hooks registered
    /// on one instance are invisible to the next. This is a known limitation:
    /// `Coordinator` owns its `HookRegistry` by value, not behind `Arc`, so
    /// the binding cannot share state across calls.
    ///
    /// The method name (`createHookRegistry`) intentionally signals "creates new
    /// instance" — a getter property would imply referential stability in JS.
    ///
    /// **Workaround:** create a `JsHookRegistry` directly and hold a reference.
    ///
    /// Future TODO #1: restructure the kernel to hold `Arc<HookRegistry>` inside
    /// `Coordinator` so this method can share the same registry instance.
    #[napi]
    pub fn create_hook_registry(&self) -> JsHookRegistry {
        log::warn!(
            "JsCoordinator::createHookRegistry() — returns a new detached HookRegistry; \
             hooks registered on one call are NOT visible via the Coordinator's internal \
             registry. Hold the returned instance directly. (Future TODO #1)"
        );
        JsHookRegistry::new_detached()
    }

    #[napi(getter)]
    pub fn cancellation(&self) -> JsCancellationToken {
        JsCancellationToken::from_inner(self.inner.cancellation().clone())
    }

    #[napi(getter)]
    pub fn config(&self) -> Result<String> {
        serde_json::to_string(self.inner.config()).map_err(|e| Error::from_reason(e.to_string()))
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

// ---------------------------------------------------------------------------
// JsAmplifierSession — wraps amplifier_core::Session for Node.js
// ---------------------------------------------------------------------------

/// Wraps `amplifier_core::Session` for Node.js — the top-level entry point.
///
/// Lifecycle: `new AmplifierSession(config) → initialize() → execute(prompt) → cleanup()`.
/// Wires together Coordinator, HookRegistry, and CancellationToken.
///
/// Known limitation: `coordinator` getter creates a separate Coordinator instance
/// because the kernel Session owns its Coordinator by value, not behind Arc.
/// Sharing requires restructuring the Rust kernel — tracked as Future TODO #1.
#[napi]
pub struct JsAmplifierSession {
    inner: Arc<Mutex<amplifier_core::Session>>,
    cached_session_id: String,
    cached_parent_id: Option<String>,
    cached_config: HashMap<String, serde_json::Value>,
}

#[napi]
impl JsAmplifierSession {
    #[napi(constructor)]
    pub fn new(
        config_json: String,
        session_id: Option<String>,
        parent_id: Option<String>,
    ) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(&config_json)
            .map_err(|e| Error::from_reason(format!("Invalid config JSON: {e}")))?;

        let config = amplifier_core::SessionConfig::from_value(value.clone())
            .map_err(|e| Error::from_reason(e.to_string()))?;

        let cached_config: HashMap<String, serde_json::Value> = serde_json::from_value(value)
            .map_err(|e| Error::from_reason(format!("invalid JSON: {e}")))?;

        let session = amplifier_core::Session::new(config, session_id.clone(), parent_id.clone());
        let cached_session_id = session.session_id().to_string();

        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
            cached_session_id,
            cached_parent_id: parent_id,
            cached_config,
        })
    }

    #[napi(getter)]
    pub fn session_id(&self) -> &str {
        &self.cached_session_id
    }

    #[napi(getter)]
    pub fn parent_id(&self) -> Option<String> {
        self.cached_parent_id.clone()
    }

    #[napi(getter)]
    pub fn is_initialized(&self) -> bool {
        match self.inner.try_lock() {
            Ok(session) => session.is_initialized(),
            // Safe default: lock is only held during async cleanup(), which sets
            // initialized to false — so false is a correct conservative fallback.
            Err(_) => false,
        }
    }

    /// Current session lifecycle state as a lowercase string.
    ///
    /// Returns one of the `SessionState` variant strings:
    /// - `"Running"` — session is active
    /// - `"Completed"` — session finished successfully
    /// - `"Failed"` — session encountered a fatal error
    /// - `"Cancelled"` — session was cancelled via the cancellation token
    ///
    /// Falls back to `"running"` if the session lock is held during `cleanup()`.
    #[napi(getter)]
    pub fn status(&self) -> String {
        match self.inner.try_lock() {
            Ok(session) => session.status().to_string(),
            // Safe default: lock is only held during async cleanup(), and sessions
            // start as "running" — returning "running" during cleanup is tolerable.
            Err(_) => "running".to_string(),
        }
    }

    /// Creates a new **fresh** JsCoordinator from this session's cached config.
    ///
    /// ⚠️  **Each call allocates a new Coordinator** — capabilities registered on
    /// one instance are invisible to the next. This is a known limitation:
    /// `Session` owns its `Coordinator` by value, not behind `Arc`, so the
    /// binding cannot expose the session's live coordinator.
    ///
    /// The method name (`createCoordinator`) intentionally signals "creates new
    /// instance" — a getter property would imply referential stability in JS.
    ///
    /// **Workaround:** call `createCoordinator()` once, hold the returned instance,
    /// and register capabilities on it before passing it to other APIs.
    ///
    /// Future TODO #1: restructure the kernel to hold `Arc<Coordinator>` inside
    /// `Session` so this method can return a handle to the session's actual coordinator.
    #[napi]
    pub fn create_coordinator(&self) -> JsCoordinator {
        log::warn!(
            "JsAmplifierSession::createCoordinator() — returns a new Coordinator built from \
             cached config; capabilities registered on one call are NOT visible on the next. \
             Hold the returned instance directly. (Future TODO #1)"
        );
        JsCoordinator {
            inner: Arc::new(amplifier_core::Coordinator::new(self.cached_config.clone())),
        }
    }

    #[napi]
    pub fn set_initialized(&self) {
        match self.inner.try_lock() {
            Ok(session) => session.set_initialized(),
            // State mutation failed — unlike read-only getters, this warrants a warning.
            // Lock contention only occurs during async cleanup(), so this is unlikely
            // in practice, but callers should know the mutation didn't happen.
            Err(_) => eprintln!(
                "amplifier-core-node: set_initialized() skipped — session lock held (cleanup in progress?)"
            ),
        }
    }

    #[napi]
    pub async fn cleanup(&self) -> Result<()> {
        let session = self.inner.lock().await;
        session.cleanup().await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// JsToolBridge — lets TS authors implement Tool as plain TS objects
// ---------------------------------------------------------------------------

/// Bridges a TypeScript tool object to Rust via `ThreadsafeFunction`.
///
/// In the hybrid coordinator pattern, these bridge objects are stored in a
/// JS-side Map (not in the Rust Coordinator). The JS orchestrator retrieves
/// them by name and calls `execute()` directly.
#[napi]
pub struct JsToolBridge {
    tool_name: String,
    tool_description: String,
    parameters_json: String,
    execute_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
}

#[napi]
impl JsToolBridge {
    #[napi(
        constructor,
        ts_args_type = "name: string, description: string, parametersJson: string, executeFn: (inputJson: string) => Promise<string>"
    )]
    pub fn new(
        name: String,
        description: String,
        parameters_json: String,
        execute_fn: JsFunction,
    ) -> Result<Self> {
        let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = execute_fn
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
                let input_str = ctx.env.create_string(&ctx.value)?;
                Ok(vec![input_str.into_unknown()])
            })?;

        Ok(Self {
            tool_name: name,
            tool_description: description,
            parameters_json,
            execute_fn: tsfn,
        })
    }

    #[napi(getter)]
    pub fn name(&self) -> &str {
        &self.tool_name
    }

    #[napi(getter)]
    pub fn description(&self) -> &str {
        &self.tool_description
    }

    #[napi]
    pub async fn execute(&self, input_json: String) -> Result<String> {
        let promise: Promise<String> = self
            .execute_fn
            .call_async(input_json)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
        promise.await.map_err(|e| Error::from_reason(e.to_string()))
    }

    #[napi]
    pub fn get_spec(&self) -> String {
        let params: serde_json::Value =
            serde_json::from_str(&self.parameters_json).unwrap_or_else(|e| {
                eprintln!(
                    "amplifier-core-node: JsToolBridge::get_spec() failed to parse parameters_json: {e}. Defaulting to empty object."
                );
                serde_json::Value::Object(serde_json::Map::new())
            });
        serde_json::json!({
            "name": self.tool_name,
            "description": self.tool_description,
            "parameters": params
        })
        .to_string()
    }
}

// ---------------------------------------------------------------------------
// Error bridging — Rust errors → typed JS error objects
// ---------------------------------------------------------------------------

/// Structured error object returned to JS with a typed `code` property.
#[napi(object)]
pub struct JsAmplifierError {
    pub code: String,
    pub message: String,
}

/// Maps a lowercase variant name to its error code string.
///
/// Variant mapping:
/// - `"session"` → `"SessionError"`
/// - `"tool"` → `"ToolError"`
/// - `"provider"` → `"ProviderError"`
/// - `"hook"` → `"HookError"`
/// - `"context"` → `"ContextError"`
/// - anything else → `"AmplifierError"`
fn error_code_for_variant(variant: &str) -> &'static str {
    match variant {
        "session" => "SessionError",
        "tool" => "ToolError",
        "provider" => "ProviderError",
        "hook" => "HookError",
        "context" => "ContextError",
        _ => "AmplifierError",
    }
}

/// Converts an error variant name and message into a typed `JsAmplifierError`.
///
/// See [`error_code_for_variant`] for the variant → code mapping.
#[napi]
pub fn amplifier_error_to_js(variant: String, message: String) -> JsAmplifierError {
    let code = error_code_for_variant(&variant).to_string();
    JsAmplifierError { code, message }
}

/// Internal helper: converts an `AmplifierError` into a `napi::Error` with a
/// `[Code] message` format suitable for crossing the FFI boundary.
///
/// Uses [`error_code_for_variant`] for consistent code mapping.
#[allow(dead_code)] // Used when async methods expose Result<T, AmplifierError> across FFI
fn amplifier_error_to_napi(err: amplifier_core::errors::AmplifierError) -> napi::Error {
    let (variant, msg) = match &err {
        amplifier_core::errors::AmplifierError::Session(e) => ("session", e.to_string()),
        amplifier_core::errors::AmplifierError::Tool(e) => ("tool", e.to_string()),
        amplifier_core::errors::AmplifierError::Provider(e) => ("provider", e.to_string()),
        amplifier_core::errors::AmplifierError::Hook(e) => ("hook", e.to_string()),
        amplifier_core::errors::AmplifierError::Context(e) => ("context", e.to_string()),
    };
    let code = error_code_for_variant(variant);
    Error::from_reason(format!("[{code}] {msg}"))
}

// ---------------------------------------------------------------------------
// Module resolver bindings (Phase 4)
// ---------------------------------------------------------------------------

/// Result from resolving a module path.
#[napi(object)]
pub struct JsModuleManifest {
    /// How the module is loaded and invoked.
    ///
    /// Valid values (string literal union):
    /// `"python"` | `"wasm"` | `"grpc"` | `"native"`
    pub transport: String,

    /// Logical role the module plays inside the kernel.
    ///
    /// Valid values (string literal union):
    /// `"tool"` | `"hook"` | `"context"` | `"approval"` | `"provider"` | `"orchestrator"`
    pub module_type: String,

    /// Artifact format used to locate or load the module.
    ///
    /// Valid values (string literal union):
    /// `"wasm"` | `"grpc"` | `"python"`
    ///
    /// - `"wasm"` — `artifactPath` contains the `.wasm` component file path
    /// - `"grpc"` — `endpoint` contains the gRPC service URL
    /// - `"python"` — `packageName` contains the importable Python package name
    pub artifact_type: String,

    /// Path to WASM artifact (present when `artifactType` is `"wasm"`).
    pub artifact_path: Option<String>,

    /// gRPC service endpoint URL (present when `artifactType` is `"grpc"`).
    pub endpoint: Option<String>,

    /// Python package name for import (present when `artifactType` is `"python"`).
    pub package_name: Option<String>,
}

/// Resolve a module from a filesystem path.
///
/// Returns a JsModuleManifest describing the transport, module type, and artifact.
#[napi]
pub fn resolve_module(path: String) -> Result<JsModuleManifest> {
    let manifest = amplifier_core::module_resolver::resolve_module(std::path::Path::new(&path))
        .map_err(|e| Error::from_reason(format!("{e}")))?;

    let transport = match manifest.transport {
        amplifier_core::transport::Transport::Python => "python",
        amplifier_core::transport::Transport::Wasm => "wasm",
        amplifier_core::transport::Transport::Grpc => "grpc",
        amplifier_core::transport::Transport::Native => "native",
    };

    let module_type = match manifest.module_type {
        amplifier_core::models::ModuleType::Tool => "tool",
        amplifier_core::models::ModuleType::Hook => "hook",
        amplifier_core::models::ModuleType::Context => "context",
        amplifier_core::models::ModuleType::Approval => "approval",
        amplifier_core::models::ModuleType::Provider => "provider",
        amplifier_core::models::ModuleType::Orchestrator => "orchestrator",
        amplifier_core::models::ModuleType::Resolver => "resolver",
    };

    let (artifact_type, artifact_path, endpoint, package_name) = match &manifest.artifact {
        amplifier_core::module_resolver::ModuleArtifact::WasmPath(path) => {
            ("wasm", Some(path.to_string_lossy().to_string()), None, None)
        }
        amplifier_core::module_resolver::ModuleArtifact::WasmBytes { path, .. } => {
            ("wasm", Some(path.to_string_lossy().to_string()), None, None)
        }
        amplifier_core::module_resolver::ModuleArtifact::GrpcEndpoint(ep) => {
            ("grpc", None, Some(ep.clone()), None)
        }
        amplifier_core::module_resolver::ModuleArtifact::PythonModule(name) => {
            ("python", None, None, Some(name.clone()))
        }
    };

    Ok(JsModuleManifest {
        transport: transport.to_string(),
        module_type: module_type.to_string(),
        artifact_type: artifact_type.to_string(),
        artifact_path,
        endpoint,
        package_name,
    })
}

/// Load a WASM module from a path and return status info.
///
/// For WASM modules: loads the component and returns module type info.
/// For Python modules: returns an error (TS host can't load Python).
#[napi]
pub fn load_wasm_from_path(path: String) -> Result<String> {
    let manifest = amplifier_core::module_resolver::resolve_module(std::path::Path::new(&path))
        .map_err(|e| Error::from_reason(format!("{e}")))?;

    if manifest.transport == amplifier_core::transport::Transport::Python {
        return Err(Error::from_reason(
            "Python module detected — compile to WASM or run as gRPC sidecar. \
             TypeScript hosts cannot load Python modules.",
        ));
    }

    if manifest.transport != amplifier_core::transport::Transport::Wasm {
        return Err(Error::from_reason(format!(
            "load_wasm_from_path only handles WASM modules, got transport '{:?}'",
            manifest.transport
        )));
    }

    let engine = amplifier_core::wasm_engine::WasmEngine::new()
        .map_err(|e| Error::from_reason(format!("WASM engine creation failed: {e}")))?;

    let coordinator = std::sync::Arc::new(amplifier_core::Coordinator::new_for_test());
    let loaded =
        amplifier_core::module_resolver::load_module(&manifest, engine.inner(), Some(coordinator))
            .map_err(|e| Error::from_reason(format!("Module loading failed: {e}")))?;

    Ok(format!("loaded:{}", loaded.variant_name()))
}
