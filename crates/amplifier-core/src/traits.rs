//! Module contract traits for the Amplifier kernel.
//!
//! These six traits define the interfaces that module authors implement.
//! The kernel stores modules as `Arc<dyn Trait>` and dispatches dynamically.
//!
//! # Design Decisions
//!
//! - **Explicit `Pin<Box<dyn Future>>`** instead of `#[async_trait]` —
//!   no macro magic, AI agents see the actual type signature.
//! - **`Send + Sync` on trait definition** — errors appear at impl site,
//!   not scattered across every usage site.
//! - **`Arc<dyn Trait>`** over generics — no generic virus, runtime module
//!   loading requires dynamic dispatch anyway.
//!
//! # Connections
//!
//! - [`Tool`], [`Provider`], [`Orchestrator`], [`ContextManager`] are the
//!   four primary module types that session/coordinator manages.
//! - [`HookHandler`] participates in the hook dispatch pipeline.
//! - [`ApprovalProvider`] provides UI-driven approval gates.
//!
//! All data types referenced here are defined in [`crate::models`],
//! [`crate::messages`], and [`crate::errors`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::errors::{AmplifierError, ContextError, HookError, ProviderError, ToolError};
use crate::messages::{ChatRequest, ChatResponse, ToolCall, ToolSpec};
use crate::models::{
    ApprovalRequest, ApprovalResponse, HookResult, ModelInfo, ProviderInfo, ToolResult,
};

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Interface for tool modules.
///
/// Tools provide capabilities that agents can invoke during orchestration.
/// Each tool has a unique name, a human-readable description, and an async
/// `execute` method that processes JSON input and returns a [`ToolResult`].
///
/// # Python equivalent
///
/// ```python
/// class Tool(Protocol):
///     @property
///     def name(self) -> str: ...
///     @property
///     def description(self) -> str: ...
///     async def execute(self, input: dict[str, Any]) -> ToolResult: ...
/// ```
///
/// # Object safety
///
/// This trait is object-safe: `Arc<dyn Tool>` is the standard storage type.
///
/// # Example
///
/// ```rust
/// use std::pin::Pin;
/// use std::future::Future;
/// use amplifier_core::traits::Tool;
/// use amplifier_core::models::ToolResult;
/// use amplifier_core::errors::ToolError;
/// use amplifier_core::messages::ToolSpec;
/// use serde_json::Value;
/// use std::collections::HashMap;
///
/// struct EchoTool;
///
/// impl Tool for EchoTool {
///     fn name(&self) -> &str { "echo" }
///     fn description(&self) -> &str { "Echoes input back" }
///     fn get_spec(&self) -> ToolSpec {
///         ToolSpec {
///             name: "echo".into(),
///             parameters: HashMap::new(),
///             description: Some("Echoes input back".into()),
///             extensions: HashMap::new(),
///         }
///     }
///     fn execute(
///         &self,
///         input: Value,
///     ) -> Pin<Box<dyn Future<Output = Result<ToolResult, ToolError>> + Send + '_>> {
///         Box::pin(async move {
///             Ok(ToolResult { success: true, output: Some(input), error: None })
///         })
///     }
/// }
/// ```
pub trait Tool: Send + Sync {
    /// Unique name used to invoke this tool (e.g., `"bash"`, `"read_file"`).
    fn name(&self) -> &str;

    /// Human-readable description shown to the LLM.
    fn description(&self) -> &str;

    /// Return a [`ToolSpec`] describing this tool's JSON Schema interface.
    ///
    /// Providers send this spec to the LLM so it knows what arguments to pass.
    fn get_spec(&self) -> ToolSpec;

