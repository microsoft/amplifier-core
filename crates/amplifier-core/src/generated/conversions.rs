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
                .map(|v| serde_json::to_string(&v).unwrap_or_default())
                .unwrap_or_default(),
            error_json: native
                .error
                .map(|e| serde_json::to_string(&e).unwrap_or_default())
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
                serde_json::from_str(&proto.output_json).ok()
            },
            error: if proto.error_json.is_empty() {
                None
            } else {
                serde_json::from_str(&proto.error_json).ok()
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
            context_window: native.context_window as i32,
            max_output_tokens: native.max_output_tokens as i32,
            capabilities: native.capabilities,
            defaults_json: serde_json::to_string(&native.defaults).unwrap_or_default(),
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
            defaults: serde_json::from_str(&proto.defaults_json).unwrap_or_default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Usage conversions
// ---------------------------------------------------------------------------

impl From<crate::messages::Usage> for super::amplifier_module::Usage {
    fn from(native: crate::messages::Usage) -> Self {
        Self {
            prompt_tokens: native.input_tokens as i32,
            completion_tokens: native.output_tokens as i32,
            total_tokens: native.total_tokens as i32,
            reasoning_tokens: native.reasoning_tokens.unwrap_or(0) as i32,
            cache_read_tokens: native.cache_read_tokens.unwrap_or(0) as i32,
            cache_creation_tokens: native.cache_write_tokens.unwrap_or(0) as i32,
        }
    }
}

impl From<super::amplifier_module::Usage> for crate::messages::Usage {
    fn from(proto: super::amplifier_module::Usage) -> Self {
        Self {
            input_tokens: i64::from(proto.prompt_tokens),
            output_tokens: i64::from(proto.completion_tokens),
            total_tokens: i64::from(proto.total_tokens),
            reasoning_tokens: if proto.reasoning_tokens == 0 {
                None
            } else {
                Some(i64::from(proto.reasoning_tokens))
            },
            cache_read_tokens: if proto.cache_read_tokens == 0 {
                None
            } else {
                Some(i64::from(proto.cache_read_tokens))
            },
            cache_write_tokens: if proto.cache_creation_tokens == 0 {
                None
            } else {
                Some(i64::from(proto.cache_creation_tokens))
            },
            extensions: std::collections::HashMap::new(),
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
        // cache_write_tokens: None → 0 → None (roundtrip preserves None)
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
}
