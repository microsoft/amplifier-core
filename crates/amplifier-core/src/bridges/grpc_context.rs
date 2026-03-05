//! gRPC bridge for remote context modules.
//!
//! [`GrpcContextBridge`] wraps a [`ContextServiceClient`] (gRPC) and implements
//! the native [`ContextManager`] trait, making a remote context manager
//! indistinguishable from a local one.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! use amplifier_core::bridges::grpc_context::GrpcContextBridge;
//! use amplifier_core::traits::ContextManager;
//! use std::sync::Arc;
//!
//! let bridge = GrpcContextBridge::connect("http://localhost:50051").await?;
//! let context: Arc<dyn ContextManager> = Arc::new(bridge);
//! # Ok(())
//! # }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use tonic::transport::Channel;

use crate::errors::ContextError;
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::context_service_client::ContextServiceClient;
use crate::generated::conversions::{native_message_to_proto, proto_message_to_native};
use crate::messages::Message;
use crate::traits::{ContextManager, Provider};

/// A bridge that wraps a remote gRPC `ContextService` as a native [`ContextManager`].
///
/// The client is held behind a [`tokio::sync::Mutex`] because
/// `ContextServiceClient` methods take `&mut self` and we need to hold
/// the lock across `.await` points.
pub struct GrpcContextBridge {
    client: tokio::sync::Mutex<ContextServiceClient<Channel>>,
}

impl GrpcContextBridge {
    /// Connect to a remote context service.
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = ContextServiceClient::connect(endpoint.to_string()).await?;

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
        })
    }

    /// Convert a [`Value`] (JSON message from context storage) to a proto
    /// [`amplifier_module::Message`].
    ///
    /// If the value can be deserialized as a native [`Message`], the full
    /// typed conversion via [`native_message_to_proto`] is used — preserving
    /// `role`, `name`, `tool_call_id`, `metadata`, and all `ContentBlock`
    /// variants.  Values that don't parse as a `Message` (e.g. plain strings
    /// stored by older code) fall back to the text-only encoding with a
    /// warning log.
    fn value_to_proto_message(message: &Value) -> amplifier_module::Message {
        match serde_json::from_value::<Message>(message.clone()) {
            Ok(native_msg) => native_message_to_proto(native_msg),
            Err(e) => {
                log::warn!(
                    "Failed to parse context message as Message, using text-only fallback: {e}"
                );
                let json_string = serde_json::to_string(message).unwrap_or_else(|ser_err| {
                    log::warn!(
                        "Failed to serialize context message to JSON: {ser_err} — using empty string"
                    );
                    String::new()
                });
                amplifier_module::Message {
                    role: 0,
                    content: Some(amplifier_module::message::Content::TextContent(json_string)),
                    name: String::new(),
                    tool_call_id: String::new(),
                    metadata_json: String::new(),
                }
            }
        }
    }

    /// Convert a proto [`amplifier_module::Message`] back to a [`Value`].
    ///
    /// Uses [`proto_message_to_native`] to get a fully-typed [`Message`] (all
    /// `ContentBlock` variants, `role`, `name`, `tool_call_id`, `metadata`)
    /// and then serialises it to JSON via `serde_json::to_value`.  Returns
    /// [`Value::Null`] only when conversion fails (proto message has no
    /// content, or serialisation errors).
    fn proto_message_to_value(msg: &amplifier_module::Message) -> Value {
        match proto_message_to_native(msg.clone()) {
            Ok(native_msg) => serde_json::to_value(native_msg).unwrap_or_else(|e| {
                log::warn!("Failed to serialise native Message to Value: {e}");
                Value::Null
            }),
            Err(e) => {
                log::warn!("Failed to convert proto Message to native: {e}");
                Value::Null
            }
        }
    }
}