    /// Execute the tool with the given JSON input.
    ///
    /// # Arguments
    ///
    /// * `input` — Tool-specific input parameters as a JSON value
    ///   (typically an object matching the schema from [`get_spec`](Tool::get_spec)).
    ///
    /// # Returns
    ///
    /// `Ok(ToolResult)` on success (even partial success — check `success` field).
    /// `Err(ToolError)` only for infrastructure failures (tool not found, etc.).
    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<ToolResult, ToolError>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Interface for LLM provider modules.
///
/// Providers receive [`ChatRequest`] (typed, validated messages) and return
/// [`ChatResponse`] (typed, structured content). Orchestrators handle
/// conversion between context storage format (`Value`) and provider
/// contract (`ChatRequest`).
///
/// # Python equivalent
///
/// ```python
/// class Provider(Protocol):
///     @property
///     def name(self) -> str: ...
///     def get_info(self) -> ProviderInfo: ...
///     async def list_models(self) -> list[ModelInfo]: ...
///     async def complete(self, request: ChatRequest, **kwargs) -> ChatResponse: ...
///     def parse_tool_calls(self, response: ChatResponse) -> list[ToolCall]: ...
/// ```
///
/// # Object safety
///
/// This trait is object-safe: `Arc<dyn Provider>` is the standard storage type.
pub trait Provider: Send + Sync {
    /// Provider identifier (e.g., `"anthropic"`, `"openai"`).
    fn name(&self) -> &str;

    /// Return provider metadata (capabilities, credentials, defaults).
    fn get_info(&self) -> ProviderInfo;

