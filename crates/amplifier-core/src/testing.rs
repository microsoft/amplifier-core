//! Test fakes for Amplifier kernel traits.
//!
//! Concrete, predictable implementations of the six module traits for use
//! in tests. Every fake stores configurable return values and records calls
//! so tests can assert both behaviour and interaction patterns.
//!
//! # Design Decisions
//!
//! - **Concrete fakes, not mock frameworks** — AI agents can read and modify
//!   these directly. Mock frameworks (mockall) generate invisible code.
//! - **`Arc<Mutex<…>>`** for interior mutability — fakes are stored as
//!   `Arc<dyn Trait>` and must be `Send + Sync`.
//! - **Pre-configured responses** — construct with expected outputs;
//!   `execute`/`complete` consume them in order.
//!
//! # Connections
//!
//! All fakes implement the corresponding trait from [`crate::traits`].
//! They are used by kernel-internal tests (hooks, coordinator, session)
//! and by downstream crate tests via the `testing` module re-export.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::errors::{AmplifierError, ContextError, HookError, ProviderError, ToolError};
use crate::messages::{ChatRequest, ChatResponse, ContentBlock, ToolCall, ToolSpec};
use crate::models::{HookResult, ModelInfo, ProviderInfo, ToolResult};
use crate::traits::{
    ApprovalProvider, ContextManager, HookHandler, Orchestrator, Provider, Tool,
};

// ---------------------------------------------------------------------------
// FakeTool
// ---------------------------------------------------------------------------

/// A fake tool that returns pre-configured results and records calls.
///
/// # Usage
///
/// ```rust
/// use amplifier_core::testing::FakeTool;
/// use amplifier_core::traits::Tool;
///
/// let tool = FakeTool::new("echo", "echoes input");
/// assert_eq!(tool.name(), "echo");
/// ```
pub struct FakeTool {
    tool_name: String,
    tool_description: String,
    /// Pre-configured responses consumed in order. When exhausted, returns
    /// a default success result.
    responses: Mutex<Vec<ToolResult>>,
    /// Records every input passed to `execute`.
    calls: Mutex<Vec<Value>>,
}

impl FakeTool {
    /// Create a fake tool that always returns a default success result.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            tool_name: name.into(),
            tool_description: description.into(),
            responses: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Create a fake tool with pre-configured responses consumed in order.
    pub fn with_responses(name: &str, description: &str, responses: Vec<ToolResult>) -> Self {
        Self {
            tool_name: name.into(),
            tool_description: description.into(),
            responses: Mutex::new(responses),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Return a clone of all recorded call inputs.
    pub fn recorded_calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }
}

impl Tool for FakeTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn get_spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.tool_name.clone(),
            parameters: HashMap::new(),
            description: Some(self.tool_description.clone()),
            extensions: HashMap::new(),
        }
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<ToolResult, ToolError>> + Send + '_>> {
        self.calls.lock().unwrap().push(input.clone());
        let result = {
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                ToolResult {
                    success: true,
                    output: Some(input),
                    error: None,
                }
            } else {
                responses.remove(0)
            }
        };
        Box::pin(async move { Ok(result) })
    }
}

// ---------------------------------------------------------------------------
// FakeProvider
// ---------------------------------------------------------------------------

/// A fake provider that returns a pre-configured text response.
pub struct FakeProvider {
    provider_name: String,
    /// Text content returned by `complete`.
    response_text: String,
    /// Records every request passed to `complete`.
    calls: Mutex<Vec<ChatRequest>>,
}

impl FakeProvider {
    /// Create a fake provider that always returns `response_text` as a text block.
    pub fn new(name: &str, response_text: &str) -> Self {
        Self {
            provider_name: name.into(),
            response_text: response_text.into(),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Return a clone of all recorded requests.
    pub fn recorded_calls(&self) -> Vec<ChatRequest> {
        self.calls.lock().unwrap().clone()
    }
}

impl Provider for FakeProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn get_info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.provider_name.clone(),
            display_name: self.provider_name.clone(),
            credential_env_vars: Vec::new(),
            capabilities: Vec::new(),
            defaults: HashMap::new(),
            config_fields: Vec::new(),
        }
    }

    fn list_models(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send + '_>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn complete(
        &self,
        request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, ProviderError>> + Send + '_>> {
        self.calls.lock().unwrap().push(request);
        let text = self.response_text.clone();
        Box::pin(async move {
            Ok(ChatResponse {
                content: vec![ContentBlock::Text {
                    text,
                    visibility: None,
                    extensions: HashMap::new(),
                }],
                tool_calls: None,
                usage: None,
                degradation: None,
                finish_reason: Some("stop".into()),
                metadata: None,
                extensions: HashMap::new(),
            })
        })
    }

    fn parse_tool_calls(&self, response: &ChatResponse) -> Vec<ToolCall> {
        response.tool_calls.clone().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// FakeContextManager
// ---------------------------------------------------------------------------

/// An in-memory context manager backed by `Arc<Mutex<Vec<Value>>>`.
pub struct FakeContextManager {
    messages: Mutex<Vec<Value>>,
}

impl FakeContextManager {
    /// Create an empty context manager.
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
        }
    }
}

