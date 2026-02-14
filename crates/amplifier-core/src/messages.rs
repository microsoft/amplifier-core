//! Chat protocol models for the Amplifier request/response envelope.
//!
//! Ports Python's `message_models.py` (Pydantic envelope types) and
//! `content_models.py` (event/streaming types) to Rust with full serde
//! JSON (de)serialization.
//!
//! # Key design decisions
//!
//! - [`ContentBlock`] uses `#[serde(tag = "type")]` for the discriminated
//!   union, matching Python's `Field(discriminator="type")`.
//! - [`MessageContent`] uses `#[serde(untagged)]` so a plain string
//!   serializes as `"hello"` and an array serializes as `[{...}]`.
//! - All structs whose Python counterpart has `extra="allow"` carry
//!   `#[serde(flatten)] pub extensions: HashMap<String, Value>` to
//!   preserve unknown fields through round-trips.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---- Simple enums ----

/// Types of content blocks.
///
/// Maps to Python's `ContentBlockType(str, Enum)` from `content_models.py`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContentBlockType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "thinking")]
    Thinking,
    #[serde(rename = "tool_call")]
    ToolCall,
    #[serde(rename = "tool_result")]
    ToolResult,
}

/// Visibility level for content blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Internal,
    Developer,
    User,
}

// ---- ContentBlock tagged union ----

/// Content block discriminated union.
///
/// Maps to Python's `ContentBlockUnion` — a tagged union of all content
/// block types using `"type"` as the discriminator field.
///
/// Each variant corresponds to a Pydantic model in `message_models.py`:
/// `TextBlock`, `ThinkingBlock`, `RedactedThinkingBlock`, `ToolCallBlock`,
/// `ToolResultBlock`, `ImageBlock`, `ReasoningBlock`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<Visibility>,
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<Visibility>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<Vec<Value>>,
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking {
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<Visibility>,
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "tool_call")]
    ToolCall {
        id: String,
        name: String,
        input: HashMap<String, Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<Visibility>,
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_call_id: String,
        output: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<Visibility>,
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "image")]
    Image {
        source: HashMap<String, Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<Visibility>,
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        content: Vec<Value>,
        summary: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<Visibility>,
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
}

// ---- Message types ----

/// Message content: either a plain string or structured content blocks.
///
/// Python: `content: Union[str, list[ContentBlockUnion]]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Message role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    Developer,
    User,
    Assistant,
    Function,
    Tool,
}

/// Single message in conversation history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, Value>>,
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Tool/function specification with JSON Schema parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub parameters: HashMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

// ---- Response format ----

/// Response format specification.
///
/// Maps to Python's `ResponseFormat` union of `ResponseFormatText`,
/// `ResponseFormatJson`, and `ResponseFormatJsonSchema`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseFormat {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "json_schema")]
    JsonSchema {
        #[serde(alias = "json_schema")]
        schema: HashMap<String, Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

// ---- Tool choice ----

/// Tool choice: a string like `"auto"`/`"none"` or a structured object.
///
/// Python: `tool_choice: str | dict[str, Any] | None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    String(String),
    Object(HashMap<String, Value>),
}

// ---- Request / Response types ----

