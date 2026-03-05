//! gRPC bridge for remote provider modules.
//!
//! [`GrpcProviderBridge`] wraps a [`ProviderServiceClient`] (gRPC) and implements
//! the native [`Provider`] trait, making a remote provider indistinguishable from
//! a local one.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! use amplifier_core::bridges::grpc_provider::GrpcProviderBridge;
//! use amplifier_core::traits::Provider;
//! use std::sync::Arc;
//!
//! let bridge = GrpcProviderBridge::connect("http://localhost:50051").await?;
//! let provider: Arc<dyn Provider> = Arc::new(bridge);
//! println!("Connected to provider: {}", provider.name());
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use tonic::transport::Channel;

use crate::errors::ProviderError;
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::provider_service_client::ProviderServiceClient;
use crate::messages::{ChatRequest, ChatResponse, ToolCall};
use crate::models::{ModelInfo, ProviderInfo};
use crate::traits::Provider;

/// Parse a JSON string into a defaults `HashMap`, logging a warning on non-empty
/// parse failures.
fn parse_defaults_json(json_str: &str, id: &str) -> HashMap<String, Value> {
    serde_json::from_str(json_str).unwrap_or_else(|e| {
        if !json_str.is_empty() {
            log::warn!(
                "Failed to parse provider '{}' defaults_json: {e} — using empty defaults",
                id
            );
        }
        HashMap::new()
    })
}

/// A bridge that wraps a remote gRPC `ProviderService` as a native [`Provider`].
///
/// The client is held behind a [`tokio::sync::Mutex`] because
/// `ProviderServiceClient` methods take `&mut self` and we need to hold
/// the lock across `.await` points.
pub struct GrpcProviderBridge {
    client: tokio::sync::Mutex<ProviderServiceClient<Channel>>,
    name: String,
    info: ProviderInfo,
}

impl GrpcProviderBridge {
    /// Connect to a remote provider service and discover its metadata.
    ///
    /// Calls `ProviderServiceClient::connect` followed by `get_info` to
    /// cache the provider's name, capabilities, and defaults.
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut client = ProviderServiceClient::connect(endpoint.to_string()).await?;

        let response = client.get_info(amplifier_module::Empty {}).await?;
        let proto_info = response.into_inner();

        let name = proto_info.id.clone();

        let defaults = parse_defaults_json(&proto_info.defaults_json, &proto_info.id);

        let info = ProviderInfo {
            id: proto_info.id,
            display_name: proto_info.display_name,
            credential_env_vars: proto_info.credential_env_vars,
            capabilities: proto_info.capabilities,
            defaults,
            config_fields: vec![], // Full ConfigField conversion in Phase 4
        };

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
            name,
            info,
        })
    }
}

impl Provider for GrpcProviderBridge {
    fn name(&self) -> &str {
        &self.name
    }

    fn get_info(&self) -> ProviderInfo {
        self.info.clone()
    }

    fn list_models(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send + '_>> {
        Box::pin(async move {
            let response = {
                let mut client = self.client.lock().await;
                client
                    .list_models(amplifier_module::Empty {})
                    .await
                    .map_err(|e| ProviderError::Other {
                        message: format!("gRPC call failed: {}", e),
                        provider: Some(self.name.clone()),
                        model: None,
                        retry_after: None,
                        status_code: None,
                        retryable: false,
                        delay_multiplier: None,
                    })?
            };

            let proto_models = response.into_inner().models;

            let models = proto_models
                .into_iter()
                .map(|m| {
                    let defaults = parse_defaults_json(&m.defaults_json, &m.id);
                    ModelInfo {
                        id: m.id,
                        display_name: m.display_name,
                        context_window: m.context_window as i64,
                        max_output_tokens: m.max_output_tokens as i64,
                        capabilities: m.capabilities,
                        defaults,
                    }
                })
                .collect();

            Ok(models)
        })
    }

