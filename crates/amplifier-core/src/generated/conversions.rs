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
            extensions: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Role conversion helpers
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use crate::messages::Role;
use super::amplifier_module::Role as ProtoRole;
use super::amplifier_module::Visibility as ProtoVisibility;

/// Convert a native [`crate::messages::Role`] to its proto `i32` equivalent.
pub fn native_role_to_proto(role: Role) -> i32 {
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
pub fn proto_role_to_native(proto_role: i32) -> Role {
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

// ---------------------------------------------------------------------------
// Visibility conversion helpers (private)
// ---------------------------------------------------------------------------

fn native_visibility_to_proto(vis: &Option<crate::messages::Visibility>) -> i32 {
    match vis {
        None => ProtoVisibility::Unspecified as i32,
        Some(crate::messages::Visibility::Internal) => ProtoVisibility::LlmOnly as i32,
        Some(crate::messages::Visibility::Developer) => ProtoVisibility::All as i32,
        Some(crate::messages::Visibility::User) => ProtoVisibility::UserOnly as i32,
    }
}

fn proto_visibility_to_native(vis: i32) -> Option<crate::messages::Visibility> {
    match ProtoVisibility::try_from(vis) {
        Ok(ProtoVisibility::LlmOnly) => Some(crate::messages::Visibility::Internal),
        Ok(ProtoVisibility::All) => Some(crate::messages::Visibility::Developer),
        Ok(ProtoVisibility::UserOnly) => Some(crate::messages::Visibility::User),
        _ => None, // Unspecified or unknown
    }
}

// ---------------------------------------------------------------------------
// ContentBlock conversion helpers (private)
// ---------------------------------------------------------------------------

fn native_content_block_to_proto(
    block: crate::messages::ContentBlock,
) -> super::amplifier_module::ContentBlock {
    use crate::messages::ContentBlock;
    use super::amplifier_module::content_block::Block;

    let (proto_block, vis) = match block {
        ContentBlock::Text {
            text,
            visibility,
            ..
        } => (
            Block::TextBlock(super::amplifier_module::TextBlock { text }),
            visibility,
        ),
        ContentBlock::Thinking {
            thinking,
            signature,
            visibility,
            content,
            ..
        } => (
            Block::ThinkingBlock(super::amplifier_module::ThinkingBlock {
                thinking,
                signature: signature.unwrap_or_default(),
                content: content
                    .map(|v| {
                        serde_json::to_string(&v).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize Thinking content to JSON: {e}");
                            String::new()
                        })
                    })
                    .unwrap_or_default(),
            }),
            visibility,
        ),
        ContentBlock::RedactedThinking {
            data,
            visibility,
            ..
        } => (
            Block::RedactedThinkingBlock(super::amplifier_module::RedactedThinkingBlock { data }),
            visibility,
        ),
        ContentBlock::ToolCall {
            id,
            name,
            input,
            visibility,
            ..
        } => (
            Block::ToolCallBlock(super::amplifier_module::ToolCallBlock {
                id,
                name,
                input_json: serde_json::to_string(&input).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolCall input to JSON: {e}");
                    String::new()
                }),
            }),
            visibility,
        ),
        ContentBlock::ToolResult {
            tool_call_id,
            output,
            visibility,
            ..
        } => (
            Block::ToolResultBlock(super::amplifier_module::ToolResultBlock {
                tool_call_id,
                output_json: serde_json::to_string(&output).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolResult output to JSON: {e}");
                    String::new()
                }),
            }),
            visibility,
        ),
        ContentBlock::Image {
            source,
            visibility,
            ..
        } => (
            Block::ImageBlock(super::amplifier_module::ImageBlock {
                media_type: source
                    .get("media_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                data: source
                    .get("data")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .as_bytes()
                    .to_vec(),
                source_json: serde_json::to_string(&source).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize Image source to JSON: {e}");
                    String::new()
                }),
            }),
            visibility,
        ),
        ContentBlock::Reasoning {
            content,
            summary,
            visibility,
            ..
        } => (
            Block::ReasoningBlock(super::amplifier_module::ReasoningBlock {
                content: content
                    .into_iter()
                    .map(|v| {
                        serde_json::to_string(&v).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize Reasoning content item to JSON: {e}");
                            String::new()
                        })
                    })
                    .collect(),
                summary: summary
                    .into_iter()
                    .map(|v| {
                        serde_json::to_string(&v).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize Reasoning summary item to JSON: {e}");
                            String::new()
                        })
                    })
                    .collect(),
            }),
            visibility,
        ),
    };

    super::amplifier_module::ContentBlock {
        block: Some(proto_block),
        visibility: native_visibility_to_proto(&vis),
    }
}