impl ContextManager for GrpcContextBridge {
    fn add_message(
        &self,
        message: Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        Box::pin(async move {
            let proto_msg = Self::value_to_proto_message(&message);

            let request = amplifier_module::AddMessageRequest {
                message: Some(proto_msg),
            };

            {
                let mut client = self.client.lock().await;
                client
                    .add_message(request)
                    .await
                    .map_err(|e| ContextError::Other {
                        message: format!("gRPC: {}", e),
                    })?;
            }

            Ok(())
        })
    }

    fn get_messages_for_request(
        &self,
        token_budget: Option<i64>,
        provider: Option<Arc<dyn Provider>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>> {
        Box::pin(async move {
            let provider_name = provider
                .as_ref()
                .map(|p| p.name().to_string())
                .unwrap_or_default();
            let request = amplifier_module::GetMessagesForRequestParams {
                token_budget: token_budget.unwrap_or(0) as i32,
                provider_name,
            };

            let response = {
                let mut client = self.client.lock().await;
                client
                    .get_messages_for_request(request)
                    .await
                    .map_err(|e| ContextError::Other {
                        message: format!("gRPC: {}", e),
                    })?
            };

            let messages = response
                .into_inner()
                .messages
                .iter()
                .map(Self::proto_message_to_value)
                .collect();

            Ok(messages)
        })
    }

    fn get_messages(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>> {
        Box::pin(async move {
            let response = {
                let mut client = self.client.lock().await;
                client
                    .get_messages(amplifier_module::Empty {})
                    .await
                    .map_err(|e| ContextError::Other {
                        message: format!("gRPC: {}", e),
                    })?
            };

            let messages = response
                .into_inner()
                .messages
                .iter()
                .map(Self::proto_message_to_value)
                .collect();

            Ok(messages)
        })
    }

    fn set_messages(
        &self,
        messages: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        Box::pin(async move {
            let proto_messages: Vec<amplifier_module::Message> =
                messages.iter().map(Self::value_to_proto_message).collect();

            let request = amplifier_module::SetMessagesRequest {
                messages: proto_messages,
            };

            {
                let mut client = self.client.lock().await;
                client
                    .set_messages(request)
                    .await
                    .map_err(|e| ContextError::Other {
                        message: format!("gRPC: {}", e),
                    })?;
            }

            Ok(())
        })
    }

    fn clear(&self) -> Pin<Box<dyn Future<Output = Result<(), ContextError>> + Send + '_>> {
        Box::pin(async move {
            {
                let mut client = self.client.lock().await;
                client
                    .clear(amplifier_module::Empty {})
                    .await
                    .map_err(|e| ContextError::Other {
                        message: format!("gRPC: {}", e),
                    })?;
            }

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn assert_context_trait_object(_: Arc<dyn crate::traits::ContextManager>) {}

    /// Compile-time check: GrpcContextBridge can be wrapped in Arc<dyn ContextManager>.
    #[allow(dead_code)]
    fn grpc_context_bridge_is_context_manager() {
        fn _check(bridge: GrpcContextBridge) {
            assert_context_trait_object(Arc::new(bridge));
        }
    }

    // -- S-1: value_to_proto_message fallback for non-Message values ------------

    /// A plain JSON value that cannot be parsed as a Message falls back to the
    /// text-only encoding with ROLE_UNSPECIFIED and empty ancillary fields.
    #[test]
    fn value_to_proto_message_non_message_value_falls_back_to_text() {
        let val = Value::String("hello".to_string());
        let msg = GrpcContextBridge::value_to_proto_message(&val);
        assert_eq!(msg.role, 0, "fallback role must be ROLE_UNSPECIFIED (0)");
        assert_eq!(msg.name, "", "fallback name must be empty");
        assert_eq!(msg.tool_call_id, "", "fallback tool_call_id must be empty");
        assert_eq!(msg.metadata_json, "", "fallback metadata_json must be empty");
        match msg.content {
            Some(amplifier_module::message::Content::TextContent(text)) => {
                assert_eq!(text, "\"hello\"");
            }
            other => panic!("expected TextContent fallback, got {other:?}"),
        }
    }

    // -- S-2: proto_message_to_value fidelity ----------------------------------

    /// A properly-encoded proto Message (role + TextContent) roundtrips through
    /// proto_message_to_value — role and content are preserved faithfully.
    #[test]
    fn proto_message_to_value_text_content_roundtrip() {
        use crate::messages::{Message, MessageContent, Role};
        use std::collections::HashMap;

        // Build the proto message via native_message_to_proto (same path the bridge uses).
        let native = Message {
            role: Role::User,
            content: MessageContent::Text("hi".into()),
            name: None,
            tool_call_id: None,
            metadata: None,
            extensions: HashMap::new(),
        };
        let proto = crate::generated::conversions::native_message_to_proto(native);
        let val = GrpcContextBridge::proto_message_to_value(&proto);
        assert_eq!(val["role"], "user");
        assert_eq!(val["content"], "hi");
    }

    /// A proto Message with no content (content == None) maps to Value::Null
    /// because proto_message_to_native returns Err for missing content.
    #[test]
    fn proto_message_to_value_none_content_is_null() {
        let msg = amplifier_module::Message {
            role: 0,
            content: None,
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
        };
        assert_eq!(GrpcContextBridge::proto_message_to_value(&msg), Value::Null);
    }

    /// A proto Message with an empty BlockContent list is decoded to a proper
    /// JSON Value — no longer silently dropped as Null.
    #[test]
    fn proto_message_to_value_empty_block_content_is_not_null() {
        let msg = amplifier_module::Message {
            role: amplifier_module::Role::User as i32,
            content: Some(amplifier_module::message::Content::BlockContent(
                amplifier_module::ContentBlockList { blocks: vec![] },
            )),
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
        };
        let val = GrpcContextBridge::proto_message_to_value(&msg);
        assert_ne!(val, Value::Null, "BlockContent must produce a proper Value");
        assert_eq!(val["role"], "user");
        assert_eq!(val["content"], serde_json::json!([]));
    }

    // -- Full-fidelity tests ---------------------------------------------------

    /// value_to_proto_message must preserve role, name, and tool_call_id when
    /// the incoming Value is a well-formed serialised Message.
    #[test]
    fn value_to_proto_message_preserves_role_name_and_tool_call_id() {
        use crate::messages::{Message, MessageContent, Role};
        use std::collections::HashMap;

        let native = Message {
            role: Role::Assistant,
            content: MessageContent::Text("hello".into()),
            name: Some("alice".into()),
            tool_call_id: Some("call_123".into()),
            metadata: None,
            extensions: HashMap::new(),
        };
        let val = serde_json::to_value(&native).expect("serialise Message to Value");
        let proto = GrpcContextBridge::value_to_proto_message(&val);

        // role must NOT be 0 (ROLE_UNSPECIFIED) — it should be Assistant
        assert_ne!(proto.role, 0, "role must not be ROLE_UNSPECIFIED");
        assert_eq!(proto.name, "alice", "name must be preserved");
        assert_eq!(proto.tool_call_id, "call_123", "tool_call_id must be preserved");
    }

    /// proto_message_to_value must produce a proper JSON Value (not Null) when
    /// the proto message carries BlockContent with actual blocks.
    #[test]
    fn proto_message_to_value_block_content_preserved() {
        let msg = amplifier_module::Message {
            role: amplifier_module::Role::Assistant as i32,
            content: Some(amplifier_module::message::Content::BlockContent(
                amplifier_module::ContentBlockList {
                    blocks: vec![amplifier_module::ContentBlock {
                        block: Some(amplifier_module::content_block::Block::TextBlock(
                            amplifier_module::TextBlock {
                                text: "hello from block".into(),
                            },
                        )),
                        visibility: 0,
                    }],
                },
            )),
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
        };
        let val = GrpcContextBridge::proto_message_to_value(&msg);
        assert_ne!(val, Value::Null, "BlockContent must NOT become Null");
        // The role field should be correct
        assert_eq!(val["role"], "assistant");
        // content should be an array with one block
        assert!(val["content"].is_array());
        assert_eq!(val["content"].as_array().unwrap().len(), 1);
    }
}