    fn complete(
        &self,
        request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, ProviderError>> + Send + '_>> {
        Box::pin(async move {
            let proto_request =
                crate::generated::conversions::native_chat_request_to_proto(&request);

            let response = {
                let mut client = self.client.lock().await;
                client
                    .complete(proto_request)
                    .await
                    .map_err(|e| ProviderError::Other {
                        message: format!("gRPC call failed: {e}"),
                        provider: Some(self.name.clone()),
                        model: None,
                        retry_after: None,
                        status_code: None,
                        retryable: false,
                        delay_multiplier: None,
                    })?
            };

            let native_response =
                crate::generated::conversions::proto_chat_response_to_native(
                    response.into_inner(),
                );

            Ok(native_response)
        })
    }

    fn parse_tool_calls(&self, response: &ChatResponse) -> Vec<ToolCall> {
        response.tool_calls.clone().unwrap_or_default()
    }
}

impl GrpcProviderBridge {
    /// Test-only constructor: build a bridge from a pre-built client without
    /// going through `connect()` (which would require a live gRPC server).
    #[cfg(test)]
    fn new_for_testing(client: ProviderServiceClient<Channel>, name: String) -> Self {
        use crate::models::ProviderInfo;
        Self {
            client: tokio::sync::Mutex::new(client),
            name,
            info: ProviderInfo {
                id: "test-provider".into(),
                display_name: "Test Provider".into(),
                credential_env_vars: vec![],
                capabilities: vec![],
                defaults: Default::default(),
                config_fields: vec![],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[allow(dead_code)]
    fn assert_provider_trait_object(_: Arc<dyn crate::traits::Provider>) {}

    /// Compile-time check: GrpcProviderBridge can be wrapped in Arc<dyn Provider>.
    #[allow(dead_code)]
    fn grpc_provider_bridge_is_provider() {
        // This function is never called, but if it compiles, the trait is satisfied.
        fn _check(bridge: GrpcProviderBridge) {
            assert_provider_trait_object(Arc::new(bridge));
        }
    }

    #[test]
    fn parse_defaults_json_valid() {
        let result = parse_defaults_json(r#"{"temperature": 0.7}"#, "test-provider");
        assert_eq!(result.get("temperature"), Some(&serde_json::json!(0.7)));
    }

    #[test]
    fn parse_defaults_json_empty_string_returns_empty() {
        let result = parse_defaults_json("", "test-provider");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_defaults_json_invalid_nonempty_returns_empty() {
        // Invalid non-empty JSON should return empty HashMap (and log a warning).
        let result = parse_defaults_json("not-valid-json", "test-provider");
        assert!(result.is_empty());
    }

    /// RED test: verifies that `complete()` actually attempts a gRPC call
    /// rather than returning the Phase-2 "not yet implemented" stub error.
    ///
    /// The bridge is pointed at a non-existent server so the call will fail
    /// with a transport/connection error — NOT the old stub message.
    ///
    /// Before the fix: returns `ProviderError::Other { message: "… not yet
    /// implemented …" }` → assertion fails (RED).
    /// After the fix: returns a gRPC transport error → assertion passes (GREEN).
    #[tokio::test]
    async fn complete_attempts_grpc_call_not_stub() {
        use crate::messages::{ChatRequest, Message, MessageContent, Role};
        use std::collections::HashMap;

        // Create a lazy channel to a port that has nothing listening.
        // `connect_lazy()` defers the actual TCP connection until the first
        // RPC, so creating the channel never blocks or fails.
        let channel = tonic::transport::Channel::from_static("http://[::1]:50099")
            .connect_lazy();
        let client = ProviderServiceClient::new(channel);
        let bridge = GrpcProviderBridge::new_for_testing(client, "test-provider".into());

        let request = ChatRequest {
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

        let result = bridge.complete(request).await;

        // The stub returned exactly this message — after the fix the bridge
        // must attempt a real RPC and return a connection/transport error.
        match &result {
            Err(ProviderError::Other { message, .. }) => {
                assert!(
                    !message.contains("not yet implemented"),
                    "complete() returned the old stub error instead of attempting a gRPC \
                     call. Got: {message}"
                );
            }
            Err(_) => {
                // Any other ProviderError variant means a real attempt was made.
            }
            Ok(_) => {
                // Succeeding would also be fine (highly unlikely with no server).
            }
        }
    }
}
