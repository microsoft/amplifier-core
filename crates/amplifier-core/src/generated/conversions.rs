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
                    log::warn!(
                        "cache_read_tokens {} overflows i32, clamping to i32::MAX",
                        v
                    );
                    i32::MAX
                })
            }),
            cache_creation_tokens: native.cache_write_tokens.map(|v| {
                i32::try_from(v).unwrap_or_else(|_| {
                    log::warn!(
                        "cache_write_tokens {} overflows i32, clamping to i32::MAX",
                        v
                    );
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

use super::amplifier_module::Role as ProtoRole;
use super::amplifier_module::Visibility as ProtoVisibility;
use crate::messages::Role;

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
    use super::amplifier_module::content_block::Block;
    use crate::messages::ContentBlock;

    let (proto_block, vis) = match block {
        ContentBlock::Text {
            text, visibility, ..
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
            data, visibility, ..
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
            source, visibility, ..
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
    use super::amplifier_module::content_block::Block;
    use crate::messages::ContentBlock;

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
pub fn native_message_to_proto(msg: crate::messages::Message) -> super::amplifier_module::Message {
    use super::amplifier_module::message;

    let content = match msg.content {
        crate::messages::MessageContent::Text(s) => Some(message::Content::TextContent(s)),
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

// ---------------------------------------------------------------------------
// HookResult conversion functions (public)
// ---------------------------------------------------------------------------

/// Convert a native [`crate::models::HookResult`] to its proto equivalent.
///
/// # Field mapping notes
///
/// - `action`: native enum variant → proto `HookAction` i32
/// - `context_injection_role`: native enum → proto `ContextInjectionRole` i32
/// - `approval_default`: native `Allow` → proto `Approve`, native `Deny` → proto `Deny`
/// - `user_message_level`: native enum → proto `UserMessageLevel` i32
/// - `approval_timeout`: native `f64` → proto `Option<f64>` (always `Some`)
/// - `approval_options`: native `Option<Vec<String>>` → proto `Vec<String>` (None → empty)
/// - All `Option<String>` fields → proto `String` (None → empty string)
/// - `data`: `Option<HashMap<String, Value>>` serialized to JSON or empty string
/// - `extensions`: dropped (proto has no extensions field)
pub fn native_hook_result_to_proto(
    result: &crate::models::HookResult,
) -> super::amplifier_module::HookResult {
    use super::amplifier_module;
    use crate::models::{ApprovalDefault, ContextInjectionRole, HookAction, UserMessageLevel};

    let action = match result.action {
        HookAction::Continue => amplifier_module::HookAction::Continue as i32,
        HookAction::Modify => amplifier_module::HookAction::Modify as i32,
        HookAction::Deny => amplifier_module::HookAction::Deny as i32,
        HookAction::InjectContext => amplifier_module::HookAction::InjectContext as i32,
        HookAction::AskUser => amplifier_module::HookAction::AskUser as i32,
    };

    let context_injection_role = match result.context_injection_role {
        ContextInjectionRole::System => amplifier_module::ContextInjectionRole::System as i32,
        ContextInjectionRole::User => amplifier_module::ContextInjectionRole::User as i32,
        ContextInjectionRole::Assistant => amplifier_module::ContextInjectionRole::Assistant as i32,
    };

    let approval_default = match result.approval_default {
        ApprovalDefault::Allow => amplifier_module::ApprovalDefault::Approve as i32,
        ApprovalDefault::Deny => amplifier_module::ApprovalDefault::Deny as i32,
    };

    let user_message_level = match result.user_message_level {
        UserMessageLevel::Info => amplifier_module::UserMessageLevel::Info as i32,
        UserMessageLevel::Warning => amplifier_module::UserMessageLevel::Warning as i32,
        UserMessageLevel::Error => amplifier_module::UserMessageLevel::Error as i32,
    };

    let data_json = result
        .data
        .as_ref()
        .map(|d| {
            serde_json::to_string(d).unwrap_or_else(|e| {
                log::warn!("Failed to serialize HookResult data to JSON: {e}");
                String::new()
            })
        })
        .unwrap_or_default();

    amplifier_module::HookResult {
        action,
        data_json,
        reason: result.reason.clone().unwrap_or_default(),
        context_injection: result.context_injection.clone().unwrap_or_default(),
        context_injection_role,
        ephemeral: result.ephemeral,
        approval_prompt: result.approval_prompt.clone().unwrap_or_default(),
        approval_options: result.approval_options.clone().unwrap_or_default(),
        approval_timeout: Some(result.approval_timeout),
        approval_default,
        suppress_output: result.suppress_output,
        user_message: result.user_message.clone().unwrap_or_default(),
        user_message_level,
        user_message_source: result.user_message_source.clone().unwrap_or_default(),
        append_to_last_tool_result: result.append_to_last_tool_result,
    }
}

// ---------------------------------------------------------------------------
// ChatRequest conversion functions (public)
// ---------------------------------------------------------------------------

/// Convert a native [`crate::messages::ChatRequest`] to its proto equivalent.
///
/// # Sentinel value conventions
///
/// Since proto scalar fields (`temperature`, `top_p`, `max_output_tokens`,
/// `stream`, `timeout`, etc.) lack `optional`, the following conventions apply
/// for the reverse direction (`proto_chat_request_to_native`):
///
/// - `temperature`, `top_p`, `timeout` == `0.0` → `None`
/// - `max_output_tokens` == `0` → `None`
/// - Empty strings → `None` for string optionals
/// - `stream == false` → `None`
///
/// Tests should use non-zero / non-empty values to verify full roundtrip
/// fidelity.
pub fn native_chat_request_to_proto(
    request: &crate::messages::ChatRequest,
) -> super::amplifier_module::ChatRequest {
    use super::amplifier_module::{
        response_format, JsonSchemaFormat, ResponseFormat as ProtoResponseFormat, ToolSpecProto,
    };
    use crate::messages::{ResponseFormat, ToolChoice};

    super::amplifier_module::ChatRequest {
        messages: request
            .messages
            .iter()
            .map(|m| native_message_to_proto(m.clone()))
            .collect(),
        tools: request
            .tools
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|t| ToolSpecProto {
                name: t.name.clone(),
                description: t.description.clone().unwrap_or_default(),
                parameters_json: serde_json::to_string(&t.parameters).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolSpec parameters to JSON: {e}");
                    String::new()
                }),
            })
            .collect(),
        response_format: request.response_format.as_ref().map(|rf| match rf {
            ResponseFormat::Text => ProtoResponseFormat {
                format: Some(response_format::Format::Text(true)),
            },
            ResponseFormat::Json => ProtoResponseFormat {
                format: Some(response_format::Format::Json(true)),
            },
            ResponseFormat::JsonSchema { schema, strict } => ProtoResponseFormat {
                format: Some(response_format::Format::JsonSchema(JsonSchemaFormat {
                    schema_json: serde_json::to_string(schema).unwrap_or_else(|e| {
                        log::warn!("Failed to serialize JsonSchema schema to JSON: {e}");
                        String::new()
                    }),
                    strict: strict.unwrap_or(false),
                })),
            },
        }),
        temperature: request.temperature.unwrap_or(0.0),
        top_p: request.top_p.unwrap_or(0.0),
        max_output_tokens: request
            .max_output_tokens
            .map(|v| {
                i32::try_from(v).unwrap_or_else(|_| {
                    log::warn!(
                        "max_output_tokens {} overflows i32, clamping to i32::MAX",
                        v
                    );
                    i32::MAX
                })
            })
            .unwrap_or(0),
        conversation_id: request.conversation_id.clone().unwrap_or_default(),
        stream: request.stream.unwrap_or(false),
        metadata_json: request
            .metadata
            .as_ref()
            .map(|m| {
                serde_json::to_string(m).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ChatRequest metadata to JSON: {e}");
                    String::new()
                })
            })
            .unwrap_or_default(),
        model: request.model.clone().unwrap_or_default(),
        tool_choice: request
            .tool_choice
            .as_ref()
            .map(|tc| match tc {
                ToolChoice::String(s) => s.clone(),
                ToolChoice::Object(obj) => serde_json::to_string(obj).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolChoice object to JSON: {e}");
                    String::new()
                }),
            })
            .unwrap_or_default(),
        stop: request.stop.clone().unwrap_or_default(),
        reasoning_effort: request.reasoning_effort.clone().unwrap_or_default(),
        timeout: request.timeout.unwrap_or(0.0),
    }
}

/// Convert a proto [`super::amplifier_module::ChatRequest`] to a native
/// [`crate::messages::ChatRequest`].
///
/// See [`native_chat_request_to_proto`] for the sentinel value conventions
/// used for scalar fields that have no `optional` proto modifier.
///
/// For `tool_choice`: if the stored string parses as a JSON object it is
/// returned as [`crate::messages::ToolChoice::Object`]; otherwise it is
/// treated as a plain [`crate::messages::ToolChoice::String`].
///
/// Messages that fail to convert are silently skipped with a warning log.
pub fn proto_chat_request_to_native(
    request: super::amplifier_module::ChatRequest,
) -> crate::messages::ChatRequest {
    use super::amplifier_module::response_format;
    use crate::messages::{ResponseFormat, ToolChoice, ToolSpec};

    crate::messages::ChatRequest {
        messages: request
            .messages
            .into_iter()
            .filter_map(|m| {
                proto_message_to_native(m)
                    .map_err(|e| {
                        log::warn!("Skipping invalid message in ChatRequest: {e}");
                        e
                    })
                    .ok()
            })
            .collect(),
        tools: if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .into_iter()
                    .map(|t| ToolSpec {
                        name: t.name,
                        description: if t.description.is_empty() {
                            None
                        } else {
                            Some(t.description)
                        },
                        parameters: if t.parameters_json.is_empty() {
                            HashMap::new()
                        } else {
                            serde_json::from_str(&t.parameters_json).unwrap_or_else(|e| {
                                log::warn!("Failed to deserialize ToolSpec parameters_json: {e}");
                                Default::default()
                            })
                        },
                        extensions: HashMap::new(),
                    })
                    .collect(),
            )
        },
        response_format: request.response_format.and_then(|rf| match rf.format {
            Some(response_format::Format::Text(_)) => Some(ResponseFormat::Text),
            Some(response_format::Format::Json(_)) => Some(ResponseFormat::Json),
            Some(response_format::Format::JsonSchema(js)) => {
                let schema = if js.schema_json.is_empty() {
                    HashMap::new()
                } else {
                    serde_json::from_str(&js.schema_json).unwrap_or_else(|e| {
                        log::warn!("Failed to deserialize JsonSchemaFormat schema_json: {e}");
                        Default::default()
                    })
                };
                Some(ResponseFormat::JsonSchema {
                    schema,
                    // proto `strict` is non-optional bool; false → None, true → Some(true)
                    strict: if js.strict { Some(true) } else { None },
                })
            }
            None => None,
        }),
        // Sentinel: 0.0 means "not set"
        temperature: if request.temperature == 0.0 {
            None
        } else {
            Some(request.temperature)
        },
        top_p: if request.top_p == 0.0 {
            None
        } else {
            Some(request.top_p)
        },
        max_output_tokens: if request.max_output_tokens == 0 {
            None
        } else {
            Some(i64::from(request.max_output_tokens))
        },
        conversation_id: if request.conversation_id.is_empty() {
            None
        } else {
            Some(request.conversation_id)
        },
        // Sentinel: false means "not set"
        stream: if request.stream { Some(true) } else { None },
        metadata: if request.metadata_json.is_empty() {
            None
        } else {
            serde_json::from_str(&request.metadata_json)
                .map_err(|e| {
                    log::warn!("Failed to deserialize ChatRequest metadata_json: {e}");
                    e
                })
                .ok()
        },
        model: if request.model.is_empty() {
            None
        } else {
            Some(request.model)
        },
        tool_choice: if request.tool_choice.is_empty() {
            None
        } else {
            // Try to parse as a JSON object; fall back to a plain string value.
            match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
                &request.tool_choice,
            ) {
                Ok(map) => Some(ToolChoice::Object(map.into_iter().collect())),
                Err(_) => Some(ToolChoice::String(request.tool_choice)),
            }
        },
        stop: if request.stop.is_empty() {
            None
        } else {
            Some(request.stop)
        },
        reasoning_effort: if request.reasoning_effort.is_empty() {
            None
        } else {
            Some(request.reasoning_effort)
        },
        timeout: if request.timeout == 0.0 {
            None
        } else {
            Some(request.timeout)
        },
        extensions: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// ChatResponse conversion functions (public)
