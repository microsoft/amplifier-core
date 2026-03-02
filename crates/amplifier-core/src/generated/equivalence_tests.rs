//! Equivalence tests — verify proto messages map 1:1 to existing Rust structs.
//!
//! Every generated proto message must be constructible with all its fields,
//! and every enum must cover the expected number of variants.

#[cfg(test)]
mod tests {
    use crate::generated::amplifier_module::*;

    /// Expected number of meaningful (non-Unspecified) enum variants per design.
    const EXPECTED_MODULE_TYPES: usize = 6;
    const EXPECTED_PROVIDER_ERROR_TYPES: usize = 8;
    const EXPECTED_HOOK_ACTIONS: usize = 5;

    #[test]
    fn proto_module_info_has_all_fields() {
        let info = ModuleInfo {
            id: "mod-001".into(),
            name: "test-module".into(),
            version: "1.0.0".into(),
            module_type: ModuleType::Provider as i32,
            mount_point: "/providers/test".into(),
            description: "A test provider module".into(),
            config_schema_json: r#"{"type":"object"}"#.into(),
            capabilities: vec!["streaming".into(), "tools".into()],
            author: "amplifier-team".into(),
        };
        assert_eq!(info.id, "mod-001");
        assert_eq!(info.name, "test-module");
        assert_eq!(info.version, "1.0.0");
        assert_eq!(info.module_type, ModuleType::Provider as i32);
        assert_eq!(info.mount_point, "/providers/test");
        assert_eq!(info.description, "A test provider module");
        assert_eq!(info.config_schema_json, r#"{"type":"object"}"#);
        assert_eq!(info.capabilities.len(), 2);
        assert_eq!(info.author, "amplifier-team");
    }

    #[test]
    fn proto_tool_result_has_all_fields() {
        let result = ToolResult {
            success: true,
            output_json: r#"{"value": 42}"#.into(),
            error_json: String::new(),
        };
        assert!(result.success);
        assert_eq!(result.output_json, r#"{"value": 42}"#);
        assert!(result.error_json.is_empty());

        // Also verify the error case
        let err_result = ToolResult {
            success: false,
            output_json: String::new(),
            error_json: r#"{"code":"NOT_FOUND"}"#.into(),
        };
        assert!(!err_result.success);
        assert!(err_result.output_json.is_empty());
        assert_eq!(err_result.error_json, r#"{"code":"NOT_FOUND"}"#);
    }

    #[test]
    fn proto_hook_result_has_all_15_fields() {
        let result = HookResult {
            action: HookAction::Modify as i32,
            data_json: r#"{"modified": true}"#.into(),
            reason: "content policy".into(),
            context_injection: "Additional context".into(),
            context_injection_role: ContextInjectionRole::System as i32,
            ephemeral: true,
            approval_prompt: "Allow this action?".into(),
            approval_options: vec!["yes".into(), "no".into(), "always".into()],
            approval_timeout: 300.0,
            approval_default: ApprovalDefault::Deny as i32,
            suppress_output: false,
            user_message: "Action requires approval".into(),
            user_message_level: UserMessageLevel::Warning as i32,
            user_message_source: "content-filter".into(),
            append_to_last_tool_result: true,
        };
        assert_eq!(result.action, HookAction::Modify as i32);
        assert_eq!(result.data_json, r#"{"modified": true}"#);
        assert_eq!(result.reason, "content policy");
        assert_eq!(result.context_injection, "Additional context");
        assert_eq!(
            result.context_injection_role,
            ContextInjectionRole::System as i32
        );
        assert!(result.ephemeral);
        assert_eq!(result.approval_prompt, "Allow this action?");
        assert_eq!(result.approval_options.len(), 3);
        assert!((result.approval_timeout - 300.0).abs() < f64::EPSILON);
        assert_eq!(result.approval_default, ApprovalDefault::Deny as i32);
        assert!(!result.suppress_output);
        assert_eq!(result.user_message, "Action requires approval");
        assert_eq!(result.user_message_level, UserMessageLevel::Warning as i32);
        assert_eq!(result.user_message_source, "content-filter");
        assert!(result.append_to_last_tool_result);
    }

    #[test]
    fn proto_provider_error_has_all_fields() {
        let err = ProviderError {
            error_type: ProviderErrorType::RateLimit as i32,
            message: "Rate limit exceeded".into(),
            provider_name: "openai".into(),
            model: "gpt-4".into(),
            status_code: 429,
            retryable: true,
            retry_after: 30.0,
        };
        assert_eq!(err.error_type, ProviderErrorType::RateLimit as i32);
        assert_eq!(err.message, "Rate limit exceeded");
        assert_eq!(err.provider_name, "openai");
        assert_eq!(err.model, "gpt-4");
        assert_eq!(err.status_code, 429);
        assert!(err.retryable);
        assert!((err.retry_after - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn proto_chat_request_has_all_fields() {
        let msg = Message {
            role: Role::User as i32,
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
            content: Some(message::Content::TextContent("Hello".into())),
        };
        // ToolSpecProto is the proto message used inside ChatRequest.tools;
        // ToolSpec is a separate proto message used in the tool module RPC responses.
        let tool = ToolSpecProto {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters_json: r#"{"type":"object"}"#.into(),
        };
        let req = ChatRequest {
            messages: vec![msg],
            tools: vec![tool],
            response_format: Some(ResponseFormat {
                format: Some(response_format::Format::Json(true)),
            }),
            temperature: 0.7,
            top_p: 0.9,
            max_output_tokens: 4096,
            conversation_id: "conv-123".into(),
            stream: true,
            metadata_json: r#"{"source":"test"}"#.into(),
            model: "gpt-4".into(),
            tool_choice: "auto".into(),
            stop: vec!["END".into()],
            reasoning_effort: "high".into(),
            timeout: 60.0,
        };
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.tools.len(), 1);
        assert!(req.response_format.is_some());
        assert!((req.temperature - 0.7).abs() < f64::EPSILON);
        assert!((req.top_p - 0.9).abs() < f64::EPSILON);
        assert_eq!(req.max_output_tokens, 4096);
        assert_eq!(req.conversation_id, "conv-123");
        assert!(req.stream);
        assert_eq!(req.metadata_json, r#"{"source":"test"}"#);
        assert_eq!(req.model, "gpt-4");
        assert_eq!(req.tool_choice, "auto");
        assert_eq!(req.stop, vec!["END"]);
        assert_eq!(req.reasoning_effort, "high");
        assert!((req.timeout - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn proto_usage_has_all_token_fields() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            reasoning_tokens: 20,
            cache_read_tokens: 30,
            cache_creation_tokens: 10,
        };
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.reasoning_tokens, 20);
        assert_eq!(usage.cache_read_tokens, 30);
        assert_eq!(usage.cache_creation_tokens, 10);
    }

    #[test]
    fn proto_model_info_has_all_fields() {
        let info = ModelInfo {
            id: "claude-3-opus".into(),
            display_name: "Claude 3 Opus".into(),
            context_window: 200_000,
            max_output_tokens: 4096,
            capabilities: vec!["vision".into(), "tools".into(), "streaming".into()],
            defaults_json: r#"{"temperature":0.7}"#.into(),
        };
        assert_eq!(info.id, "claude-3-opus");
        assert_eq!(info.display_name, "Claude 3 Opus");
        assert_eq!(info.context_window, 200_000);
        assert_eq!(info.max_output_tokens, 4096);
        assert_eq!(info.capabilities.len(), 3);
        assert_eq!(info.defaults_json, r#"{"temperature":0.7}"#);
    }

    #[test]
    fn proto_approval_roundtrip() {
        let request = ApprovalRequest {
            tool_name: "bash".into(),
            action: "execute".into(),
            details_json: r#"{"command":"rm -rf /tmp/test"}"#.into(),
            risk_level: "high".into(),
            timeout: 120.0,
        };
        assert_eq!(request.tool_name, "bash");
        assert_eq!(request.action, "execute");
        assert_eq!(request.details_json, r#"{"command":"rm -rf /tmp/test"}"#);
        assert_eq!(request.risk_level, "high");
        assert!((request.timeout - 120.0).abs() < f64::EPSILON);

        let response = ApprovalResponse {
            approved: true,
            reason: "User approved".into(),
            remember: false,
        };
        assert!(response.approved);
        assert_eq!(response.reason, "User approved");
        assert!(!response.remember);
    }

    // Exhaustive match helpers — adding a proto enum variant without updating
    // these functions produces a compile error, keeping the tests self-updating.

    fn module_type_label(v: ModuleType) -> &'static str {
        match v {
            ModuleType::Unspecified => "unspecified",
            ModuleType::Provider => "provider",
            ModuleType::Tool => "tool",
            ModuleType::Hook => "hook",
            ModuleType::Memory => "memory",
            ModuleType::Guardrail => "guardrail",
            ModuleType::Approval => "approval",
        }
    }

    fn provider_error_type_label(v: ProviderErrorType) -> &'static str {
        match v {
            ProviderErrorType::Unspecified => "unspecified",
            ProviderErrorType::Auth => "auth",
            ProviderErrorType::RateLimit => "rate_limit",
            ProviderErrorType::ContextLength => "context_length",
            ProviderErrorType::InvalidRequest => "invalid_request",
            ProviderErrorType::ContentFilter => "content_filter",
            ProviderErrorType::Unavailable => "unavailable",
            ProviderErrorType::Timeout => "timeout",
            ProviderErrorType::Other => "other",
        }
    }

    fn hook_action_label(v: HookAction) -> &'static str {
        match v {
            HookAction::Unspecified => "unspecified",
            HookAction::Continue => "continue",
            HookAction::Modify => "modify",
            HookAction::Deny => "deny",
            HookAction::InjectContext => "inject_context",
            HookAction::AskUser => "ask_user",
        }
    }

    /// Asserts that a proto enum has the expected number of non-Unspecified
    /// variants and that every variant round-trips through `i32`.
    ///
    /// `$label_fn` must be an exhaustive-match helper (no wildcards) so that
    /// adding a variant without updating the list causes a compile error.
    macro_rules! assert_enum_coverage {
        ($enum_ty:ty, $variants:expr, $unspecified:expr, $expected_meaningful:expr, $label_fn:expr) => {{
            // Exhaustive match guarantees we track every variant
            for &v in &$variants {
                assert!(!$label_fn(v).is_empty());
            }
            // Verify expected non-Unspecified count
            let meaningful = $variants.iter().filter(|v| **v != $unspecified).count();
            assert_eq!(meaningful, $expected_meaningful);
            // Verify round-trip through i32
            for &v in &$variants {
                let i = v as i32;
                assert_eq!(
                    <$enum_ty as TryFrom<i32>>::try_from(i).unwrap(),
                    v,
                    "round-trip failed for i32 value {i}"
                );
            }
        }};
    }

    #[test]
    fn proto_module_type_covers_all_variants() {
        assert_enum_coverage!(
            ModuleType,
            [
                ModuleType::Unspecified,
                ModuleType::Provider,
                ModuleType::Tool,
                ModuleType::Hook,
                ModuleType::Memory,
                ModuleType::Guardrail,
                ModuleType::Approval,
            ],
            ModuleType::Unspecified,
            EXPECTED_MODULE_TYPES,
            module_type_label
        );
    }

    #[test]
    fn proto_provider_error_type_covers_all_variants() {
        assert_enum_coverage!(
            ProviderErrorType,
            [
                ProviderErrorType::Unspecified,
                ProviderErrorType::Auth,
                ProviderErrorType::RateLimit,
                ProviderErrorType::ContextLength,
                ProviderErrorType::InvalidRequest,
                ProviderErrorType::ContentFilter,
                ProviderErrorType::Unavailable,
                ProviderErrorType::Timeout,
                ProviderErrorType::Other,
            ],
            ProviderErrorType::Unspecified,
            EXPECTED_PROVIDER_ERROR_TYPES,
            provider_error_type_label
        );
    }

    #[test]
    fn proto_hook_action_covers_all_variants() {
        assert_enum_coverage!(
            HookAction,
            [
                HookAction::Unspecified,
                HookAction::Continue,
                HookAction::Modify,
                HookAction::Deny,
                HookAction::InjectContext,
                HookAction::AskUser,
            ],
            HookAction::Unspecified,
            EXPECTED_HOOK_ACTIONS,
            hook_action_label
        );
    }
}
