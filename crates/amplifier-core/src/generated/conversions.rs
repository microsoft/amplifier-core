//! Bidirectional `From` conversions between hand-written Rust types and
//! proto-generated types, enabling zero-copy-style mapping across the gRPC
//! boundary.

// ---------------------------------------------------------------------------
// ToolResult conversions
// ---------------------------------------------------------------------------

impl From<crate::models::ToolResult> for super::amplifier_module::ToolResult {
    fn from(native: crate::models::ToolResult) -> Self {
        Self {
            success: native.success,
            output_json: native
                .output
                .map(|v| {
                    serde_json::to_string(&v).unwrap_or_else(|e| {
                        log::warn!("Failed to serialize ToolResult output to JSON: {e}");
                        String::new()
                    })
                })
                .unwrap_or_default(),
            error_json: native
                .error
                .map(|e| {
                    serde_json::to_string(&e).unwrap_or_else(|ser_err| {
                        log::warn!("Failed to serialize ToolResult error to JSON: {ser_err}");
                        String::new()
                    })
                })
                .unwrap_or_default(),
        }
    }
}

impl From<super::amplifier_module::ToolResult> for crate::models::ToolResult {
    fn from(proto: super::amplifier_module::ToolResult) -> Self {
        Self {
            success: proto.success,
            output: if proto.output_json.is_empty() {
                None
            } else {
                serde_json::from_str(&proto.output_json)
                    .map_err(|e| {
                        log::warn!("Failed to deserialize ToolResult output_json: {e}");
                        e
                    })
                    .ok()
            },
            error: if proto.error_json.is_empty() {
                None
            } else {
                serde_json::from_str(&proto.error_json)
                    .map_err(|e| {
                        log::warn!("Failed to deserialize ToolResult error_json: {e}");
                        e
                    })
                    .ok()
            },
        }
    }
}

// ---------------------------------------------------------------------------
// ModelInfo conversions
// ---------------------------------------------------------------------------

impl From<crate::models::ModelInfo> for super::amplifier_module::ModelInfo {
    fn from(native: crate::models::ModelInfo) -> Self {
        Self {
            id: native.id,
            display_name: native.display_name,
            context_window: i32::try_from(native.context_window).unwrap_or_else(|_| {
                log::warn!(
                    "context_window {} overflows i32, clamping to i32::MAX",
                    native.context_window
                );
                i32::MAX
            }),
            max_output_tokens: i32::try_from(native.max_output_tokens).unwrap_or_else(|_| {
                log::warn!(
                    "max_output_tokens {} overflows i32, clamping to i32::MAX",
                    native.max_output_tokens
                );
                i32::MAX
            }),
            capabilities: native.capabilities,
            defaults_json: serde_json::to_string(&native.defaults).unwrap_or_else(|e| {
                log::warn!("Failed to serialize ModelInfo defaults to JSON: {e}");
                String::new()
            }),
        }
    }
}

