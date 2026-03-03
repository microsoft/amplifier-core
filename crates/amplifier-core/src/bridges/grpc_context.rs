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

    // TODO(grpc-v2): Message fields (role, name, tool_call_id, metadata) are not yet
    // transmitted through the gRPC bridge. The native Value may contain these fields
    // but they are zeroed in the proto message. Full Message conversion requires
    // proto schema updates (Phase 4).
    fn value_to_proto_message(message: &Value) -> amplifier_module::Message {
        log::debug!(
            "Converting Value to proto Message — role, name, tool_call_id, metadata_json are not yet transmitted"
        );
        let json_string = serde_json::to_string(message).unwrap_or_else(|e| {
            log::warn!("Failed to serialize context message to JSON: {e} — using empty string");
            String::new()
        });
        amplifier_module::Message {
            role: 0, // ROLE_UNSPECIFIED — TODO(grpc-v2): map from native message role
            content: Some(amplifier_module::message::Content::TextContent(json_string)),
            name: String::new(), // TODO(grpc-v2): extract from native message
            tool_call_id: String::new(), // TODO(grpc-v2): extract from native message
            metadata_json: String::new(), // TODO(grpc-v2): extract from native message
        }
    }

    // TODO(grpc-v2): Only TextContent is handled. BlockContent and other variants
    // are mapped to Null, losing data. Full ContentBlock conversion requires Phase 4.
    fn proto_message_to_value(msg: &amplifier_module::Message) -> Value {
        match &msg.content {
            Some(amplifier_module::message::Content::TextContent(text)) => {
                serde_json::from_str(text).unwrap_or(Value::String(text.clone()))
            }
            Some(_other) => {
                log::debug!(
                    "Non-TextContent message variant encountered — mapping to Null (not yet supported)"
                );
                Value::Null
            }
            None => Value::Null,
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
        _provider: Option<Arc<dyn Provider>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>, ContextError>> + Send + '_>> {
        Box::pin(async move {
            // TODO(grpc-v2): provider_name parameter is not transmitted to the remote
            // context manager. The _provider parameter is accepted but unused.
            log::debug!(
                "get_messages_for_request: provider_name is not transmitted through gRPC bridge"
            );
            let request = amplifier_module::GetMessagesForRequestParams {
                token_budget: token_budget.unwrap_or(0) as i32,
                provider_name: String::new(), // TODO(grpc-v2): extract from _provider param
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

    // ── S-1 regression: value_to_proto_message structural gaps ─────────────

    /// value_to_proto_message stores JSON as TextContent and zeroes all other fields.
    #[test]
    fn value_to_proto_message_text_content_and_zeroed_fields() {
        let val = Value::String("hello".to_string());
        let msg = GrpcContextBridge::value_to_proto_message(&val);
        assert_eq!(msg.role, 0, "role should be ROLE_UNSPECIFIED (0)");
        assert_eq!(msg.name, "", "name should be empty");
        assert_eq!(msg.tool_call_id, "", "tool_call_id should be empty");
        assert_eq!(msg.metadata_json, "", "metadata_json should be empty");
        match msg.content {
            Some(amplifier_module::message::Content::TextContent(text)) => {
                assert_eq!(text, "\"hello\"");
            }
            other => panic!("expected TextContent, got {other:?}"),
        }
    }

    // ── S-2 regression: proto_message_to_value structural gaps ─────────────

    /// TextContent round-trips through proto_message_to_value correctly.
    #[test]
    fn proto_message_to_value_text_content_roundtrip() {
        let json = r#"{"role":"user","content":"hi"}"#;
        let msg = amplifier_module::Message {
            role: 0,
            content: Some(amplifier_module::message::Content::TextContent(
                json.to_string(),
            )),
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
        };
        let val = GrpcContextBridge::proto_message_to_value(&msg);
        assert_eq!(val["role"], "user");
        assert_eq!(val["content"], "hi");
    }

    /// None content maps to Value::Null.
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

    /// BlockContent (non-TextContent variant) maps to Value::Null — data loss documented
    /// by TODO(grpc-v2) in the implementation.
    #[test]
    fn proto_message_to_value_block_content_is_null() {
        let msg = amplifier_module::Message {
            role: 0,
            content: Some(amplifier_module::message::Content::BlockContent(
                amplifier_module::ContentBlockList { blocks: vec![] },
            )),
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
        };
        assert_eq!(
            GrpcContextBridge::proto_message_to_value(&msg),
            Value::Null,
            "BlockContent must map to Null until grpc-v2 phase"
        );
    }
}
