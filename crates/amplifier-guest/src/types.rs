use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Specification for a tool exposed by a WASM module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub parameters: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Result returned from a tool execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    #[serde(default = "default_true")]
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// A subscription declaring which event a hook wants to receive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscription {
    pub event: String,
    pub priority: i32,
    pub name: String,
}

/// Action a hook handler can take in response to a lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookAction {
    Continue,
    Deny,
    Modify,
    InjectContext,
    AskUser,
}

/// Role for injected context messages.
/// Serializes with default PascalCase (e.g. "System", "User") per spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextInjectionRole {
    System,
    User,
    Assistant,
}

/// Default behavior when approval times out.
/// Serializes with default PascalCase (e.g. "Allow", "Deny") per spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalDefault {
    Allow,
    Deny,
}

/// Severity level for user-facing messages.
/// Serializes with default PascalCase (e.g. "Info", "Warning") per spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserMessageLevel {
    Info,
    Warning,
    Error,
}

/// Full result returned by a hook handler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookResult {
    pub action: HookAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_injection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_injection_role: Option<ContextInjectionRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_options: Option<Vec<String>>,
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_default: Option<ApprovalDefault>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message_level: Option<UserMessageLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append_to_last_tool_result: Option<Value>,
}

fn default_approval_timeout() -> f64 {
    300.0
}

impl Default for HookResult {
    fn default() -> Self {
        Self {
            action: HookAction::Continue,
            data: None,
            reason: None,
            context_injection: None,
            context_injection_role: None,
            ephemeral: None,
            approval_prompt: None,
            approval_options: None,
            approval_timeout: default_approval_timeout(),
            approval_default: None,
            suppress_output: None,
            user_message: None,
            user_message_level: None,
            user_message_source: None,
            append_to_last_tool_result: None,
        }
    }
}

/// Request for human-in-the-loop approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub action: String,
    pub details: HashMap<String, Value>,
    pub risk_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
}

/// Response from the approval provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub remember: bool,
}

/// Metadata about an LLM provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub credential_env_vars: Vec<String>,
    pub capabilities: Vec<String>,
    pub defaults: HashMap<String, Value>,
}

/// Metadata about a specific model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub context_window: i64,
    pub max_output_tokens: i64,
    pub capabilities: Vec<String>,
    pub defaults: HashMap<String, Value>,
}

