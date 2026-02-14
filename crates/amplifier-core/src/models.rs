//! Core data models for the Amplifier kernel.
//!
//! Ports the data models from `amplifier_core/models.py` to Rust.
//! All structs use `serde` for JSON serialization, matching the Python
//! Pydantic models field-for-field.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Action type for hook results.
///
/// Determines how the hook pipeline processes the event:
/// - `Continue` — proceed normally
/// - `Deny` — block the operation (short-circuits handler chain)
/// - `Modify` — modify event data (chains through handlers)
/// - `InjectContext` — add content to agent's conversation context
/// - `AskUser` — request user approval before proceeding
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookAction {
    Continue,
    Deny,
    Modify,
    InjectContext,
    AskUser,
}

impl Default for HookAction {
    fn default() -> Self {
        Self::Continue
    }
}

/// Role for context injection messages.
///
/// - `System` (default) — environmental feedback
/// - `User` — simulate user input
/// - `Assistant` — agent self-talk
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextInjectionRole {
    System,
    User,
    Assistant,
}

impl Default for ContextInjectionRole {
    fn default() -> Self {
        Self::System
    }
}

/// Default decision on approval timeout or error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDefault {
    Allow,
    Deny,
}

impl Default for ApprovalDefault {
    fn default() -> Self {
        Self::Deny
    }
}

/// Severity level for user messages from hooks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMessageLevel {
    Info,
    Warning,
    Error,
}

impl Default for UserMessageLevel {
    fn default() -> Self {
        Self::Info
    }
}

/// Configuration field type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldType {
    Text,
    Secret,
    Choice,
    Boolean,
}

impl Default for ConfigFieldType {
    fn default() -> Self {
        Self::Text
    }
}

/// Module type classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleType {
    Orchestrator,
    Provider,
    Tool,
    Context,
    Hook,
    Resolver,
}

/// Session state.
///
/// Matches the Python `Literal["running", "completed", "failed", "cancelled"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::Running
    }
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Result from hook execution with enhanced capabilities.
///
/// Hooks can observe, block, modify operations, inject context to the agent,
/// request user approval, and control output visibility. These capabilities
/// enable hooks to participate in the agent's cognitive loop.
///
/// # Actions
///
/// - `continue`: Proceed normally with the operation
/// - `deny`: Block the operation (short-circuits handler chain)
/// - `modify`: Modify event data (chains through handlers)
/// - `inject_context`: Add content to agent's context (enables feedback loops)
/// - `ask_user`: Request user approval before proceeding (dynamic permissions)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookResult {
    /// Action to take.
    #[serde(default)]
    pub action: HookAction,

    /// Modified event data (for action='modify'). Changes chain through handlers.
    #[serde(default)]
    pub data: Option<HashMap<String, Value>>,

    /// Explanation for deny/modification. Shown to agent when operation is blocked.
    #[serde(default)]
    pub reason: Option<String>,

    // -- Context injection fields --

    /// Text to inject into agent's conversation context (for action='inject_context').
    /// Agent sees this content and can respond to it. Enables automated feedback loops.
    #[serde(default)]
    pub context_injection: Option<String>,

    /// Role for injected message in conversation.
    #[serde(default)]
    pub context_injection_role: ContextInjectionRole,

    /// If true, injection is temporary (only for current LLM call, not stored in history).
    #[serde(default)]
    pub ephemeral: bool,

    // -- Approval gate fields --

    /// Question to ask user (for action='ask_user').
    #[serde(default)]
    pub approval_prompt: Option<String>,

    /// User choice options for approval.
    #[serde(default)]
    pub approval_options: Option<Vec<String>>,

    /// Seconds to wait for user response. Default 300.0 (5 minutes).
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout: f64,

    /// Default decision on timeout or error.
    #[serde(default)]
    pub approval_default: ApprovalDefault,

    // -- Output control fields --

    /// Hide hook's stdout/stderr from user transcript.
    #[serde(default)]
    pub suppress_output: bool,

    /// Message to display to user (separate from context_injection).
    #[serde(default)]
    pub user_message: Option<String>,

    /// Severity level for user_message.
    #[serde(default)]
    pub user_message_level: UserMessageLevel,

    /// Source name for user_message display (e.g., 'python-check').
    #[serde(default)]
    pub user_message_source: Option<String>,

    // -- Injection placement control --

    /// If true and ephemeral=true, append context_injection to the last tool result
    /// message instead of creating a new message.
    #[serde(default)]
    pub append_to_last_tool_result: bool,

    /// Extension fields for forward-compatibility.
    /// Captures any unknown JSON keys during deserialization.
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