impl Default for FakeContextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextManager for FakeContextManager {
    fn add_message(
        &self,
        message: Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        self.messages.lock().unwrap().push(message);
        Box::pin(async { Ok(()) })
    }

    fn get_messages_for_request(
        &self,
        _token_budget: Option<i64>,
        _provider: Option<Arc<dyn Provider>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>> {
        let msgs = self.messages.lock().unwrap().clone();
        Box::pin(async move { Ok(msgs) })
    }

    fn get_messages(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>> {
        let msgs = self.messages.lock().unwrap().clone();
        Box::pin(async move { Ok(msgs) })
    }

    fn set_messages(
        &self,
        messages: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        *self.messages.lock().unwrap() = messages;
        Box::pin(async { Ok(()) })
    }

    fn clear(&self) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        self.messages.lock().unwrap().clear();
        Box::pin(async { Ok(()) })
    }
}

// ---------------------------------------------------------------------------
// FakeHookHandler
// ---------------------------------------------------------------------------

/// A fake hook handler that records events and returns a configurable result.
pub struct FakeHookHandler {
    /// The result to return on every `handle` call.
    result: HookResult,
    /// Records `(event, data)` for every `handle` call.
    events: Mutex<Vec<(String, Value)>>,
}

impl FakeHookHandler {
    /// Create a handler that always returns `HookAction::Continue`.
    pub fn new() -> Self {
        Self {
            result: HookResult::default(),
            events: Mutex::new(Vec::new()),
        }
    }

    /// Create a handler that always returns the given result.
    pub fn with_result(result: HookResult) -> Self {
        Self {
            result,
            events: Mutex::new(Vec::new()),
        }
    }

    /// Return a clone of all recorded `(event_name, data)` pairs.
    pub fn recorded_events(&self) -> Vec<(String, Value)> {
        self.events.lock().unwrap().clone()
    }
}

impl Default for FakeHookHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl HookHandler for FakeHookHandler {
    fn handle(
        &self,
        event: &str,
        data: Value,
    ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
        self.events
            .lock()
            .unwrap()
            .push((event.to_string(), data));
        let result = self.result.clone();
        Box::pin(async move { Ok(result) })
    }
}

// ---------------------------------------------------------------------------
// FakeOrchestrator
// ---------------------------------------------------------------------------

/// A fake orchestrator that returns a pre-configured response string.
pub struct FakeOrchestrator {
    response: String,
}

impl FakeOrchestrator {
    /// Create a fake orchestrator that always returns `response`.
    pub fn new(response: &str) -> Self {
        Self {
            response: response.into(),
        }
    }
}

impl Orchestrator for FakeOrchestrator {
    fn execute(
        &self,
        _prompt: String,
        _context: Arc<dyn ContextManager>,
        _providers: HashMap<String, Arc<dyn Provider>>,
        _tools: HashMap<String, Arc<dyn Tool>>,
        _hooks: Value,
        _coordinator: Value,
    ) -> Pin<Box<dyn Future<Output = Result<String, AmplifierError>> + Send + '_>> {
        let resp = self.response.clone();
        Box::pin(async move { Ok(resp) })
    }
}

// ---------------------------------------------------------------------------
// FakeApprovalProvider
// ---------------------------------------------------------------------------

/// A fake approval provider that auto-approves or auto-denies.
pub struct FakeApprovalProvider {
    approved: bool,
}

impl FakeApprovalProvider {
    /// Create a provider that always approves.
    pub fn approving() -> Self {
        Self { approved: true }
    }

    /// Create a provider that always denies.
    pub fn denying() -> Self {
        Self { approved: false }
    }
}

impl ApprovalProvider for FakeApprovalProvider {
    fn request_approval(
        &self,
        _request: crate::models::ApprovalRequest,
    ) -> Pin<Box<dyn Future<Output = Result<crate::models::ApprovalResponse, AmplifierError>> + Send + '_>>
    {
        let response = crate::models::ApprovalResponse {
            approved: self.approved,
            reason: None,
            remember: false,
        };
        Box::pin(async move { Ok(response) })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn fake_tool_returns_success() {
        let tool = FakeTool::new("echo", "echoes input");
        let result = tool
            .execute(serde_json::json!({"text": "hello"}))
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn fake_tool_returns_preconfigured_results() {
        let tool = FakeTool::with_responses(
            "multi",
            "multi tool",
            vec![
                crate::models::ToolResult {
                    success: true,
                    output: Some(serde_json::json!("first")),
                    error: None,
                },
                crate::models::ToolResult {
                    success: false,
                    output: None,
                    error: None,
                },
            ],
        );
        let r1 = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(r1.success);
        assert_eq!(r1.output, Some(serde_json::json!("first")));

        let r2 = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(!r2.success);
    }

    #[tokio::test]
    async fn fake_tool_records_calls() {
        let tool = FakeTool::new("rec", "records");
        tool.execute(serde_json::json!({"a": 1})).await.unwrap();
        tool.execute(serde_json::json!({"b": 2})).await.unwrap();
        let calls = tool.recorded_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], serde_json::json!({"a": 1}));
    }