// ---------------------------------------------------------------------------

/// Convert a native [`crate::messages::ChatResponse`] to its proto equivalent.
///
/// # Field mapping notes
///
/// - `content`: the full `Vec<ContentBlock>` is serialized as a JSON string into
///   the proto `content` field (empty string when no content).
/// - `tool_calls`: each `ToolCall.arguments` map is serialized to
///   `ToolCallMessage.arguments_json`.
/// - `usage`: delegated to the existing `Usage` `From` impl.
/// - `degradation`: mapped field-for-field (extensions are dropped).
/// - `finish_reason`: empty string sentinel in proto → `None` on restore.
/// - `metadata`: serialized to `metadata_json`.
/// - `extensions`: dropped (proto has no extensions field).
pub fn native_chat_response_to_proto(
    response: &crate::messages::ChatResponse,
) -> super::amplifier_module::ChatResponse {
    super::amplifier_module::ChatResponse {
        content: serde_json::to_string(&response.content).unwrap_or_else(|e| {
            log::warn!("Failed to serialize ChatResponse content to JSON: {e}");
            String::new()
        }),
        tool_calls: response
            .tool_calls
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|tc| super::amplifier_module::ToolCallMessage {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments_json: serde_json::to_string(&tc.arguments).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolCall arguments to JSON: {e}");
                    String::new()
                }),
            })
            .collect(),
        usage: response.usage.clone().map(Into::into),
        degradation: response
            .degradation
            .as_ref()
            .map(|d| super::amplifier_module::Degradation {
                requested: d.requested.clone(),
                actual: d.actual.clone(),
                reason: d.reason.clone(),
            }),
        finish_reason: response.finish_reason.clone().unwrap_or_default(),
        metadata_json: response
            .metadata
            .as_ref()
            .map(|m| {
                serde_json::to_string(m).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ChatResponse metadata to JSON: {e}");
                    String::new()
                })
            })
            .unwrap_or_default(),
    }
}