fn default_approval_timeout() -> f64 {
    300.0
}

impl Default for HookResult {
    fn default() -> Self {
        Self {
            action: HookAction::default(),
            data: None,
            reason: None,
            context_injection: None,
            context_injection_role: ContextInjectionRole::default(),
            ephemeral: false,
            approval_prompt: None,
            approval_options: None,
            approval_timeout: default_approval_timeout(),
            approval_default: ApprovalDefault::default(),
            suppress_output: false,
            user_message: None,
            user_message_level: UserMessageLevel::default(),
            user_message_source: None,
            append_to_last_tool_result: false,
            extensions: HashMap::new(),
        }
    }
}

/// Result from tool execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether execution succeeded.
    #[serde(default = "default_true")]
    pub success: bool,

    /// Tool output data.
    #[serde(default)]
    pub output: Option<Value>,

    /// Error details if failed.
    #[serde(default)]
    pub error: Option<HashMap<String, Value>>,
}

fn default_true() -> bool {
    true
}

impl Default for ToolResult {
    fn default() -> Self {
        Self {
            success: true,
            output: None,
            error: None,
        }
    }
}

/// Model metadata for provider models.
///
/// Describes capabilities and defaults for a specific model available from a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-sonnet-4-5", "gpt-5.2").
    pub id: String,

    /// Human-readable model name.
    pub display_name: String,

    /// Maximum context window in tokens.
    pub context_window: i64,

    /// Maximum output tokens.
    pub max_output_tokens: i64,

    /// Extensible capability list (e.g., "tools", "vision", "streaming").
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Model-specific default config values (e.g., temperature, max_tokens).
    #[serde(default)]
    pub defaults: HashMap<String, Value>,
}

/// A configuration field that a provider needs, with prompt metadata.
///
/// Providers define their configuration needs through these fields. The app-cli
/// renders them generically into interactive prompts, keeping all provider-specific
/// logic in the provider modules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigField {
    /// Field identifier (used as key in config dict).
    pub id: String,

    /// Human-readable label for prompts.
    pub display_name: String,

    /// Field type: "text", "secret", "choice", "boolean".
    #[serde(default)]
    pub field_type: ConfigFieldType,

    /// Question to ask the user.
    pub prompt: String,

    /// Environment variable to check/set.
    #[serde(default)]
    pub env_var: Option<String>,

    /// Valid choices (for field_type='choice').
    #[serde(default)]
    pub choices: Option<Vec<String>>,

    /// Whether this field is required.
    #[serde(default = "default_true")]
    pub required: bool,

    /// Default value if not provided.
    #[serde(default, rename = "default")]
    pub default_value: Option<String>,

    /// Conditional visibility: show this field only when another field
    /// has a specific value (e.g., `{"model": "claude-sonnet-4-5"}`).
    #[serde(default)]
    pub show_when: Option<HashMap<String, String>>,

    /// If true, this field is shown after model selection.
    #[serde(default)]
    pub requires_model: bool,
}

/// Provider metadata.
///
/// Describes capabilities, authentication requirements, and defaults for a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider identifier (e.g., "anthropic", "openai").
    pub id: String,

    /// Human-readable provider name.
    pub display_name: String,

    /// Environment variables for credentials (e.g., `["ANTHROPIC_API_KEY"]`).
    #[serde(default)]
    pub credential_env_vars: Vec<String>,

    /// Extensible capability list (e.g., "streaming", "batch", "embeddings").
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Provider-level default config values (e.g., timeout, max_retries).
    #[serde(default)]
    pub defaults: HashMap<String, Value>,

    /// Configuration fields for interactive setup.
    #[serde(default)]
    pub config_fields: Vec<ConfigField>,
}