/// Request for an LLM chat completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from an LLM chat completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::HashMap;

    // --- ToolSpec tests ---

    #[test]
    fn test_tool_spec_creation() {
        let mut params = HashMap::new();
        params.insert("arg1".to_string(), json!("string"));
        let spec = ToolSpec {
            name: "my_tool".to_string(),
            parameters: params,
            description: Some("A test tool".to_string()),
        };
        assert_eq!(spec.name, "my_tool");
        assert!(spec.description.is_some());
    }

    #[test]
    fn test_tool_spec_serde_roundtrip() {
        let mut params = HashMap::new();
        params.insert("x".to_string(), json!(42));
        let spec = ToolSpec {
            name: "calc".to_string(),
            parameters: params,
            description: None,
        };
        let json_str = serde_json::to_string(&spec).unwrap();
        let deserialized: ToolSpec = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.name, "calc");
        assert_eq!(deserialized.parameters.get("x"), Some(&json!(42)));
        assert!(deserialized.description.is_none());
    }

    // --- ToolResult tests ---

    #[test]
    fn test_tool_result_defaults() {
        let result = ToolResult::default();
        assert!(result.success);
        assert!(result.output.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tool_result_serde_roundtrip() {
        let result = ToolResult {
            success: false,
            output: Some(json!("hello")),
            error: Some({
                let mut m = HashMap::new();
                m.insert("code".to_string(), json!(404));
                m
            }),
        };
        let json_str = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&json_str).unwrap();
        assert!(!deserialized.success);
        assert_eq!(deserialized.output, Some(json!("hello")));
        let err = deserialized.error.as_ref().unwrap();
        assert_eq!(err.get("code"), Some(&json!(404)));
    }

    // --- HookAction tests ---

    #[test]
    fn test_hook_action_serde_snake_case() {
        let action = HookAction::InjectContext;
        let json_str = serde_json::to_string(&action).unwrap();
        assert_eq!(json_str, "\"inject_context\"");

        let action = HookAction::AskUser;
        let json_str = serde_json::to_string(&action).unwrap();
        assert_eq!(json_str, "\"ask_user\"");
    }

    #[test]
    fn test_hook_action_all_variants() {
        let variants = vec![
            HookAction::Continue,
            HookAction::Deny,
            HookAction::Modify,
            HookAction::InjectContext,
            HookAction::AskUser,
        ];
        for v in variants {
            let s = serde_json::to_string(&v).unwrap();
            let back: HookAction = serde_json::from_str(&s).unwrap();
            assert_eq!(format!("{:?}", v), format!("{:?}", back));
        }
    }

    // --- ContextInjectionRole tests ---

    #[test]
    fn test_context_injection_role_variants() {
        let roles = vec![
            ContextInjectionRole::System,
            ContextInjectionRole::User,
            ContextInjectionRole::Assistant,
        ];
        for r in roles {
            let s = serde_json::to_string(&r).unwrap();
            let back: ContextInjectionRole = serde_json::from_str(&s).unwrap();
            assert_eq!(format!("{:?}", r), format!("{:?}", back));
        }
    }

    // --- ApprovalDefault tests ---

    #[test]
    fn test_approval_default_variants() {
        let vals = vec![ApprovalDefault::Allow, ApprovalDefault::Deny];
        for v in vals {
            let s = serde_json::to_string(&v).unwrap();
            let back: ApprovalDefault = serde_json::from_str(&s).unwrap();
            assert_eq!(format!("{:?}", v), format!("{:?}", back));
        }
    }

    // --- UserMessageLevel tests ---

    #[test]
    fn test_user_message_level_variants() {
        let vals = vec![
            UserMessageLevel::Info,
            UserMessageLevel::Warning,
            UserMessageLevel::Error,
        ];
        for v in vals {
            let s = serde_json::to_string(&v).unwrap();
            let back: UserMessageLevel = serde_json::from_str(&s).unwrap();
            assert_eq!(format!("{:?}", v), format!("{:?}", back));
        }
    }

    // --- HookResult tests ---

    #[test]
    fn test_hook_result_defaults() {
        let hr = HookResult::default();
        assert_eq!(hr.approval_timeout, 300.0);
    }

    #[test]
    fn test_hook_result_serde_roundtrip() {
        let hr = HookResult {
            action: HookAction::Continue,
            data: None,
            reason: Some("test reason".to_string()),
            context_injection: None,
            context_injection_role: None,
            ephemeral: None,
            approval_prompt: None,
            approval_options: None,
            approval_timeout: 300.0,
            approval_default: None,
            suppress_output: None,
            user_message: None,
            user_message_level: None,
            user_message_source: None,
            append_to_last_tool_result: None,
        };
        let json_str = serde_json::to_string(&hr).unwrap();
        let deserialized: HookResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.action, HookAction::Continue);
        assert!(deserialized.data.is_none());
        assert_eq!(deserialized.reason, Some("test reason".to_string()));
        assert!(deserialized.context_injection.is_none());
        assert!(deserialized.context_injection_role.is_none());
        assert!(deserialized.ephemeral.is_none());
        assert!(deserialized.approval_prompt.is_none());
        assert!(deserialized.approval_options.is_none());
        assert_eq!(deserialized.approval_timeout, 300.0);
        assert!(deserialized.approval_default.is_none());
        assert!(deserialized.suppress_output.is_none());
        assert!(deserialized.user_message.is_none());
        assert!(deserialized.user_message_level.is_none());
        assert!(deserialized.user_message_source.is_none());
        assert!(deserialized.append_to_last_tool_result.is_none());
    }

    // --- ApprovalRequest tests ---

    #[test]
    fn test_approval_request_creation() {
        let req = ApprovalRequest {
            tool_name: "rm".to_string(),
            action: "delete".to_string(),
            details: {
                let mut m = HashMap::new();
                m.insert("path".to_string(), json!("/tmp/test"));
                m
            },
            risk_level: "high".to_string(),
            timeout: Some(60.0),
        };
        assert_eq!(req.tool_name, "rm");
        assert_eq!(req.risk_level, "high");
    }

    #[test]
    fn test_approval_request_serde_roundtrip() {
        let req = ApprovalRequest {
            tool_name: "tool".to_string(),
            action: "exec".to_string(),
            details: HashMap::new(),
            risk_level: "low".to_string(),
            timeout: None,
        };
        let json_str = serde_json::to_string(&req).unwrap();
        let deserialized: ApprovalRequest = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.tool_name, "tool");
        assert_eq!(deserialized.action, "exec");
        assert!(deserialized.details.is_empty());
        assert_eq!(deserialized.risk_level, "low");
        assert!(deserialized.timeout.is_none());
    }

    // --- ApprovalResponse tests ---

    #[test]
    fn test_approval_response_creation() {
        let resp = ApprovalResponse {
            approved: true,
            reason: Some("looks safe".to_string()),
            remember: false,
        };
        assert!(resp.approved);
        assert!(!resp.remember);
    }

    // --- ProviderInfo tests ---

    #[test]
    fn test_provider_info_creation() {
        let info = ProviderInfo {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            credential_env_vars: vec!["OPENAI_API_KEY".to_string()],
            capabilities: vec!["chat".to_string()],
            defaults: {
                let mut m = HashMap::new();
                m.insert("model".to_string(), json!("gpt-4"));
                m
            },
        };
        assert_eq!(info.id, "openai");
        assert_eq!(info.credential_env_vars.len(), 1);
    }

    // --- ModelInfo tests ---

    #[test]
    fn test_model_info_creation() {
        let info = ModelInfo {
            id: "gpt-4".to_string(),
            display_name: "GPT-4".to_string(),
            context_window: 128000,
            max_output_tokens: 4096,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
            defaults: HashMap::new(),
        };
        assert_eq!(info.context_window, 128000);
        assert_eq!(info.max_output_tokens, 4096);
    }

    // --- ChatRequest tests ---

    #[test]
    fn test_chat_request_serde_roundtrip() {
        let req = ChatRequest {
            messages: vec![json!({"role": "user", "content": "hello"})],
            model: Some("gpt-4".to_string()),
            temperature: Some(0.7),
            max_output_tokens: Some(1024),
            extra: {
                let mut m = HashMap::new();
                m.insert("stream".to_string(), json!(false));
                m
            },
        };
        let json_str = serde_json::to_string(&req).unwrap();
        // #[serde(flatten)] causes extra fields to appear at the top level in JSON.
        // On deserialization, any top-level key not matching a named field is absorbed
        // into the `extra` HashMap, providing extensible wire-format support.
        let v: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["model"], json!("gpt-4"));
        assert_eq!(v["stream"], json!(false));

        let deserialized: ChatRequest = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.model, Some("gpt-4".to_string()));
        assert_eq!(deserialized.extra.get("stream"), Some(&json!(false)));
    }

    // --- ChatResponse tests ---

    #[test]
    fn test_chat_response_serde_roundtrip() {
        let resp = ChatResponse {
            content: vec![json!({"type": "text", "text": "Hello!"})],
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            extra: HashMap::new(),
        };
        let json_str = serde_json::to_string(&resp).unwrap();
        let deserialized: ChatResponse = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.content.len(), 1);
        assert_eq!(deserialized.content[0]["text"], json!("Hello!"));
        assert!(deserialized.tool_calls.is_none());
        assert_eq!(deserialized.finish_reason, Some("stop".to_string()));
        assert!(deserialized.extra.is_empty());
    }

    // --- PartialEq roundtrip tests ---

    #[test]
    fn test_tool_spec_partial_eq_roundtrip() {
        let mut params = HashMap::new();
        params.insert("x".to_string(), json!(42));
        let original = ToolSpec {
            name: "calc".to_string(),
            parameters: params,
            description: Some("calculator".to_string()),
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ToolSpec = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_tool_result_partial_eq_roundtrip() {
        let original = ToolResult {
            success: false,
            output: Some(json!("hello")),
            error: Some({
                let mut m = HashMap::new();
                m.insert("code".to_string(), json!(404));
                m
            }),
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_hook_result_partial_eq_roundtrip() {
        let original = HookResult {
            action: HookAction::InjectContext,
            data: Some(json!({"key": "value"})),
            reason: Some("test reason".to_string()),
            context_injection: Some("injected".to_string()),
            context_injection_role: Some(ContextInjectionRole::System),
            ephemeral: Some(true),
            approval_prompt: Some("approve?".to_string()),
            approval_options: Some(vec!["yes".to_string(), "no".to_string()]),
            approval_timeout: 300.0,
            approval_default: Some(ApprovalDefault::Deny),
            suppress_output: Some(false),
            user_message: Some("msg".to_string()),
            user_message_level: Some(UserMessageLevel::Warning),
            user_message_source: Some("hook".to_string()),
            append_to_last_tool_result: Some(json!("extra")),
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: HookResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_approval_request_partial_eq_roundtrip() {
        let original = ApprovalRequest {
            tool_name: "rm".to_string(),
            action: "delete".to_string(),
            details: {
                let mut m = HashMap::new();
                m.insert("path".to_string(), json!("/tmp/test"));
                m
            },
            risk_level: "high".to_string(),
            timeout: Some(60.0),
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ApprovalRequest = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_approval_response_partial_eq_roundtrip() {
        let original = ApprovalResponse {
            approved: true,
            reason: Some("looks safe".to_string()),
            remember: false,
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ApprovalResponse = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_provider_info_partial_eq_roundtrip() {
        let original = ProviderInfo {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            credential_env_vars: vec!["OPENAI_API_KEY".to_string()],
            capabilities: vec!["chat".to_string()],
            defaults: {
                let mut m = HashMap::new();
                m.insert("model".to_string(), json!("gpt-4"));
                m
            },
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ProviderInfo = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_model_info_partial_eq_roundtrip() {
        let original = ModelInfo {
            id: "gpt-4".to_string(),
            display_name: "GPT-4".to_string(),
            context_window: 128000,
            max_output_tokens: 4096,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
            defaults: HashMap::new(),
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, original);
    }

    // --- EventSubscription tests ---

    #[test]
    fn test_event_subscription_creation() {
        let sub = EventSubscription {
            event: "before_tool".to_string(),
            priority: 10,
            name: "my-hook".to_string(),
        };
        assert_eq!(sub.event, "before_tool");
        assert_eq!(sub.priority, 10);
        assert_eq!(sub.name, "my-hook");
    }

    #[test]
    fn test_event_subscription_serde_roundtrip() {
        let sub = EventSubscription {
            event: "after_tool".to_string(),
            priority: -5,
            name: "cleanup-hook".to_string(),
        };
        let json_str = serde_json::to_string(&sub).unwrap();
        let deserialized: EventSubscription = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.event, "after_tool");
        assert_eq!(deserialized.priority, -5);
        assert_eq!(deserialized.name, "cleanup-hook");
    }

    #[test]
    fn test_event_subscription_clone() {
        let sub = EventSubscription {
            event: "before_completion".to_string(),
            priority: 0,
            name: "observer".to_string(),
        };
        let cloned = sub.clone();
        assert_eq!(cloned.event, sub.event);
        assert_eq!(cloned.priority, sub.priority);
        assert_eq!(cloned.name, sub.name);
    }

    // --- Re-export test ---

    #[test]
    fn test_value_reexport() {
        // Verify serde_json::Value is re-exported from the crate root
        let _v: serde_json::Value = json!(42);
    }
}