    /// List models available from this provider.
    ///
    /// Implementations may query an API, return a hardcoded list, or return
    /// an empty `Vec` if model discovery is not supported.
    fn list_models(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send + '_>>;

    /// Generate a completion from a [`ChatRequest`].
    ///
    /// # Arguments
    ///
    /// * `request` — Typed chat request with messages, tools, and config.
    ///
    /// # Returns
    ///
    /// `Ok(ChatResponse)` with content blocks, optional tool calls, and usage.
    /// `Err(ProviderError)` with a typed error (rate limit, auth, timeout, etc.).
    fn complete(
        &self,
        request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, ProviderError>> + Send + '_>>;

    /// Extract tool calls from a provider response.
    ///
    /// Each provider may encode tool calls differently in the response.
    /// This method normalises them into [`ToolCall`] structs.
    fn parse_tool_calls(&self, response: &ChatResponse) -> Vec<ToolCall>;
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Interface for agent-loop orchestrator modules.
///
/// The orchestrator owns the prompt→response loop: it asks the context
/// manager for messages, calls a provider, handles tool calls, and
/// emits hook events.
///
/// # Python equivalent
///
/// ```python
/// class Orchestrator(Protocol):
///     async def execute(
///         self, prompt, context, providers, tools, hooks, **kwargs,
///     ) -> str: ...
/// ```
///
/// In Python the kernel injects `coordinator=<ModuleCoordinator>` via
/// `**kwargs`. In Rust the coordinator is passed as an explicit `Value`
/// parameter to avoid hidden coupling. The concrete `Coordinator` type
/// is defined later in [`crate::coordinator`]; passing it as `Value`
/// here keeps `traits.rs` free of circular dependencies.
///
/// # Object safety
///
/// This trait is object-safe: `Arc<dyn Orchestrator>` is the standard storage type.
pub trait Orchestrator: Send + Sync {
    /// Run the agent loop for a single prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` — User input text.
    /// * `context` — Context manager for conversation state.
    /// * `providers` — Named LLM providers available for this session.
    /// * `tools` — Named tools available for this session.
    /// * `hooks` — Hook dispatch context (serialised; the concrete
    ///   `HookRegistry` is defined in [`crate::hooks`]).
    /// * `coordinator` — Module coordinator context (serialised; the
    ///   concrete `Coordinator` is defined in [`crate::coordinator`]).
    ///
    /// # Returns
    ///
    /// The final response string on success, or an [`AmplifierError`].
    fn execute(
        &self,
        prompt: String,
        context: Arc<dyn ContextManager>,
        providers: HashMap<String, Arc<dyn Provider>>,
        tools: HashMap<String, Arc<dyn Tool>>,
        hooks: Value,
        coordinator: Value,
    ) -> Pin<Box<dyn Future<Output = Result<String, AmplifierError>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// ContextManager
// ---------------------------------------------------------------------------

/// Interface for context management modules.
///
/// Context managers own memory policy. Orchestrators ask for messages;
/// context managers decide how to fit them within limits. This maintains
/// clean mechanism/policy separation — orchestrators are mechanisms that
/// request messages, context managers are policies that decide what to return.
///
/// # Python equivalent
///
/// ```python
/// class ContextManager(Protocol):
///     async def add_message(self, message: dict) -> None: ...
///     async def get_messages_for_request(
///         self, token_budget=None, provider=None,
///     ) -> list[dict]: ...
///     async def get_messages(self) -> list[dict]: ...
///     async def set_messages(self, messages: list[dict]) -> None: ...
///     async def clear(self) -> None: ...
/// ```
///
/// Messages are represented as [`Value`] (JSON) matching the Python
/// convention where contexts store `dict[str, Any]`.
///
/// # Object safety
///
/// This trait is object-safe: `Arc<dyn ContextManager>` is the standard storage type.
pub trait ContextManager: Send + Sync {
    /// Append a message to the context history.
    ///
    /// * `message` — JSON object with at least `"role"` and `"content"` keys.
    fn add_message(
        &self,
        message: Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>>;

    /// Get messages ready for an LLM request, compacted if necessary.
    ///
    /// The context manager handles any compaction needed internally.
    /// Orchestrators call this before every LLM request and trust the
    /// context manager to return messages that fit within limits.
    ///
    /// # Arguments
    ///
    /// * `token_budget` — Optional explicit token limit.
    /// * `provider` — Optional provider for dynamic budget calculation
    ///   (budget = context_window − max_output_tokens − safety_margin).
    fn get_messages_for_request(
        &self,
        token_budget: Option<i64>,
        provider: Option<Arc<dyn Provider>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>>;

    /// Get all messages (raw, uncompacted) for transcripts/debugging.
    fn get_messages(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>>;

    /// Replace the entire message list (for session resume).
    fn set_messages(
        &self,
        messages: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>>;

    /// Clear all messages from context.
    fn clear(&self) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// HookHandler
// ---------------------------------------------------------------------------

/// Interface for hook handlers.
///
/// Hook handlers are callables that respond to lifecycle events emitted by
/// the kernel. They return a [`HookResult`] indicating what action to take
/// (continue, deny, modify, inject context, or ask user).
///
/// # Python equivalent
///
/// ```python
/// class HookHandler(Protocol):
///     async def __call__(self, event: str, data: dict) -> HookResult: ...
/// ```
///
/// In Rust the method is named `handle` (since `__call__` is Python-specific).
///
/// # Object safety
///
/// This trait is object-safe: `Arc<dyn HookHandler>` is the standard storage type.
pub trait HookHandler: Send + Sync {
    /// Handle a lifecycle event.
    ///
    /// # Arguments
    ///
    /// * `event` — Canonical event name (see [`crate::events`]).
    /// * `data` — Event payload as a JSON value.
    ///
    /// # Returns
    ///
    /// A [`HookResult`] with the desired action and any associated data.
    /// Errors are reported via [`HookError`] and do **not** short-circuit
    /// the handler chain — the registry logs them and continues.
    fn handle(
        &self,
        event: &str,
        data: Value,
    ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// ApprovalProvider
// ---------------------------------------------------------------------------

/// Interface for UI components that provide approval dialogs.
///
/// When a hook returns `action: "ask_user"`, the kernel asks the registered
/// `ApprovalProvider` to present the request to the user and return their
/// decision.
///
/// # Python equivalent
///
/// ```python
/// class ApprovalProvider(Protocol):
///     async def request_approval(
///         self, request: ApprovalRequest,
///     ) -> ApprovalResponse: ...
/// ```
///
/// # Object safety
///
/// This trait is object-safe: `Arc<dyn ApprovalProvider>` is the standard storage type.
pub trait ApprovalProvider: Send + Sync {
    /// Request approval from the user.
    ///
    /// # Arguments
    ///
    /// * `request` — Describes the action, risk level, and optional timeout.
    ///
    /// # Returns
    ///
    /// `Ok(ApprovalResponse)` with the user's decision.
    /// `Err(AmplifierError)` on timeout or infrastructure failure.
    fn request_approval(
        &self,
        request: ApprovalRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ApprovalResponse, AmplifierError>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify all traits are object-safe (can be used as `Arc<dyn Trait>`).
    ///
    /// If any trait is not object-safe, this test fails at **compile time**.
    #[test]
    fn traits_are_object_safe() {
        fn _assert_tool(_: Arc<dyn Tool>) {}
        fn _assert_provider(_: Arc<dyn Provider>) {}
        fn _assert_orchestrator(_: Arc<dyn Orchestrator>) {}
        fn _assert_context(_: Arc<dyn ContextManager>) {}
        fn _assert_hook(_: Arc<dyn HookHandler>) {}
        fn _assert_approval(_: Arc<dyn ApprovalProvider>) {}
    }
}