fn proto_content_block_to_native(
    block: super::amplifier_module::ContentBlock,
) -> crate::messages::ContentBlock {
    use crate::messages::ContentBlock;
    use super::amplifier_module::content_block::Block;

    let vis = proto_visibility_to_native(block.visibility);

    match block.block {
        Some(Block::TextBlock(tb)) => ContentBlock::Text {
            text: tb.text,
            visibility: vis,
            extensions: HashMap::new(),
        },
        Some(Block::ThinkingBlock(tb)) => ContentBlock::Thinking {
            thinking: tb.thinking,
            signature: if tb.signature.is_empty() {
                None
            } else {
                Some(tb.signature)
            },
            visibility: vis,
            content: if tb.content.is_empty() {
                None
            } else {
                serde_json::from_str(&tb.content)
                    .map_err(|e| {
                        log::warn!("Failed to deserialize ThinkingBlock content: {e}");
                        e
                    })
                    .ok()
            },
            extensions: HashMap::new(),
        },
        Some(Block::RedactedThinkingBlock(rb)) => ContentBlock::RedactedThinking {
            data: rb.data,
            visibility: vis,
            extensions: HashMap::new(),
        },
        Some(Block::ToolCallBlock(tc)) => ContentBlock::ToolCall {
            id: tc.id,
            name: tc.name,
            input: serde_json::from_str(&tc.input_json).unwrap_or_else(|e| {
                log::warn!("Failed to deserialize ToolCallBlock input_json: {e}");
                Default::default()
            }),
            visibility: vis,
            extensions: HashMap::new(),
        },
        Some(Block::ToolResultBlock(tr)) => ContentBlock::ToolResult {
            tool_call_id: tr.tool_call_id,
            output: serde_json::from_str(&tr.output_json).unwrap_or_else(|e| {
                log::warn!("Failed to deserialize ToolResultBlock output_json: {e}");
                serde_json::Value::Null
            }),
            visibility: vis,
            extensions: HashMap::new(),
        },
        Some(Block::ImageBlock(ib)) => ContentBlock::Image {
            source: if ib.source_json.is_empty() {
                HashMap::new()
            } else {
                serde_json::from_str(&ib.source_json).unwrap_or_else(|e| {
                    log::warn!("Failed to deserialize ImageBlock source_json: {e}");
                    Default::default()
                })
            },
            visibility: vis,
            extensions: HashMap::new(),
        },
        Some(Block::ReasoningBlock(rb)) => ContentBlock::Reasoning {
            content: rb
                .content
                .into_iter()
                .filter_map(|s| {
                    serde_json::from_str(&s)
                        .map_err(|e| {
                            log::warn!("Failed to deserialize ReasoningBlock content item: {e}");
                            e
                        })
                        .ok()
                })
                .collect(),
            summary: rb
                .summary
                .into_iter()
                .filter_map(|s| {
                    serde_json::from_str(&s)
                        .map_err(|e| {
                            log::warn!("Failed to deserialize ReasoningBlock summary item: {e}");
                            e
                        })
                        .ok()
                })
                .collect(),
            visibility: vis,
            extensions: HashMap::new(),
        },
        None => {
            log::warn!("Proto ContentBlock has no block variant set, falling back to empty Text");
            ContentBlock::Text {
                text: String::new(),
                visibility: vis,
                extensions: HashMap::new(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Message conversion functions (public)
// ---------------------------------------------------------------------------

/// Convert a native [`crate::messages::Message`] to its proto equivalent.
pub fn native_message_to_proto(
    msg: crate::messages::Message,
) -> super::amplifier_module::Message {
    use super::amplifier_module::message;

    let content = match msg.content {
        crate::messages::MessageContent::Text(s) => {
            Some(message::Content::TextContent(s))
        }
        crate::messages::MessageContent::Blocks(blocks) => {
            let proto_blocks: Vec<_> = blocks
                .into_iter()
                .map(native_content_block_to_proto)
                .collect();
            Some(message::Content::BlockContent(
                super::amplifier_module::ContentBlockList {
                    blocks: proto_blocks,
                },
            ))
        }
    };

    super::amplifier_module::Message {
        role: native_role_to_proto(msg.role),
        content,
        name: msg.name.unwrap_or_default(),
        tool_call_id: msg.tool_call_id.unwrap_or_default(),
        metadata_json: msg
            .metadata
            .map(|m| {
                serde_json::to_string(&m).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize Message metadata to JSON: {e}");
                    String::new()
                })
            })
            .unwrap_or_default(),
    }
}

/// Convert a proto [`super::amplifier_module::Message`] to a native
/// [`crate::messages::Message`].
///
/// Returns `Err` if the proto message has no content (the `oneof content`
/// field is `None`).
pub fn proto_message_to_native(
    proto: super::amplifier_module::Message,
) -> Result<crate::messages::Message, String> {
    let content = match proto.content {
        None => return Err("Message has no content".to_string()),
        Some(super::amplifier_module::message::Content::TextContent(s)) => {
            crate::messages::MessageContent::Text(s)
        }
        Some(super::amplifier_module::message::Content::BlockContent(bl)) => {
            crate::messages::MessageContent::Blocks(
                bl.blocks
                    .into_iter()
                    .map(proto_content_block_to_native)
                    .collect(),
            )
        }
    };

    Ok(crate::messages::Message {
        role: proto_role_to_native(proto.role),
        content,
        name: if proto.name.is_empty() {
            None
        } else {
            Some(proto.name)
        },
        tool_call_id: if proto.tool_call_id.is_empty() {
            None
        } else {
            Some(proto.tool_call_id)
        },
        metadata: if proto.metadata_json.is_empty() {
            None
        } else {
            serde_json::from_str(&proto.metadata_json)
                .map_err(|e| {
                    log::warn!("Failed to deserialize Message metadata_json: {e}");
                    e
                })
                .ok()
        },
        extensions: HashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::messages::Role;
    use super::super::amplifier_module::Role as ProtoRole;

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
        assert_eq!(super::native_role_to_proto(Role::System), ProtoRole::System as i32);
        assert_eq!(super::native_role_to_proto(Role::User), ProtoRole::User as i32);
        assert_eq!(super::native_role_to_proto(Role::Assistant), ProtoRole::Assistant as i32);
        assert_eq!(super::native_role_to_proto(Role::Tool), ProtoRole::Tool as i32);
        assert_eq!(super::native_role_to_proto(Role::Function), ProtoRole::Function as i32);
        assert_eq!(super::native_role_to_proto(Role::Developer), ProtoRole::Developer as i32);
    }

    #[test]
    fn proto_role_to_native_role_all_variants() {
        assert_eq!(super::proto_role_to_native(ProtoRole::System as i32), Role::System);
        assert_eq!(super::proto_role_to_native(ProtoRole::User as i32), Role::User);
        assert_eq!(super::proto_role_to_native(ProtoRole::Assistant as i32), Role::Assistant);
        assert_eq!(super::proto_role_to_native(ProtoRole::Tool as i32), Role::Tool);
        assert_eq!(super::proto_role_to_native(ProtoRole::Function as i32), Role::Function);
        assert_eq!(super::proto_role_to_native(ProtoRole::Developer as i32), Role::Developer);
    }

    #[test]
    fn proto_role_unspecified_defaults_to_user() {
        assert_eq!(super::proto_role_to_native(ProtoRole::Unspecified as i32), Role::User);
    }

    #[test]
    fn proto_role_unknown_defaults_to_user() {
        // 999 and -1 are not valid proto Role values
        assert_eq!(super::proto_role_to_native(999), Role::User);
        assert_eq!(super::proto_role_to_native(-1), Role::User);
    }

    // -- Message conversion tests --

    #[test]
    fn message_text_content_roundtrip() {
        use crate::messages::{Message, MessageContent};

        let original = Message {
            role: Role::User,
            content: MessageContent::Text("Hello, world!".into()),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.role, original.role);
        assert_eq!(restored.content, original.content);
        assert_eq!(restored.name, None);
        assert_eq!(restored.tool_call_id, None);
    }

    #[test]
    fn message_block_content_text_roundtrip() {
        use crate::messages::{ContentBlock, Message, MessageContent};

        let original = Message {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::Text {
                text: "thinking...".into(),
                visibility: None,
                extensions: HashMap::new(),
            }]),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.role, original.role);
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn message_with_tool_call_id_roundtrip() {
        use crate::messages::{Message, MessageContent};

        let original = Message {
            role: Role::Tool,
            content: MessageContent::Text("result data".into()),
            name: Some("read_file".into()),
            tool_call_id: Some("call_123".into()),
            metadata: Some(HashMap::from([
                ("source".to_string(), serde_json::json!("test")),
            ])),
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.role, original.role);
        assert_eq!(restored.content, original.content);
        assert_eq!(restored.name, Some("read_file".into()));
        assert_eq!(restored.tool_call_id, Some("call_123".into()));
        assert_eq!(restored.metadata, original.metadata);
    }

    // -- Individual ContentBlock variant roundtrip tests --

    #[test]
    fn content_block_thinking_roundtrip() {
        use crate::messages::{ContentBlock, Message, MessageContent, Visibility};

        let original = Message {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::Thinking {
                thinking: "Let me reason about this...".into(),
                signature: Some("sig_abc123".into()),
                visibility: Some(Visibility::Internal),
                content: Some(vec![serde_json::json!({"type": "text", "text": "inner"})]),
                extensions: HashMap::new(),
            }]),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn content_block_redacted_thinking_roundtrip() {
        use crate::messages::{ContentBlock, Message, MessageContent};

        let original = Message {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::RedactedThinking {
                data: "redacted_data_blob".into(),
                visibility: None,
                extensions: HashMap::new(),
            }]),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn content_block_tool_call_roundtrip() {
        use crate::messages::{ContentBlock, Message, MessageContent, Visibility};

        let original = Message {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolCall {
                id: "call_456".into(),
                name: "read_file".into(),
                input: HashMap::from([
                    ("path".to_string(), serde_json::json!("/tmp/test.txt")),
                ]),
                visibility: Some(Visibility::Developer),
                extensions: HashMap::new(),
            }]),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn content_block_tool_result_roundtrip() {
        use crate::messages::{ContentBlock, Message, MessageContent};

        let original = Message {
            role: Role::Tool,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_call_id: "call_456".into(),
                output: serde_json::json!({"status": "ok", "lines": 42}),
                visibility: None,
                extensions: HashMap::new(),
            }]),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn content_block_image_roundtrip() {
        use crate::messages::{ContentBlock, Message, MessageContent, Visibility};

        let source = HashMap::from([
            ("media_type".to_string(), serde_json::json!("image/png")),
            ("data".to_string(), serde_json::json!("iVBORw0KGgo=")),
        ]);
        let original = Message {
            role: Role::User,
            content: MessageContent::Blocks(vec![ContentBlock::Image {
                source,
                visibility: Some(Visibility::User),
                extensions: HashMap::new(),
            }]),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn content_block_reasoning_roundtrip() {
        use crate::messages::{ContentBlock, Message, MessageContent};

        let original = Message {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::Reasoning {
                content: vec![
                    serde_json::json!({"type": "text", "text": "Step 1"}),
                    serde_json::json!({"type": "text", "text": "Step 2"}),
                ],
                summary: vec![serde_json::json!({"type": "text", "text": "Summary"})],
                visibility: None,
                extensions: HashMap::new(),
            }]),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = super::native_message_to_proto(original.clone());
        let restored = super::proto_message_to_native(proto).expect("should succeed");
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn message_none_content_returns_error() {
        use super::super::amplifier_module;

        let proto = amplifier_module::Message {
            role: amplifier_module::Role::User as i32,
            content: None,
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
        };
        let result = super::proto_message_to_native(proto);
        assert!(result.is_err(), "None content should return Err");
    }
}
