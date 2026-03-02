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

    /// Convert a native `Value` message to a proto `Message`.
    ///
    /// Phase 2 simplified conversion: serialize the Value to a JSON string
    /// and store it as `text_content` on a proto Message.
    fn value_to_proto_message(message: &Value) -> amplifier_module::Message {
        let json_string = serde_json::to_string(message).unwrap_or_default();
        amplifier_module::Message {
            role: 0, // ROLE_UNSPECIFIED
            content: Some(amplifier_module::message::Content::TextContent(json_string)),
            name: String::new(),
            tool_call_id: String::new(),
            metadata_json: String::new(),
        }
    }

    /// Convert a proto `Message` to a native `Value`.
    ///
    /// Phase 2 simplified conversion: extract the `text_content` string
    /// and parse it back as a JSON Value.
    fn proto_message_to_value(msg: &amplifier_module::Message) -> Value {
        match &msg.content {
            Some(amplifier_module::message::Content::TextContent(text)) => {
                serde_json::from_str(text).unwrap_or(Value::String(text.clone()))
            }
            _ => Value::Null,
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
            let request = amplifier_module::GetMessagesForRequestParams {
                token_budget: token_budget.unwrap_or(0) as i32,
                provider_name: String::new(),
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
}