/// Complete chat request to provider.
///
/// Maps to Python's `ChatRequest`. All optional fields use
/// `skip_serializing_if` so absent fields don't appear in JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSpec>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Tool call in response.
///
/// Maps to Python's `ToolCall` (distinct from `ToolCallBlock` content block).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: HashMap<String, Value>,
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Token usage information.
///
/// The three required fields are reported by all providers. Optional fields
/// surface commonly-available metrics. Unknown provider-specific metrics
/// are captured in `extensions` via `#[serde(flatten)]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<i64>,
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Model degradation information.
///
/// When a provider falls back to a different model, this records what was
/// requested vs. what was actually used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Degradation {
    pub requested: String,
    pub actual: String,
    pub reason: String,
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Response from provider.
///
/// Maps to Python's `ChatResponse`. Contains content blocks, optional
/// tool calls, usage info, and metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation: Option<Degradation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, Value>>,
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- ContentBlockType ----

    #[test]
    fn content_block_type_serialization() {
        assert_eq!(
            serde_json::to_value(ContentBlockType::Text).unwrap(),
            json!("text")
        );
        assert_eq!(
            serde_json::to_value(ContentBlockType::Thinking).unwrap(),
            json!("thinking")
        );
        assert_eq!(
            serde_json::to_value(ContentBlockType::ToolCall).unwrap(),
            json!("tool_call")
        );
        assert_eq!(
            serde_json::to_value(ContentBlockType::ToolResult).unwrap(),
            json!("tool_result")
        );
    }

    #[test]
    fn content_block_type_deserialization() {
        assert_eq!(
            serde_json::from_value::<ContentBlockType>(json!("text")).unwrap(),
            ContentBlockType::Text
        );
        assert_eq!(
            serde_json::from_value::<ContentBlockType>(json!("tool_call")).unwrap(),
            ContentBlockType::ToolCall
        );
    }

    // ---- Visibility ----

    #[test]
    fn visibility_serialization() {
        assert_eq!(
            serde_json::to_value(Visibility::Internal).unwrap(),
            json!("internal")
        );
        assert_eq!(
            serde_json::to_value(Visibility::Developer).unwrap(),
            json!("developer")
        );
        assert_eq!(
            serde_json::to_value(Visibility::User).unwrap(),
            json!("user")
        );
    }

    // ---- ContentBlock discriminated union ----

    #[test]
    fn content_block_text_serialization() {
        let block = ContentBlock::Text {
            text: "hello".into(),
            visibility: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text", "ContentBlock must use internally-tagged 'type' field");
        assert_eq!(json["text"], "hello");
        assert!(json.get("visibility").is_none(), "None fields must be omitted");
    }

    #[test]
    fn content_block_text_deserialization() {
        let json = json!({"type": "text", "text": "hello"});
        let block: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(
            block,
            ContentBlock::Text {
                text: "hello".into(),
                visibility: None,
                extensions: HashMap::new(),
            }
        );
    }

    #[test]
    fn content_block_text_with_visibility() {
        let block = ContentBlock::Text {
            text: "hi".into(),
            visibility: Some(Visibility::User),
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["visibility"], "user");
    }

    #[test]
    fn content_block_thinking_round_trip() {
        let block = ContentBlock::Thinking {
            thinking: "let me think".into(),
            signature: Some("sig123".into()),
            visibility: Some(Visibility::Internal),
            content: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "thinking");
        assert_eq!(json["thinking"], "let me think");
        assert_eq!(json["signature"], "sig123");
        let deserialized: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, block);
    }

    #[test]
    fn content_block_redacted_thinking_round_trip() {
        let block = ContentBlock::RedactedThinking {
            data: "redacted_data".into(),
            visibility: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "redacted_thinking");
        assert_eq!(json["data"], "redacted_data");
        let deserialized: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, block);
    }

    #[test]
    fn content_block_tool_call_round_trip() {
        let mut input = HashMap::new();
        input.insert("path".into(), json!("/tmp/test"));
        let block = ContentBlock::ToolCall {
            id: "call_123".into(),
            name: "read_file".into(),
            input,
            visibility: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_call");
        assert_eq!(json["id"], "call_123");
        assert_eq!(json["name"], "read_file");
        assert_eq!(json["input"]["path"], "/tmp/test");
        let deserialized: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, block);
    }

    #[test]
    fn content_block_tool_result_round_trip() {
        let block = ContentBlock::ToolResult {
            tool_call_id: "call_123".into(),
            output: json!("file contents"),
            visibility: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_call_id"], "call_123");
        let deserialized: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, block);
    }

    #[test]
    fn content_block_image_round_trip() {
        let mut source = HashMap::new();
        source.insert("media_type".into(), json!("image/png"));
        source.insert("data".into(), json!("abc123"));
        let block = ContentBlock::Image {
            source,
            visibility: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "image");
        assert_eq!(json["source"]["media_type"], "image/png");
        let deserialized: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, block);
    }

    #[test]
    fn content_block_reasoning_round_trip() {
        let block = ContentBlock::Reasoning {
            content: vec![json!("step1"), json!("step2")],
            summary: vec![json!("result")],
            visibility: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "reasoning");
        let deserialized: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, block);
    }

    #[test]
    fn content_block_extensions_preserved() {
        let json = json!({
            "type": "text",
            "text": "hello",
            "custom_field": "custom_value",
            "another": 42
        });
        let block: ContentBlock = serde_json::from_value(json).unwrap();
        if let ContentBlock::Text { extensions, .. } = &block {
            assert_eq!(extensions.get("custom_field"), Some(&json!("custom_value")));
            assert_eq!(extensions.get("another"), Some(&json!(42)));
        } else {
            panic!("Expected Text variant");
        }
        // Round-trip preserves extensions
        let serialized = serde_json::to_value(&block).unwrap();
        assert_eq!(serialized["custom_field"], "custom_value");
        assert_eq!(serialized["another"], 42);
    }

    // ---- MessageContent ----

    #[test]
    fn message_content_string_serialization() {
        let content = MessageContent::Text("hello".into());
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json, json!("hello"), "String content must serialize as plain string (untagged)");
    }

    #[test]
    fn message_content_blocks_serialization() {
        let content = MessageContent::Blocks(vec![ContentBlock::Text {
            text: "hello".into(),
            visibility: None,
            extensions: HashMap::new(),
        }]);
        let json = serde_json::to_value(&content).unwrap();
        assert!(json.is_array(), "Block content must serialize as array (untagged)");
        assert_eq!(json[0]["type"], "text");
        assert_eq!(json[0]["text"], "hello");
    }

    #[test]
    fn message_content_string_deserialization() {
        let json = json!("hello");
        let content: MessageContent = serde_json::from_value(json).unwrap();
        assert_eq!(content, MessageContent::Text("hello".into()));
    }

    #[test]
    fn message_content_blocks_deserialization() {
        let json = json!([{"type": "text", "text": "hello"}]);
        let content: MessageContent = serde_json::from_value(json).unwrap();
        match content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                assert!(matches!(
                    &blocks[0],
                    ContentBlock::Text { text, .. } if text == "hello"
                ));
            }
            _ => panic!("Expected Blocks variant"),
        }
    }

    // ---- Role ----

    #[test]
    fn role_serialization() {
        assert_eq!(serde_json::to_value(Role::System).unwrap(), json!("system"));
        assert_eq!(serde_json::to_value(Role::Developer).unwrap(), json!("developer"));
        assert_eq!(serde_json::to_value(Role::User).unwrap(), json!("user"));
        assert_eq!(serde_json::to_value(Role::Assistant).unwrap(), json!("assistant"));
        assert_eq!(serde_json::to_value(Role::Function).unwrap(), json!("function"));
        assert_eq!(serde_json::to_value(Role::Tool).unwrap(), json!("tool"));
    }

    // ---- Message ----

    #[test]
    fn message_with_string_content() {
        let msg = Message {
            role: Role::User,
            content: MessageContent::Text("hello".into()),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "hello");
        assert!(json.get("name").is_none());
        assert!(json.get("tool_call_id").is_none());
    }

    #[test]
    fn message_with_block_content() {
        let msg = Message {
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
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert!(json["content"].is_array());
        assert_eq!(json["content"][0]["type"], "text");
    }

    #[test]
    fn message_round_trip() {
        let json = json!({
            "role": "tool",
            "content": "result",
            "tool_call_id": "call_123",
            "name": "read_file"
        });
        let msg: Message = serde_json::from_value(json).unwrap();
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.content, MessageContent::Text("result".into()));
        assert_eq!(msg.tool_call_id, Some("call_123".into()));
        assert_eq!(msg.name, Some("read_file".into()));
    }

    #[test]
    fn message_extensions_preserved() {
        let json = json!({
            "role": "user",
            "content": "hello",
            "custom_field": "preserved"
        });
        let msg: Message = serde_json::from_value(json).unwrap();
        assert_eq!(
            msg.extensions.get("custom_field"),
            Some(&json!("preserved"))
        );
        let serialized = serde_json::to_value(&msg).unwrap();
        assert_eq!(serialized["custom_field"], "preserved");
    }

    // ---- ToolSpec ----

    #[test]
    fn tool_spec_round_trip() {
        let spec = ToolSpec {
            name: "read_file".into(),
            parameters: {
                let mut m = HashMap::new();
                m.insert("type".into(), json!("object"));
                m.insert(
                    "properties".into(),
                    json!({"path": {"type": "string"}}),
                );
                m
            },
            description: Some("Read a file".into()),
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["name"], "read_file");
        assert_eq!(json["description"], "Read a file");
        let deserialized: ToolSpec = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, spec);
    }

    // ---- ResponseFormat ----

    #[test]
    fn response_format_text_serialization() {
        let fmt = ResponseFormat::Text;
        let json = serde_json::to_value(&fmt).unwrap();
        assert_eq!(json, json!({"type": "text"}));
    }

    #[test]
    fn response_format_json_serialization() {
        let fmt = ResponseFormat::Json;
        let json = serde_json::to_value(&fmt).unwrap();
        assert_eq!(json, json!({"type": "json"}));
    }

    #[test]
    fn response_format_json_schema_serialization() {
        let fmt = ResponseFormat::JsonSchema {
            schema: {
                let mut m = HashMap::new();
                m.insert("type".into(), json!("object"));
                m
            },
            strict: Some(true),
        };
        let json = serde_json::to_value(&fmt).unwrap();
        assert_eq!(json["type"], "json_schema");
        assert_eq!(json["schema"]["type"], "object");
        assert_eq!(json["strict"], true);
    }

    #[test]
    fn response_format_text_deserialization() {
        let json = json!({"type": "text"});
        let fmt: ResponseFormat = serde_json::from_value(json).unwrap();
        assert_eq!(fmt, ResponseFormat::Text);
    }

    #[test]
    fn response_format_json_schema_deserialization() {
        let json = json!({"type": "json_schema", "schema": {"type": "object"}});
        let fmt: ResponseFormat = serde_json::from_value(json).unwrap();
        match &fmt {
            ResponseFormat::JsonSchema { schema, strict } => {
                assert_eq!(schema.get("type"), Some(&json!("object")));
                assert_eq!(*strict, None);
            }
            _ => panic!("Expected JsonSchema variant"),
        }
    }

    #[test]
    fn response_format_json_schema_alias() {
        // Accept "json_schema" key as alias for "schema" (matching Python field name)
        let json = json!({"type": "json_schema", "json_schema": {"type": "object"}});
        let fmt: ResponseFormat = serde_json::from_value(json).unwrap();
        assert!(matches!(fmt, ResponseFormat::JsonSchema { .. }));
    }

    // ---- ToolChoice ----

    #[test]
    fn tool_choice_string_serialization() {
        let tc = ToolChoice::String("auto".into());
        let json = serde_json::to_value(&tc).unwrap();
        assert_eq!(json, json!("auto"), "String tool_choice must serialize as plain string");
    }

    #[test]
    fn tool_choice_object_serialization() {
        let mut obj = HashMap::new();
        obj.insert("type".into(), json!("function"));
        obj.insert("function".into(), json!({"name": "read_file"}));
        let tc = ToolChoice::Object(obj);
        let json = serde_json::to_value(&tc).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "read_file");
    }

    #[test]
    fn tool_choice_string_deserialization() {
        let json = json!("none");
        let tc: ToolChoice = serde_json::from_value(json).unwrap();
        assert_eq!(tc, ToolChoice::String("none".into()));
    }

    // ---- ChatRequest ----

    #[test]
    fn chat_request_minimal() {
        let req = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("hello".into()),
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
        let json = serde_json::to_value(&req).unwrap();
        assert!(json["messages"].is_array());
        assert_eq!(json["messages"][0]["content"], "hello");
        // Optional fields must NOT be present
        assert!(json.get("tools").is_none());
        assert!(json.get("temperature").is_none());
        assert!(json.get("model").is_none());
    }

    #[test]
    fn chat_request_all_fields() {
        let req = ChatRequest {
            messages: vec![Message {
                role: Role::System,
                content: MessageContent::Text("You are helpful.".into()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
            }],
            tools: Some(vec![ToolSpec {
                name: "search".into(),
                parameters: HashMap::new(),
                description: Some("Search the web".into()),
                extensions: HashMap::new(),
            }]),
            response_format: Some(ResponseFormat::Text),
            temperature: Some(0.7),
            top_p: Some(0.9),
            max_output_tokens: Some(4096),
            conversation_id: Some("conv_123".into()),
            stream: Some(true),
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("source".into(), json!("test"));
                m
            }),
            model: Some("gpt-4".into()),
            tool_choice: Some(ToolChoice::String("auto".into())),
            stop: Some(vec!["END".into()]),
            reasoning_effort: Some("high".into()),
            timeout: Some(30.0),
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["temperature"], 0.7);
        assert_eq!(json["model"], "gpt-4");
        assert_eq!(json["tool_choice"], "auto");
        assert_eq!(json["stop"], json!(["END"]));
        assert_eq!(json["reasoning_effort"], "high");
        assert_eq!(json["timeout"], 30.0);
    }

    #[test]
    fn chat_request_round_trip() {
        let json = json!({
            "messages": [{"role": "user", "content": "hello"}],
            "model": "gpt-4",
            "temperature": 0.5
        });
        let req: ChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.model, Some("gpt-4".into()));
        assert_eq!(req.temperature, Some(0.5));
        assert!(req.tools.is_none());
    }

    #[test]
    fn chat_request_extensions_preserved() {
        let json = json!({
            "messages": [{"role": "user", "content": "hello"}],
            "custom_param": "custom_value"
        });
        let req: ChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(
            req.extensions.get("custom_param"),
            Some(&json!("custom_value"))
        );
    }

    // ---- ToolCall ----

    #[test]
    fn tool_call_round_trip() {
        let tc = ToolCall {
            id: "call_456".into(),
            name: "write_file".into(),
            arguments: {
                let mut m = HashMap::new();
                m.insert("path".into(), json!("/tmp/out"));
                m.insert("content".into(), json!("data"));
                m
            },
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&tc).unwrap();
        assert_eq!(json["id"], "call_456");
        assert_eq!(json["name"], "write_file");
        let deserialized: ToolCall = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, tc);
    }

    // ---- Usage ----

    #[test]
    fn usage_round_trip() {
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            reasoning_tokens: Some(20),
            cache_read_tokens: None,
            cache_write_tokens: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["input_tokens"], 100);
        assert_eq!(json["output_tokens"], 50);
        assert_eq!(json["total_tokens"], 150);
        assert_eq!(json["reasoning_tokens"], 20);
        assert!(json.get("cache_read_tokens").is_none());
        let deserialized: Usage = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, usage);
    }

    #[test]
    fn usage_extensions_preserved() {
        let json = json!({
            "input_tokens": 100,
            "output_tokens": 50,
            "total_tokens": 150,
            "cache_creation_input_tokens": 25
        });
        let usage: Usage = serde_json::from_value(json).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(
            usage.extensions.get("cache_creation_input_tokens"),
            Some(&json!(25))
        );
        // Round-trip preserves
        let serialized = serde_json::to_value(&usage).unwrap();
        assert_eq!(serialized["cache_creation_input_tokens"], 25);
    }

    // ---- Degradation ----

    #[test]
    fn degradation_round_trip() {
        let d = Degradation {
            requested: "gpt-4".into(),
            actual: "gpt-3.5-turbo".into(),
            reason: "model unavailable".into(),
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["requested"], "gpt-4");
        assert_eq!(json["actual"], "gpt-3.5-turbo");
        assert_eq!(json["reason"], "model unavailable");
        let deserialized: Degradation = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, d);
    }

    // ---- ChatResponse ----

    #[test]
    fn chat_response_round_trip() {
        let resp = ChatResponse {
            content: vec![ContentBlock::Text {
                text: "Hello!".into(),
                visibility: None,
                extensions: HashMap::new(),
            }],
            tool_calls: None,
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                extensions: HashMap::new(),
            }),
            degradation: None,
            finish_reason: Some("stop".into()),
            metadata: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["content"].is_array());
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "Hello!");
        assert_eq!(json["usage"]["input_tokens"], 10);
        assert_eq!(json["finish_reason"], "stop");
        assert!(json.get("tool_calls").is_none());
        assert!(json.get("degradation").is_none());
    }

    #[test]
    fn chat_response_with_tool_calls() {
        let resp = ChatResponse {
            content: vec![ContentBlock::Text {
                text: "Let me search.".into(),
                visibility: None,
                extensions: HashMap::new(),
            }],
            tool_calls: Some(vec![ToolCall {
                id: "call_789".into(),
                name: "search".into(),
                arguments: {
                    let mut m = HashMap::new();
                    m.insert("query".into(), json!("rust serde"));
                    m
                },
                extensions: HashMap::new(),
            }]),
            usage: None,
            degradation: None,
            finish_reason: Some("tool_calls".into()),
            metadata: None,
            extensions: HashMap::new(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["tool_calls"][0]["id"], "call_789");
        assert_eq!(json["tool_calls"][0]["name"], "search");
        assert_eq!(json["finish_reason"], "tool_calls");
    }

    #[test]
    fn chat_response_deserialization() {
        let json = json!({
            "content": [{"type": "text", "text": "Hello!"}],
            "finish_reason": "stop",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "total_tokens": 150
            }
        });
        let resp: ChatResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.finish_reason, Some("stop".into()));
        assert!(resp.usage.is_some());
    }
}