impl From<super::amplifier_module::ModelInfo> for crate::models::ModelInfo {
    fn from(proto: super::amplifier_module::ModelInfo) -> Self {
        Self {
            id: proto.id,
            display_name: proto.display_name,
            context_window: i64::from(proto.context_window),
            max_output_tokens: i64::from(proto.max_output_tokens),
            capabilities: proto.capabilities,
            defaults: if proto.defaults_json.is_empty() {
                Default::default()
            } else {
                serde_json::from_str(&proto.defaults_json).unwrap_or_else(|e| {
                    log::warn!("Failed to deserialize ModelInfo defaults_json: {e}");
                    Default::default()
                })
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Usage conversions
// ---------------------------------------------------------------------------

impl From<crate::messages::Usage> for super::amplifier_module::Usage {
    fn from(native: crate::messages::Usage) -> Self {
        Self {
            prompt_tokens: i32::try_from(native.input_tokens).unwrap_or_else(|_| {
                log::warn!(
                    "input_tokens {} overflows i32, clamping to i32::MAX",
                    native.input_tokens
                );
                i32::MAX
            }),
            completion_tokens: i32::try_from(native.output_tokens).unwrap_or_else(|_| {
                log::warn!(
                    "output_tokens {} overflows i32, clamping to i32::MAX",
                    native.output_tokens
                );
                i32::MAX
            }),
            total_tokens: i32::try_from(native.total_tokens).unwrap_or_else(|_| {
                log::warn!(
                    "total_tokens {} overflows i32, clamping to i32::MAX",
                    native.total_tokens
                );
                i32::MAX
            }),
            reasoning_tokens: native.reasoning_tokens.map(|v| {
                i32::try_from(v).unwrap_or_else(|_| {
                    log::warn!("reasoning_tokens {} overflows i32, clamping to i32::MAX", v);
                    i32::MAX
                })
            }),
            cache_read_tokens: native.cache_read_tokens.map(|v| {
                i32::try_from(v).unwrap_or_else(|_| {
                    log::warn!("cache_read_tokens {} overflows i32, clamping to i32::MAX", v);
                    i32::MAX
                })
            }),
            cache_creation_tokens: native.cache_write_tokens.map(|v| {
                i32::try_from(v).unwrap_or_else(|_| {
                    log::warn!("cache_write_tokens {} overflows i32, clamping to i32::MAX", v);
                    i32::MAX
                })
            }),
        }
    }
}

impl From<super::amplifier_module::Usage> for crate::messages::Usage {
    fn from(proto: super::amplifier_module::Usage) -> Self {
        Self {
            input_tokens: i64::from(proto.prompt_tokens),
            output_tokens: i64::from(proto.completion_tokens),
            total_tokens: i64::from(proto.total_tokens),
            reasoning_tokens: proto.reasoning_tokens.map(i64::from),
            cache_read_tokens: proto.cache_read_tokens.map(i64::from),
            cache_write_tokens: proto.cache_creation_tokens.map(i64::from),
            extensions: std::collections::HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Role conversion helpers
// ---------------------------------------------------------------------------

/// Convert a native [`crate::messages::Role`] to its proto `i32` equivalent.
pub fn native_role_to_proto(role: crate::messages::Role) -> i32 {
    use crate::messages::Role;
    use super::amplifier_module::Role as ProtoRole;

    match role {
        Role::System => ProtoRole::System as i32,
        Role::User => ProtoRole::User as i32,
        Role::Assistant => ProtoRole::Assistant as i32,
        Role::Tool => ProtoRole::Tool as i32,
        Role::Function => ProtoRole::Function as i32,
        Role::Developer => ProtoRole::Developer as i32,
    }
}

/// Convert a proto `i32` role value to a native [`crate::messages::Role`].
///
/// `Unspecified` (0) and unknown values default to [`crate::messages::Role::User`]
/// with a warning log.
pub fn proto_role_to_native(proto_role: i32) -> crate::messages::Role {
    use crate::messages::Role;
    use super::amplifier_module::Role as ProtoRole;

    match ProtoRole::try_from(proto_role) {
        Ok(ProtoRole::System) => Role::System,
        Ok(ProtoRole::User) => Role::User,
        Ok(ProtoRole::Assistant) => Role::Assistant,
        Ok(ProtoRole::Tool) => Role::Tool,
        Ok(ProtoRole::Function) => Role::Function,
        Ok(ProtoRole::Developer) => Role::Developer,
        Ok(ProtoRole::Unspecified) => {
            log::warn!("Proto role Unspecified (0), defaulting to User");
            Role::User
        }
        Err(_) => {
            log::warn!("Unknown proto role value {proto_role}, defaulting to User");
            Role::User
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn tool_result_roundtrip() {
        let original = crate::models::ToolResult {
            success: true,
            output: Some(serde_json::json!({"key": "value"})),
            error: None,
        };
        let proto: super::super::amplifier_module::ToolResult = original.clone().into();
        let restored: crate::models::ToolResult = proto.into();
        assert_eq!(original, restored);
    }

    #[test]
    fn tool_result_with_error_roundtrip() {
        let original = crate::models::ToolResult {
            success: false,
            output: None,
            error: Some(HashMap::from([(
                "message".to_string(),
                serde_json::json!("something failed"),
            )])),
        };
        let proto: super::super::amplifier_module::ToolResult = original.clone().into();
        let restored: crate::models::ToolResult = proto.into();
        assert_eq!(original, restored);
    }

    #[test]
    fn model_info_roundtrip() {
        let original = crate::models::ModelInfo {
            id: "gpt-4".into(),
            display_name: "GPT-4".into(),
            context_window: 128000,
            max_output_tokens: 8192,
            capabilities: vec!["tools".into(), "vision".into()],
            defaults: HashMap::from([("temperature".to_string(), serde_json::json!(0.7))]),
        };
        let proto: super::super::amplifier_module::ModelInfo = original.clone().into();
        let restored: crate::models::ModelInfo = proto.into();
        assert_eq!(original, restored);
    }

    #[test]
    fn usage_roundtrip() {
        let original = crate::messages::Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            reasoning_tokens: Some(20),
            cache_read_tokens: Some(10),
            cache_write_tokens: None, // 0 in proto, None when restored
            extensions: HashMap::new(),
        };
        let proto: super::super::amplifier_module::Usage = original.clone().into();
        let restored: crate::messages::Usage = proto.into();
        assert_eq!(original.input_tokens, restored.input_tokens);
        assert_eq!(original.output_tokens, restored.output_tokens);
        assert_eq!(original.total_tokens, restored.total_tokens);
        assert_eq!(original.reasoning_tokens, restored.reasoning_tokens);
        assert_eq!(original.cache_read_tokens, restored.cache_read_tokens);
        // cache_write_tokens: None → None (optional proto preserves None)
        assert_eq!(restored.cache_write_tokens, None);
        // extensions are lost in proto roundtrip (proto has no extensions field)
        assert!(restored.extensions.is_empty());
    }

    #[test]
    fn usage_with_all_optional_tokens() {
        let original = crate::messages::Usage {
            input_tokens: 200,
            output_tokens: 100,
            total_tokens: 300,
            reasoning_tokens: Some(50),
            cache_read_tokens: Some(30),
            cache_write_tokens: Some(20),
            extensions: HashMap::new(),
        };
        let proto: super::super::amplifier_module::Usage = original.clone().into();
        let restored: crate::messages::Usage = proto.into();
        assert_eq!(original.input_tokens, restored.input_tokens);
        assert_eq!(original.output_tokens, restored.output_tokens);
        assert_eq!(original.total_tokens, restored.total_tokens);
        assert_eq!(original.reasoning_tokens, restored.reasoning_tokens);
        assert_eq!(original.cache_read_tokens, restored.cache_read_tokens);
        assert_eq!(original.cache_write_tokens, restored.cache_write_tokens);
    }

    /// Verify that `Some(0)` survives roundtrip now that proto uses `optional` fields.
    #[test]
    fn usage_some_zero_roundtrips_correctly() {
        let original = crate::messages::Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            reasoning_tokens: Some(0),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            extensions: HashMap::new(),
        };
        let proto: super::super::amplifier_module::Usage = original.clone().into();
        let restored: crate::messages::Usage = proto.into();
        assert_eq!(restored.reasoning_tokens, Some(0), "Some(0) reasoning_tokens must survive roundtrip");
        assert_eq!(restored.cache_read_tokens, Some(0), "Some(0) cache_read_tokens must survive roundtrip");
        assert_eq!(restored.cache_write_tokens, Some(0), "Some(0) cache_write_tokens must survive roundtrip");
    }

    // -- E-3: ModelInfo i64→i32 overflow clamps to i32::MAX --

    #[test]
    fn model_info_context_window_overflow_clamps() {
        let original = crate::models::ModelInfo {
            id: "big-model".into(),
            display_name: "Big".into(),
            context_window: i64::from(i32::MAX) + 1,
            max_output_tokens: 100,
            capabilities: vec![],
            defaults: HashMap::new(),
        };
        let proto: super::super::amplifier_module::ModelInfo = original.into();
        assert_eq!(proto.context_window, i32::MAX);
    }

    #[test]
    fn model_info_max_output_tokens_overflow_clamps() {
        let original = crate::models::ModelInfo {
            id: "big-model".into(),
            display_name: "Big".into(),
            context_window: 100,
            max_output_tokens: i64::from(i32::MAX) + 500,
            capabilities: vec![],
            defaults: HashMap::new(),
        };
        let proto: super::super::amplifier_module::ModelInfo = original.into();
        assert_eq!(proto.max_output_tokens, i32::MAX);
    }

    // -- E-4: Usage i64→i32 overflow clamps to i32::MAX --

    #[test]
    fn usage_prompt_tokens_overflow_clamps() {
        let original = crate::messages::Usage {
            input_tokens: i64::from(i32::MAX) + 1,
            output_tokens: 0,
            total_tokens: 0,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            extensions: HashMap::new(),
        };
        let proto: super::super::amplifier_module::Usage = original.into();
        assert_eq!(proto.prompt_tokens, i32::MAX);
    }

    #[test]
    fn usage_completion_tokens_overflow_clamps() {
        let original = crate::messages::Usage {
            input_tokens: 0,
            output_tokens: i64::from(i32::MAX) + 1,
            total_tokens: 0,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            extensions: HashMap::new(),
        };
        let proto: super::super::amplifier_module::Usage = original.into();
        assert_eq!(proto.completion_tokens, i32::MAX);
    }

    #[test]
    fn usage_total_tokens_overflow_clamps() {
        let original = crate::messages::Usage {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: i64::from(i32::MAX) + 1,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            extensions: HashMap::new(),
        };
        let proto: super::super::amplifier_module::Usage = original.into();
        assert_eq!(proto.total_tokens, i32::MAX);
    }

    // -- Role conversion helper tests --

    #[test]
    fn native_role_to_proto_role_all_variants() {
        use crate::messages::Role;
        use super::super::amplifier_module::Role as ProtoRole;

        assert_eq!(super::native_role_to_proto(Role::System), ProtoRole::System as i32);
        assert_eq!(super::native_role_to_proto(Role::User), ProtoRole::User as i32);
        assert_eq!(super::native_role_to_proto(Role::Assistant), ProtoRole::Assistant as i32);
        assert_eq!(super::native_role_to_proto(Role::Tool), ProtoRole::Tool as i32);
        assert_eq!(super::native_role_to_proto(Role::Function), ProtoRole::Function as i32);
        assert_eq!(super::native_role_to_proto(Role::Developer), ProtoRole::Developer as i32);
    }

    #[test]
    fn proto_role_to_native_role_all_variants() {
        use crate::messages::Role;
        use super::super::amplifier_module::Role as ProtoRole;

        assert_eq!(super::proto_role_to_native(ProtoRole::System as i32), Role::System);
        assert_eq!(super::proto_role_to_native(ProtoRole::User as i32), Role::User);
        assert_eq!(super::proto_role_to_native(ProtoRole::Assistant as i32), Role::Assistant);
        assert_eq!(super::proto_role_to_native(ProtoRole::Tool as i32), Role::Tool);
        assert_eq!(super::proto_role_to_native(ProtoRole::Function as i32), Role::Function);
        assert_eq!(super::proto_role_to_native(ProtoRole::Developer as i32), Role::Developer);
    }

    #[test]
    fn proto_role_unspecified_defaults_to_user() {
        use crate::messages::Role;
        use super::super::amplifier_module::Role as ProtoRole;

        assert_eq!(super::proto_role_to_native(ProtoRole::Unspecified as i32), Role::User);
    }

    #[test]
    fn proto_role_unknown_defaults_to_user() {
        use crate::messages::Role;

        // 999 is not a valid proto Role value
        assert_eq!(super::proto_role_to_native(999), Role::User);
    }
}
