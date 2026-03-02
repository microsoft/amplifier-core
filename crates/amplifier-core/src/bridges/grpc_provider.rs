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

        let defaults: HashMap<String, Value> =
            serde_json::from_str(&proto_info.defaults_json).unwrap_or_default();

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
                    })?
            };

            let proto_models = response.into_inner().models;

            let models = proto_models
                .into_iter()
                .map(|m| {
                    let defaults: HashMap<String, Value> =
                        serde_json::from_str(&m.defaults_json).unwrap_or_default();
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
        _request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, ProviderError>> + Send + '_>> {
        Box::pin(async move {
            // Phase 2 stub: Message ↔ proto::Message and ContentBlock ↔
            // proto::ContentBlock conversions are not yet implemented.
            // Fail loudly so callers know this bridge cannot complete yet.
            // Full conversion will land in Phase 4 (Task 21).
            Err(ProviderError::Other {
                message: "GrpcProviderBridge::complete() not yet implemented: \
                          Message/ContentBlock conversion requires Phase 4"
                    .into(),
                provider: Some(self.name.clone()),
                model: None,
                retry_after: None,
                status_code: None,
                retryable: false,
            })
        })
    }

    fn parse_tool_calls(&self, response: &ChatResponse) -> Vec<ToolCall> {
        response.tool_calls.clone().unwrap_or_default()
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
}