/// Convert a proto [`super::amplifier_module::ChatResponse`] to a native
/// [`crate::messages::ChatResponse`].
///
/// - `content`: JSON-deserialized back to `Vec<ContentBlock>`; empty string → empty `Vec`.
/// - `tool_calls`: empty repeated field → `None`; non-empty → `Some(Vec<ToolCall>)`.
/// - `finish_reason`: empty string → `None`.
/// - `metadata_json`: empty string → `None`.
/// - `extensions`: always empty (proto has no extensions field).
pub fn proto_chat_response_to_native(
    response: super::amplifier_module::ChatResponse,
) -> crate::messages::ChatResponse {
    crate::messages::ChatResponse {
        content: if response.content.is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&response.content).unwrap_or_else(|e| {
                log::warn!("Failed to deserialize ChatResponse content: {e}");
                Vec::new()
            })
        },
        tool_calls: if response.tool_calls.is_empty() {
            None
        } else {
            Some(
                response
                    .tool_calls
                    .into_iter()
                    .map(|tc| crate::messages::ToolCall {
                        id: tc.id,
                        name: tc.name,
                        arguments: if tc.arguments_json.is_empty() {
                            HashMap::new()
                        } else {
                            serde_json::from_str(&tc.arguments_json).unwrap_or_else(|e| {
                                log::warn!("Failed to deserialize ToolCall arguments_json: {e}");
                                Default::default()
                            })
                        },
                        extensions: HashMap::new(),
                    })
                    .collect(),
            )
        },
        usage: response.usage.map(Into::into),
        degradation: response.degradation.map(|d| crate::messages::Degradation {
            requested: d.requested,
            actual: d.actual,
            reason: d.reason,
            extensions: HashMap::new(),
        }),
        finish_reason: if response.finish_reason.is_empty() {
            None
        } else {
            Some(response.finish_reason)
        },
        metadata: if response.metadata_json.is_empty() {
            None
        } else {
            serde_json::from_str(&response.metadata_json)
                .map_err(|e| {
                    log::warn!("Failed to deserialize ChatResponse metadata_json: {e}");
                    e
                })
                .ok()
        },
        extensions: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::amplifier_module::Role as ProtoRole;
    use crate::messages::Role;

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
        assert_eq!(
            restored.reasoning_tokens,
            Some(0),
            "Some(0) reasoning_tokens must survive roundtrip"
        );
        assert_eq!(
            restored.cache_read_tokens,
            Some(0),
            "Some(0) cache_read_tokens must survive roundtrip"
        );
        assert_eq!(
            restored.cache_write_tokens,
            Some(0),
            "Some(0) cache_write_tokens must survive roundtrip"
        );
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
        assert_eq!(
            super::native_role_to_proto(Role::System),
            ProtoRole::System as i32
        );
        assert_eq!(
            super::native_role_to_proto(Role::User),
            ProtoRole::User as i32
        );
        assert_eq!(
            super::native_role_to_proto(Role::Assistant),
            ProtoRole::Assistant as i32
        );
        assert_eq!(
            super::native_role_to_proto(Role::Tool),
            ProtoRole::Tool as i32
        );
        assert_eq!(
            super::native_role_to_proto(Role::Function),
            ProtoRole::Function as i32
        );
        assert_eq!(
            super::native_role_to_proto(Role::Developer),
            ProtoRole::Developer as i32
        );
    }

    #[test]
    fn proto_role_to_native_role_all_variants() {
        assert_eq!(
            super::proto_role_to_native(ProtoRole::System as i32),
            Role::System
        );
        assert_eq!(
            super::proto_role_to_native(ProtoRole::User as i32),
            Role::User
        );
        assert_eq!(
            super::proto_role_to_native(ProtoRole::Assistant as i32),
            Role::Assistant
        );
        assert_eq!(
            super::proto_role_to_native(ProtoRole::Tool as i32),
            Role::Tool
        );
        assert_eq!(
            super::proto_role_to_native(ProtoRole::Function as i32),
            Role::Function
        );
        assert_eq!(
            super::proto_role_to_native(ProtoRole::Developer as i32),
            Role::Developer
        );
    }

    #[test]
    fn proto_role_unspecified_defaults_to_user() {
        assert_eq!(
            super::proto_role_to_native(ProtoRole::Unspecified as i32),
            Role::User
        );
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
            metadata: Some(HashMap::from([(
                "source".to_string(),
                serde_json::json!("test"),
            )])),
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
                input: HashMap::from([("path".to_string(), serde_json::json!("/tmp/test.txt"))]),
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

    // -- ChatRequest conversion tests --

    #[test]
    fn chat_request_minimal_roundtrip() {
        use crate::messages::{ChatRequest, Message, MessageContent};

        let original = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("Hello!".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
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
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_request_to_proto(&original);
        let restored = super::proto_chat_request_to_native(proto);

        assert_eq!(restored.messages.len(), 1);
        assert_eq!(restored.messages[0].role, original.messages[0].role);
        assert_eq!(restored.messages[0].content, original.messages[0].content);
        assert!(restored.tools.is_none());
        assert!(restored.response_format.is_none());
        assert!(restored.temperature.is_none());
        assert!(restored.model.is_none());
    }

    #[test]
    fn chat_request_full_fields_roundtrip() {
        use crate::messages::{
            ChatRequest, Message, MessageContent, ResponseFormat, ToolChoice, ToolSpec,
        };

        let original = ChatRequest {
            messages: vec![Message {
                role: Role::Assistant,
                content: MessageContent::Text("I can help!".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
            }],
            tools: Some(vec![ToolSpec {
                name: "search".into(),
                description: Some("Search the web".into()),
                parameters: {
                    let mut m = HashMap::new();
                    m.insert("type".into(), serde_json::json!("object"));
                    m.insert(
                        "properties".into(),
                        serde_json::json!({"query": {"type": "string"}}),
                    );
                    m
                },
                extensions: HashMap::new(),
            }]),
            response_format: Some(ResponseFormat::Text),
            temperature: Some(0.7),
            top_p: Some(0.9),
            max_output_tokens: Some(2048),
            conversation_id: Some("conv_abc".into()),
            stream: Some(true),
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("source".into(), serde_json::json!("test-suite"));
                m
            }),
            model: Some("gpt-4o".into()),
            tool_choice: Some(ToolChoice::String("auto".into())),
            stop: Some(vec!["END".into(), "STOP".into()]),
            reasoning_effort: Some("high".into()),
            timeout: Some(30.0),
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_request_to_proto(&original);
        let restored = super::proto_chat_request_to_native(proto);

        assert_eq!(restored.messages.len(), 1);
        assert_eq!(restored.model, Some("gpt-4o".into()));
        assert_eq!(restored.temperature, Some(0.7));
        assert_eq!(restored.top_p, Some(0.9));
        assert_eq!(restored.max_output_tokens, Some(2048));
        assert_eq!(restored.conversation_id, Some("conv_abc".into()));
        assert_eq!(restored.stream, Some(true));
        assert_eq!(restored.reasoning_effort, Some("high".into()));
        assert_eq!(restored.timeout, Some(30.0));
        assert_eq!(restored.stop, Some(vec!["END".into(), "STOP".into()]));
        assert_eq!(
            restored.tool_choice,
            Some(ToolChoice::String("auto".into()))
        );
        assert_eq!(restored.response_format, Some(ResponseFormat::Text));
        assert_eq!(restored.metadata, original.metadata);
    }

    #[test]
    fn chat_request_tools_roundtrip() {
        use crate::messages::{ChatRequest, Message, MessageContent, ToolSpec};

        let original = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("help".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
            }],
            tools: Some(vec![
                ToolSpec {
                    name: "read_file".into(),
                    description: Some("Read a file from disk".into()),
                    parameters: {
                        let mut m = HashMap::new();
                        m.insert("type".into(), serde_json::json!("object"));
                        m
                    },
                    extensions: HashMap::new(),
                },
                ToolSpec {
                    name: "write_file".into(),
                    description: None,
                    parameters: HashMap::new(),
                    extensions: HashMap::new(),
                },
            ]),
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
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_request_to_proto(&original);
        let restored = super::proto_chat_request_to_native(proto);

        let tools = restored.tools.expect("tools must be Some");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[0].description, Some("Read a file from disk".into()));
        let params_type = tools[0].parameters.get("type");
        assert_eq!(params_type, Some(&serde_json::json!("object")));
        assert_eq!(tools[1].name, "write_file");
        assert!(tools[1].description.is_none());
    }

    #[test]
    fn chat_request_response_format_json_roundtrip() {
        use crate::messages::{ChatRequest, Message, MessageContent, ResponseFormat};

        let original = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("go".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
            }],
            tools: None,
            response_format: Some(ResponseFormat::Json),
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
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_request_to_proto(&original);
        let restored = super::proto_chat_request_to_native(proto);
        assert_eq!(restored.response_format, Some(ResponseFormat::Json));
    }

    #[test]
    fn chat_request_response_format_json_schema_roundtrip() {
        use crate::messages::{ChatRequest, Message, MessageContent, ResponseFormat};

        let schema = {
            let mut m = HashMap::new();
            m.insert("type".into(), serde_json::json!("object"));
            m.insert(
                "properties".into(),
                serde_json::json!({"answer": {"type": "string"}}),
            );
            m
        };

        let original = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("go".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
            }],
            tools: None,
            response_format: Some(ResponseFormat::JsonSchema {
                schema: schema.clone(),
                strict: Some(true),
            }),
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
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_request_to_proto(&original);
        let restored = super::proto_chat_request_to_native(proto);

        match restored.response_format {
            Some(ResponseFormat::JsonSchema {
                schema: restored_schema,
                strict,
            }) => {
                assert_eq!(
                    restored_schema.get("type"),
                    Some(&serde_json::json!("object"))
                );
                assert_eq!(strict, Some(true));
            }
            other => panic!("Expected JsonSchema response_format, got: {other:?}"),
        }
    }

    #[test]
    fn chat_request_tool_choice_object_roundtrip() {
        use crate::messages::{ChatRequest, Message, MessageContent, ToolChoice};

        let tool_choice_obj = {
            let mut m = HashMap::new();
            m.insert("type".into(), serde_json::json!("function"));
            m.insert("function".into(), serde_json::json!({"name": "read_file"}));
            m
        };

        let original = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("do it".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
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
            tool_choice: Some(ToolChoice::Object(tool_choice_obj.clone())),
            stop: None,
            reasoning_effort: None,
            timeout: None,
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_request_to_proto(&original);
        let restored = super::proto_chat_request_to_native(proto);

        match restored.tool_choice {
            Some(ToolChoice::Object(obj)) => {
                assert_eq!(obj.get("type"), Some(&serde_json::json!("function")));
                assert_eq!(
                    obj.get("function"),
                    Some(&serde_json::json!({"name": "read_file"}))
                );
            }
            other => panic!("Expected ToolChoice::Object, got: {other:?}"),
        }
    }

    // -- ChatResponse conversion tests (RED: functions not yet implemented) --

    #[test]
    fn chat_response_minimal_roundtrip() {
        use crate::messages::ChatResponse;

        let original = ChatResponse {
            content: vec![crate::messages::ContentBlock::Text {
                text: "Hello, world!".into(),
                visibility: None,
                extensions: HashMap::new(),
            }],
            tool_calls: None,
            usage: None,
            degradation: None,
            finish_reason: None,
            metadata: None,
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_response_to_proto(&original);
        let restored = super::proto_chat_response_to_native(proto);

        assert_eq!(restored.content.len(), 1);
        assert_eq!(restored.content, original.content);
        assert!(restored.tool_calls.is_none());
        assert!(restored.usage.is_none());
        assert!(restored.degradation.is_none());
        assert!(restored.finish_reason.is_none());
        assert!(restored.metadata.is_none());
    }

    #[test]
    fn chat_response_full_fields_roundtrip() {
        use crate::messages::{ChatResponse, Degradation, ToolCall, Usage};

        let original = ChatResponse {
            content: vec![
                crate::messages::ContentBlock::Text {
                    text: "Here's the answer.".into(),
                    visibility: None,
                    extensions: HashMap::new(),
                },
                crate::messages::ContentBlock::Thinking {
                    thinking: "Let me reason...".into(),
                    signature: Some("sig_xyz".into()),
                    visibility: Some(crate::messages::Visibility::Internal),
                    content: None,
                    extensions: HashMap::new(),
                },
            ],
            tool_calls: Some(vec![ToolCall {
                id: "call_001".into(),
                name: "search".into(),
                arguments: HashMap::from([
                    ("query".to_string(), serde_json::json!("rust async")),
                    ("limit".to_string(), serde_json::json!(10)),
                ]),
                extensions: HashMap::new(),
            }]),
            usage: Some(Usage {
                input_tokens: 200,
                output_tokens: 100,
                total_tokens: 300,
                reasoning_tokens: Some(50),
                cache_read_tokens: Some(20),
                cache_write_tokens: None,
                extensions: HashMap::new(),
            }),
            degradation: Some(Degradation {
                requested: "gpt-4-turbo".into(),
                actual: "gpt-4".into(),
                reason: "rate limit".into(),
                extensions: HashMap::new(),
            }),
            finish_reason: Some("stop".into()),
            metadata: Some(HashMap::from([(
                "request_id".to_string(),
                serde_json::json!("req_abc123"),
            )])),
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_response_to_proto(&original);
        let restored = super::proto_chat_response_to_native(proto);

        // content blocks
        assert_eq!(restored.content.len(), 2);
        assert_eq!(restored.content, original.content);

        // tool_calls
        let tool_calls = restored
            .tool_calls
            .as_ref()
            .expect("tool_calls must be Some");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_001");
        assert_eq!(tool_calls[0].name, "search");
        assert_eq!(
            tool_calls[0].arguments.get("query"),
            Some(&serde_json::json!("rust async"))
        );
        assert_eq!(
            tool_calls[0].arguments.get("limit"),
            Some(&serde_json::json!(10))
        );

        // usage
        let usage = restored.usage.as_ref().expect("usage must be Some");
        assert_eq!(usage.input_tokens, 200);
        assert_eq!(usage.output_tokens, 100);
        assert_eq!(usage.total_tokens, 300);
        assert_eq!(usage.reasoning_tokens, Some(50));
        assert_eq!(usage.cache_read_tokens, Some(20));

        // degradation
        let deg = restored
            .degradation
            .as_ref()
            .expect("degradation must be Some");
        assert_eq!(deg.requested, "gpt-4-turbo");
        assert_eq!(deg.actual, "gpt-4");
        assert_eq!(deg.reason, "rate limit");

        // finish_reason
        assert_eq!(restored.finish_reason, Some("stop".into()));

        // metadata
        let meta = restored.metadata.as_ref().expect("metadata must be Some");
        assert_eq!(
            meta.get("request_id"),
            Some(&serde_json::json!("req_abc123"))
        );
    }

    #[test]
    fn chat_response_tool_calls_roundtrip() {
        use crate::messages::{ChatResponse, ToolCall};

        let original = ChatResponse {
            content: vec![crate::messages::ContentBlock::Text {
                text: "Let me look that up.".into(),
                visibility: None,
                extensions: HashMap::new(),
            }],
            tool_calls: Some(vec![
                ToolCall {
                    id: "call_A".into(),
                    name: "read_file".into(),
                    arguments: HashMap::from([(
                        "path".to_string(),
                        serde_json::json!("/tmp/data.txt"),
                    )]),
                    extensions: HashMap::new(),
                },
                ToolCall {
                    id: "call_B".into(),
                    name: "write_file".into(),
                    arguments: HashMap::from([
                        ("path".to_string(), serde_json::json!("/tmp/out.txt")),
                        ("content".to_string(), serde_json::json!("hello")),
                    ]),
                    extensions: HashMap::new(),
                },
            ]),
            usage: None,
            degradation: None,
            finish_reason: Some("tool_calls".into()),
            metadata: None,
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_response_to_proto(&original);
        let restored = super::proto_chat_response_to_native(proto);

        let tool_calls = restored.tool_calls.expect("tool_calls must be Some");
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].id, "call_A");
        assert_eq!(tool_calls[0].name, "read_file");
        assert_eq!(tool_calls[1].id, "call_B");
        assert_eq!(tool_calls[1].name, "write_file");
        assert_eq!(restored.finish_reason, Some("tool_calls".into()));
    }

    #[test]
    fn chat_response_empty_content_roundtrip() {
        use crate::messages::ChatResponse;

        let original = ChatResponse {
            content: vec![],
            tool_calls: None,
            usage: None,
            degradation: None,
            finish_reason: Some("stop".into()),
            metadata: None,
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_response_to_proto(&original);
        let restored = super::proto_chat_response_to_native(proto);

        assert!(restored.content.is_empty());
        assert_eq!(restored.finish_reason, Some("stop".into()));
    }

    // -- HookResult native → proto conversion tests (RED: function not yet implemented) --

    #[test]
    fn hook_result_default_native_to_proto_fields() {
        use super::super::amplifier_module;
        use crate::models::HookResult;

        let native = HookResult::default();
        let proto = super::native_hook_result_to_proto(&native);

        // action: Continue (default)
        assert_eq!(proto.action, amplifier_module::HookAction::Continue as i32);
        // string optionals → empty strings
        assert_eq!(proto.reason, "");
        assert_eq!(proto.context_injection, "");
        assert_eq!(proto.approval_prompt, "");
        assert_eq!(proto.user_message, "");
        assert_eq!(proto.user_message_source, "");
        // data_json: None → empty string
        assert_eq!(proto.data_json, "");
        // bools: false (default)
        assert!(!proto.ephemeral);
        assert!(!proto.suppress_output);
        assert!(!proto.append_to_last_tool_result);
        // approval_options: None → empty vec
        assert!(proto.approval_options.is_empty());
        // approval_timeout: 300.0 → Some(300.0)
        assert_eq!(proto.approval_timeout, Some(300.0));
        // approval_default: Deny (default)
        assert_eq!(
            proto.approval_default,
            amplifier_module::ApprovalDefault::Deny as i32
        );
        // context_injection_role: System (default)
        assert_eq!(
            proto.context_injection_role,
            amplifier_module::ContextInjectionRole::System as i32
        );
        // user_message_level: Info (default)
        assert_eq!(
            proto.user_message_level,
            amplifier_module::UserMessageLevel::Info as i32
        );
    }

    #[test]
    fn hook_result_all_hook_action_variants_to_proto() {
        use super::super::amplifier_module;
        use crate::models::{HookAction, HookResult};

        let cases = [
            (
                HookAction::Continue,
                amplifier_module::HookAction::Continue as i32,
            ),
            (
                HookAction::Modify,
                amplifier_module::HookAction::Modify as i32,
            ),
            (HookAction::Deny, amplifier_module::HookAction::Deny as i32),
            (
                HookAction::InjectContext,
                amplifier_module::HookAction::InjectContext as i32,
            ),
            (
                HookAction::AskUser,
                amplifier_module::HookAction::AskUser as i32,
            ),
        ];
        for (native_action, expected_i32) in cases {
            let native = HookResult {
                action: native_action,
                ..Default::default()
            };
            let proto = super::native_hook_result_to_proto(&native);
            assert_eq!(proto.action, expected_i32);
        }
    }

    #[test]
    fn hook_result_context_injection_role_all_variants_to_proto() {
        use super::super::amplifier_module;
        use crate::models::{ContextInjectionRole, HookResult};

        let cases = [
            (
                ContextInjectionRole::System,
                amplifier_module::ContextInjectionRole::System as i32,
            ),
            (
                ContextInjectionRole::User,
                amplifier_module::ContextInjectionRole::User as i32,
            ),
            (
                ContextInjectionRole::Assistant,
                amplifier_module::ContextInjectionRole::Assistant as i32,
            ),
        ];
        for (native_role, expected_i32) in cases {
            let native = HookResult {
                context_injection_role: native_role,
                ..Default::default()
            };
            let proto = super::native_hook_result_to_proto(&native);
            assert_eq!(proto.context_injection_role, expected_i32);
        }
    }

    #[test]
    fn hook_result_approval_default_all_variants_to_proto() {
        use super::super::amplifier_module;
        use crate::models::{ApprovalDefault, HookResult};

        // Allow → Approve
        let native = HookResult {
            approval_default: ApprovalDefault::Allow,
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert_eq!(
            proto.approval_default,
            amplifier_module::ApprovalDefault::Approve as i32
        );

        // Deny → Deny
        let native = HookResult {
            approval_default: ApprovalDefault::Deny,
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert_eq!(
            proto.approval_default,
            amplifier_module::ApprovalDefault::Deny as i32
        );
    }

    #[test]
    fn hook_result_user_message_level_all_variants_to_proto() {
        use super::super::amplifier_module;
        use crate::models::{HookResult, UserMessageLevel};

        let cases = [
            (
                UserMessageLevel::Info,
                amplifier_module::UserMessageLevel::Info as i32,
            ),
            (
                UserMessageLevel::Warning,
                amplifier_module::UserMessageLevel::Warning as i32,
            ),
            (
                UserMessageLevel::Error,
                amplifier_module::UserMessageLevel::Error as i32,
            ),
        ];
        for (native_level, expected_i32) in cases {
            let native = HookResult {
                user_message_level: native_level,
                ..Default::default()
            };
            let proto = super::native_hook_result_to_proto(&native);
            assert_eq!(proto.user_message_level, expected_i32);
        }
    }

    #[test]
    fn hook_result_string_option_fields_to_proto() {
        use crate::models::HookResult;

        let native = HookResult {
            reason: Some("blocked".to_string()),
            context_injection: Some("extra context".to_string()),
            approval_prompt: Some("Proceed?".to_string()),
            user_message: Some("Watch out!".to_string()),
            user_message_source: Some("security-hook".to_string()),
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert_eq!(proto.reason, "blocked");
        assert_eq!(proto.context_injection, "extra context");
        assert_eq!(proto.approval_prompt, "Proceed?");
        assert_eq!(proto.user_message, "Watch out!");
        assert_eq!(proto.user_message_source, "security-hook");
    }

    #[test]
    fn hook_result_bool_fields_to_proto() {
        use crate::models::HookResult;

        let native = HookResult {
            ephemeral: true,
            suppress_output: true,
            append_to_last_tool_result: true,
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert!(proto.ephemeral);
        assert!(proto.suppress_output);
        assert!(proto.append_to_last_tool_result);
    }

    #[test]
    fn hook_result_approval_options_some_to_proto() {
        use crate::models::HookResult;

        let native = HookResult {
            approval_options: Some(vec!["allow".to_string(), "deny".to_string()]),
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert_eq!(
            proto.approval_options,
            vec!["allow".to_string(), "deny".to_string()]
        );
    }

    #[test]
    fn hook_result_approval_options_none_to_empty_vec() {
        use crate::models::HookResult;

        let native = HookResult {
            approval_options: None,
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert!(proto.approval_options.is_empty());
    }

    #[test]
    fn hook_result_approval_timeout_to_optional_proto() {
        use crate::models::HookResult;

        // Default 300.0 → Some(300.0)
        let native = HookResult {
            approval_timeout: 300.0,
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert_eq!(proto.approval_timeout, Some(300.0));

        // Custom 60.0 → Some(60.0)
        let native = HookResult {
            approval_timeout: 60.0,
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert_eq!(proto.approval_timeout, Some(60.0));
    }

    #[test]
    fn hook_result_data_json_some_to_proto() {
        use crate::models::HookResult;

        let mut data = HashMap::new();
        data.insert("key".to_string(), serde_json::json!("value"));
        let native = HookResult {
            data: Some(data),
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        // Should be valid non-empty JSON
        assert!(!proto.data_json.is_empty());
        let parsed: serde_json::Value =
            serde_json::from_str(&proto.data_json).expect("data_json should be valid JSON");
        assert_eq!(parsed["key"], serde_json::json!("value"));
    }

    #[test]
    fn hook_result_data_json_none_to_empty_string() {
        use crate::models::HookResult;

        let native = HookResult {
            data: None,
            ..Default::default()
        };
        let proto = super::native_hook_result_to_proto(&native);
        assert_eq!(proto.data_json, "");
    }

    #[test]
    fn hook_result_roundtrip_via_bridge_reverse() {
        use crate::bridges::grpc_hook::GrpcHookBridge;
        use crate::models::{
            ApprovalDefault, ContextInjectionRole, HookAction, HookResult, UserMessageLevel,
        };

        let original = HookResult {
            action: HookAction::AskUser,
            data: None,
            reason: Some("needs approval".to_string()),
            context_injection: Some("please confirm".to_string()),
            context_injection_role: ContextInjectionRole::User,
            ephemeral: true,
            approval_prompt: Some("Allow this action?".to_string()),
            approval_options: Some(vec!["yes".to_string(), "no".to_string()]),
            approval_timeout: 120.0,
            approval_default: ApprovalDefault::Allow,
            suppress_output: true,
            user_message: Some("Action requires approval".to_string()),
            user_message_level: UserMessageLevel::Warning,
            user_message_source: Some("approval-hook".to_string()),
            append_to_last_tool_result: false,
            extensions: HashMap::new(),
        };

        let proto = super::native_hook_result_to_proto(&original);
        let restored = GrpcHookBridge::proto_to_native_hook_result(proto);

        assert_eq!(restored.action, original.action);
        assert_eq!(restored.reason, original.reason);
        assert_eq!(restored.context_injection, original.context_injection);
        assert_eq!(
            restored.context_injection_role,
            original.context_injection_role
        );
        assert_eq!(restored.ephemeral, original.ephemeral);
        assert_eq!(restored.approval_prompt, original.approval_prompt);
        assert_eq!(restored.approval_options, original.approval_options);
        assert_eq!(restored.approval_timeout, original.approval_timeout);
        assert_eq!(restored.approval_default, original.approval_default);
        assert_eq!(restored.suppress_output, original.suppress_output);
        assert_eq!(restored.user_message, original.user_message);
        assert_eq!(restored.user_message_level, original.user_message_level);
        assert_eq!(restored.user_message_source, original.user_message_source);
        assert_eq!(
            restored.append_to_last_tool_result,
            original.append_to_last_tool_result
        );
    }

    #[test]
    fn chat_request_multiple_messages_roundtrip() {
        use crate::messages::{ChatRequest, ContentBlock, Message, MessageContent};

        let original = ChatRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: MessageContent::Text("You are helpful.".into()),
                    name: None,
                    tool_call_id: None,
                    metadata: None,
                    extensions: HashMap::new(),
                },
                Message {
                    role: Role::User,
                    content: MessageContent::Blocks(vec![ContentBlock::Text {
                        text: "Help me!".into(),
                        visibility: None,
                        extensions: HashMap::new(),
                    }]),
                    name: None,
                    tool_call_id: None,
                    metadata: None,
                    extensions: HashMap::new(),
                },
            ],
            tools: None,
            response_format: None,
            temperature: Some(1.0),
            top_p: None,
            max_output_tokens: None,
            conversation_id: None,
            stream: None,
            metadata: None,
            model: Some("claude-3-opus".into()),
            tool_choice: None,
            stop: None,
            reasoning_effort: None,
            timeout: None,
            extensions: HashMap::new(),
        };

        let proto = super::native_chat_request_to_proto(&original);
        let restored = super::proto_chat_request_to_native(proto);

        assert_eq!(restored.messages.len(), 2);
        assert_eq!(restored.messages[0].role, Role::System);
        assert_eq!(
            restored.messages[0].content,
            MessageContent::Text("You are helpful.".into())
        );
        assert_eq!(restored.messages[1].role, Role::User);
        assert_eq!(restored.model, Some("claude-3-opus".into()));
        assert_eq!(restored.temperature, Some(1.0));
    }
}