/// Module metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleInfo {
    /// Module identifier.
    pub id: String,

    /// Module display name.
    pub name: String,

    /// Module version.
    pub version: String,

    /// Module type.
    #[serde(rename = "type")]
    pub module_type: ModuleType,

    /// Where module should be mounted.
    pub mount_point: String,

    /// Module description.
    pub description: String,

    /// JSON schema for module configuration.
    #[serde(default)]
    pub config_schema: Option<Value>,
}

/// Session status and metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStatus {
    /// Unique session ID.
    pub session_id: String,

    /// When the session started (ISO 8601 string).
    pub started_at: String,

    /// When the session ended (ISO 8601 string).
    #[serde(default)]
    pub ended_at: Option<String>,

    /// Current session state.
    #[serde(default)]
    pub status: SessionState,

    // Counters

    /// Total number of messages.
    #[serde(default)]
    pub total_messages: i64,

    /// Number of tool invocations.
    #[serde(default)]
    pub tool_invocations: i64,

    /// Number of successful tool executions.
    #[serde(default)]
    pub tool_successes: i64,

    /// Number of failed tool executions.
    #[serde(default)]
    pub tool_failures: i64,

    // Token usage

    /// Total input tokens consumed.
    #[serde(default)]
    pub total_input_tokens: i64,

    /// Total output tokens produced.
    #[serde(default)]
    pub total_output_tokens: i64,

    // Cost tracking

    /// Estimated cost (if available).
    #[serde(default)]
    pub estimated_cost: Option<f64>,

    // Last activity

    /// Last activity timestamp (ISO 8601 string).
    #[serde(default)]
    pub last_activity: Option<String>,

    /// Last error details.
    #[serde(default)]
    pub last_error: Option<HashMap<String, Value>>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- HookResult tests (from PLAN) ---

    #[test]
    fn hook_result_default_is_continue() {
        let result = HookResult::default();
        assert_eq!(result.action, HookAction::Continue);
        assert!(result.data.is_none());
        assert!(result.reason.is_none());
    }

    #[test]
    fn hook_result_deny_with_reason() {
        let result = HookResult {
            action: HookAction::Deny,
            reason: Some("blocked".into()),
            ..Default::default()
        };
        assert_eq!(result.action, HookAction::Deny);
        assert_eq!(result.reason.as_deref(), Some("blocked"));
    }

    #[test]
    fn hook_result_inject_context_defaults() {
        let result = HookResult::default();
        assert!(result.context_injection.is_none());
        assert_eq!(result.context_injection_role, ContextInjectionRole::System);
        assert!(!result.ephemeral);
    }

    #[test]
    fn hook_result_approval_defaults() {
        let result = HookResult::default();
        assert_eq!(result.approval_timeout, 300.0);
        assert_eq!(result.approval_default, ApprovalDefault::Deny);
        assert!(result.approval_prompt.is_none());
        assert!(result.approval_options.is_none());
    }

    #[test]
    fn hook_result_output_control_defaults() {
        let result = HookResult::default();
        assert!(!result.suppress_output);
        assert!(result.user_message.is_none());
        assert_eq!(result.user_message_level, UserMessageLevel::Info);
        assert!(result.user_message_source.is_none());
        assert!(!result.append_to_last_tool_result);
    }

    #[test]
    fn hook_result_extensions_capture_unknown_keys() {
        let json = r#"{"action": "continue", "custom_key": "custom_value"}"#;
        let result: HookResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.action, HookAction::Continue);
        assert_eq!(
            result.extensions.get("custom_key"),
            Some(&json!("custom_value"))
        );
    }

    #[test]
    fn hook_result_serialization_roundtrip() {
        let result = HookResult {
            action: HookAction::InjectContext,
            context_injection: Some("Linter error on line 42".into()),
            context_injection_role: ContextInjectionRole::System,
            suppress_output: true,
            user_message: Some("Found issues".into()),
            user_message_level: UserMessageLevel::Warning,
            ..Default::default()
        };
        let json_str = serde_json::to_string(&result).unwrap();
        let deserialized: HookResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.action, HookAction::InjectContext);
        assert_eq!(
            deserialized.context_injection.as_deref(),
            Some("Linter error on line 42")
        );
        assert!(deserialized.suppress_output);
        assert_eq!(deserialized.user_message_level, UserMessageLevel::Warning);
    }

    // --- HookAction tests (from PLAN) ---

    #[test]
    fn hook_action_serializes_as_lowercase_string() {
        let action = HookAction::InjectContext;
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json, json!("inject_context"));
    }

    #[test]
    fn hook_action_all_variants_serialize() {
        assert_eq!(
            serde_json::to_value(HookAction::Continue).unwrap(),
            json!("continue")
        );
        assert_eq!(
            serde_json::to_value(HookAction::Deny).unwrap(),
            json!("deny")
        );
        assert_eq!(
            serde_json::to_value(HookAction::Modify).unwrap(),
            json!("modify")
        );
        assert_eq!(
            serde_json::to_value(HookAction::InjectContext).unwrap(),
            json!("inject_context")
        );
        assert_eq!(
            serde_json::to_value(HookAction::AskUser).unwrap(),
            json!("ask_user")
        );
    }

    // --- ToolResult tests (from PLAN) ---

    #[test]
    fn tool_result_success_default() {
        let result = ToolResult::default();
        assert!(result.success);
        assert!(result.output.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn tool_result_serialization_roundtrip() {
        let result = ToolResult {
            success: true,
            output: Some(json!({"key": "value"})),
            error: None,
        };
        let json_str = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.success, result.success);
    }

    #[test]
    fn tool_result_with_error() {
        let result = ToolResult {
            success: false,
            output: None,
            error: Some(HashMap::from([(
                "message".to_string(),
                json!("command failed"),
            )])),
        };
        assert!(!result.success);
        assert_eq!(
            result.error.as_ref().unwrap().get("message"),
            Some(&json!("command failed"))
        );
    }

    // --- ModelInfo tests (from PLAN) ---

    #[test]
    fn model_info_with_defaults() {
        let info = ModelInfo {
            id: "gpt-4".into(),
            display_name: "GPT-4".into(),
            context_window: 128000,
            max_output_tokens: 4096,
            capabilities: vec!["streaming".into()],
            defaults: Default::default(),
        };
        assert_eq!(info.id, "gpt-4");
    }

    #[test]
    fn model_info_serialization_roundtrip() {
        let info = ModelInfo {
            id: "claude-sonnet-4-5".into(),
            display_name: "Claude Sonnet 4.5".into(),
            context_window: 200000,
            max_output_tokens: 8192,
            capabilities: vec!["tools".into(), "vision".into(), "streaming".into()],
            defaults: HashMap::from([("temperature".into(), json!(0.7))]),
        };
        let json_str = serde_json::to_string(&info).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.id, info.id);
        assert_eq!(deserialized.capabilities.len(), 3);
        assert_eq!(deserialized.defaults.get("temperature"), Some(&json!(0.7)));
    }

    // --- ConfigField tests ---

    #[test]
    fn config_field_type_default_is_text() {
        let field = ConfigField {
            id: "api_key".into(),
            display_name: "API Key".into(),
            field_type: ConfigFieldType::default(),
            prompt: "Enter your API key".into(),
            env_var: Some("API_KEY".into()),
            choices: None,
            required: true,
            default_value: None,
            show_when: None,
            requires_model: false,
        };
        assert_eq!(field.field_type, ConfigFieldType::Text);
        assert!(field.required);
    }

    // --- ProviderInfo tests ---

    #[test]
    fn provider_info_roundtrip() {
        let info = ProviderInfo {
            id: "anthropic".into(),
            display_name: "Anthropic".into(),
            credential_env_vars: vec!["ANTHROPIC_API_KEY".into()],
            capabilities: vec!["streaming".into(), "tools".into()],
            defaults: HashMap::from([("timeout".into(), json!(30))]),
            config_fields: vec![],
        };
        let json_str = serde_json::to_string(&info).unwrap();
        let deserialized: ProviderInfo = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.id, "anthropic");
        assert_eq!(deserialized.credential_env_vars.len(), 1);
    }

    // --- ModuleInfo / ModuleType tests ---

    #[test]
    fn module_type_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_value(ModuleType::Orchestrator).unwrap(),
            json!("orchestrator")
        );
        assert_eq!(
            serde_json::to_value(ModuleType::Provider).unwrap(),
            json!("provider")
        );
        assert_eq!(
            serde_json::to_value(ModuleType::Tool).unwrap(),
            json!("tool")
        );
        assert_eq!(
            serde_json::to_value(ModuleType::Context).unwrap(),
            json!("context")
        );
        assert_eq!(
            serde_json::to_value(ModuleType::Hook).unwrap(),
            json!("hook")
        );
        assert_eq!(
            serde_json::to_value(ModuleType::Resolver).unwrap(),
            json!("resolver")
        );
    }

    #[test]
    fn module_info_serialization_roundtrip() {
        let info = ModuleInfo {
            id: "bash-tool".into(),
            name: "Bash Tool".into(),
            version: "1.0.0".into(),
            module_type: ModuleType::Tool,
            mount_point: "tools".into(),
            description: "Execute bash commands".into(),
            config_schema: None,
        };
        let json_str = serde_json::to_string(&info).unwrap();
        let deserialized: ModuleInfo = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.id, "bash-tool");
        assert_eq!(deserialized.module_type, ModuleType::Tool);
        // Verify "type" is used as JSON key (not "module_type")
        let json_val: Value = serde_json::from_str(&json_str).unwrap();
        assert!(json_val.get("type").is_some());
        assert!(json_val.get("module_type").is_none());
    }

    // --- SessionState / SessionStatus tests ---

    #[test]
    fn session_state_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_value(SessionState::Running).unwrap(),
            json!("running")
        );
        assert_eq!(
            serde_json::to_value(SessionState::Completed).unwrap(),
            json!("completed")
        );
        assert_eq!(
            serde_json::to_value(SessionState::Failed).unwrap(),
            json!("failed")
        );
        assert_eq!(
            serde_json::to_value(SessionState::Cancelled).unwrap(),
            json!("cancelled")
        );
    }

    #[test]
    fn session_status_roundtrip() {
        let status = SessionStatus {
            session_id: "sess-123".into(),
            started_at: "2025-01-01T00:00:00Z".into(),
            ended_at: None,
            status: SessionState::Running,
            total_messages: 5,
            tool_invocations: 3,
            tool_successes: 2,
            tool_failures: 1,
            total_input_tokens: 1000,
            total_output_tokens: 500,
            estimated_cost: Some(0.05),
            last_activity: Some("2025-01-01T00:01:00Z".into()),
            last_error: None,
        };
        let json_str = serde_json::to_string(&status).unwrap();
        let deserialized: SessionStatus = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.session_id, "sess-123");
        assert_eq!(deserialized.status, SessionState::Running);
        assert_eq!(deserialized.total_messages, 5);
        assert_eq!(deserialized.estimated_cost, Some(0.05));
    }

    #[test]
    fn session_status_defaults_from_json() {
        let json = r#"{"session_id": "s1", "started_at": "2025-01-01T00:00:00Z"}"#;
        let status: SessionStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.status, SessionState::Running);
        assert_eq!(status.total_messages, 0);
        assert_eq!(status.tool_invocations, 0);
        assert!(status.ended_at.is_none());
    }
}