    #[test]
    fn fake_tool_is_arc_compatible() {
        let tool: Arc<dyn Tool> = Arc::new(FakeTool::new("test", "desc"));
        assert_eq!(tool.name(), "test");
        assert_eq!(tool.description(), "desc");
    }

    #[tokio::test]
    async fn fake_provider_returns_response() {
        let provider = FakeProvider::new("test-provider", "Hello from test");
        let req = crate::messages::ChatRequest {
            messages: vec![crate::messages::Message {
                role: crate::messages::Role::User,
                content: crate::messages::MessageContent::Text("hi".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: Default::default(),
            }],
            tools: None,
            response_format: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            conversation_id: None,
            stream: None,
            metadata: None,
            model: None,
            tool_choice: None,
            stop: None,
            reasoning_effort: None,
            timeout: None,
            extensions: Default::default(),
        };
        let response = provider.complete(req).await.unwrap();
        assert!(!response.content.is_empty());
    }

    #[test]
    fn fake_provider_is_arc_compatible() {
        let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new("p", "resp"));
        assert_eq!(provider.name(), "p");
    }

    #[tokio::test]
    async fn fake_context_manager_stores_messages() {
        let ctx = FakeContextManager::new();
        ctx.add_message(serde_json::json!({"role": "user", "content": "hello"}))
            .await
            .unwrap();
        ctx.add_message(serde_json::json!({"role": "assistant", "content": "hi"}))
            .await
            .unwrap();
        let msgs = ctx.get_messages().await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[tokio::test]
    async fn fake_context_manager_set_and_clear() {
        let ctx = FakeContextManager::new();
        ctx.set_messages(vec![serde_json::json!({"role": "system", "content": "init"})])
            .await
            .unwrap();
        assert_eq!(ctx.get_messages().await.unwrap().len(), 1);

        ctx.clear().await.unwrap();
        assert!(ctx.get_messages().await.unwrap().is_empty());
    }

    #[test]
    fn fake_context_manager_is_arc_compatible() {
        let _ctx: Arc<dyn ContextManager> = Arc::new(FakeContextManager::new());
    }

    #[tokio::test]
    async fn fake_hook_handler_returns_continue() {
        let handler = FakeHookHandler::new();
        let result = handler
            .handle("test:event", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result.action, crate::models::HookAction::Continue);
    }

    #[tokio::test]
    async fn fake_hook_handler_records_events() {
        let handler = FakeHookHandler::new();
        handler
            .handle("tool:pre", serde_json::json!({"tool": "bash"}))
            .await
            .unwrap();
        handler
            .handle("tool:post", serde_json::json!({"tool": "bash"}))
            .await
            .unwrap();
        let events = handler.recorded_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "tool:pre");
        assert_eq!(events[1].0, "tool:post");
    }

    #[tokio::test]
    async fn fake_hook_handler_with_custom_result() {
        let custom = crate::models::HookResult {
            action: crate::models::HookAction::Deny,
            reason: Some("blocked".into()),
            ..Default::default()
        };
        let handler = FakeHookHandler::with_result(custom.clone());
        let result = handler
            .handle("test:event", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result.action, crate::models::HookAction::Deny);
        assert_eq!(result.reason.as_deref(), Some("blocked"));
    }

    #[test]
    fn fake_hook_handler_is_arc_compatible() {
        let _handler: Arc<dyn HookHandler> = Arc::new(FakeHookHandler::new());
    }

    #[tokio::test]
    async fn fake_orchestrator_returns_response() {
        let orch = FakeOrchestrator::new("orchestrated response");
        let result = orch
            .execute(
                "hello".into(),
                Arc::new(FakeContextManager::new()),
                Default::default(),
                Default::default(),
                serde_json::json!({}),
                serde_json::json!({}),
            )
            .await
            .unwrap();
        assert_eq!(result, "orchestrated response");
    }

    #[test]
    fn fake_orchestrator_is_arc_compatible() {
        let _orch: Arc<dyn Orchestrator> = Arc::new(FakeOrchestrator::new("ok"));
    }
}
