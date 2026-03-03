//! gRPC bridge for remote tool modules.
//!
//! [`GrpcToolBridge`] wraps a [`ToolServiceClient`] (gRPC) and implements the
//! native [`Tool`] trait, making a remote tool indistinguishable from a local one.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! use amplifier_core::bridges::grpc_tool::GrpcToolBridge;
//! use amplifier_core::traits::Tool;
//! use std::sync::Arc;
//!
//! let bridge = GrpcToolBridge::connect("http://localhost:50051").await?;
//! let tool: Arc<dyn Tool> = Arc::new(bridge);
//! println!("Connected to tool: {}", tool.name());
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use tonic::transport::Channel;

use crate::errors::ToolError;
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::tool_service_client::ToolServiceClient;
use crate::messages;
use crate::models::ToolResult;
use crate::traits::Tool;

const CONTENT_TYPE_JSON: &str = "application/json";

/// A bridge that wraps a remote gRPC `ToolService` as a native [`Tool`].
///
/// The client is held behind a [`tokio::sync::Mutex`] because
/// `ToolServiceClient` methods take `&mut self` and we need to hold
/// the lock across `.await` points.
pub struct GrpcToolBridge {
    client: tokio::sync::Mutex<ToolServiceClient<Channel>>,
    name: String,
    description: String,
    spec: messages::ToolSpec,
}

impl GrpcToolBridge {
    /// Connect to a remote tool service and discover its spec.
    ///
    /// Calls `ToolServiceClient::connect` followed by `get_spec` to
    /// cache the tool's name, description, and parameter schema.
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut client = ToolServiceClient::connect(endpoint.to_string()).await?;

        let response = client.get_spec(amplifier_module::Empty {}).await?;
        let proto_spec = response.into_inner();

        let name = proto_spec.name.clone();
        let description = proto_spec.description.clone();

        let parameters: HashMap<String, Value> = serde_json::from_str(&proto_spec.parameters_json)
            .unwrap_or_else(|e| {
                if !proto_spec.parameters_json.is_empty() {
                    log::warn!(
                        "Failed to parse tool '{}' parameters_json: {e} — using empty schema",
                        proto_spec.name
                    );
                }
                HashMap::new()
            });

        let spec = messages::ToolSpec {
            name: proto_spec.name,
            parameters,
            description: if proto_spec.description.is_empty() {
                None
            } else {
                Some(proto_spec.description)
            },
            extensions: HashMap::new(),
        };

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
            name,
            description,
            spec,
        })
    }
}

impl Tool for GrpcToolBridge {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn get_spec(&self) -> messages::ToolSpec {
        self.spec.clone()
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<ToolResult, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let json_bytes = serde_json::to_vec(&input).map_err(|e| ToolError::Other {
                message: format!("gRPC call failed: {}", e),
            })?;

            let request = amplifier_module::ToolExecuteRequest {
                input: json_bytes,
                content_type: CONTENT_TYPE_JSON.to_string(),
            };

            let response = {
                let mut client = self.client.lock().await;
                client
                    .execute(request)
                    .await
                    .map_err(|e| ToolError::Other {
                        message: format!("gRPC call failed: {}", e),
                    })?
            };

            let resp = response.into_inner();

            if !resp.content_type.is_empty() && resp.content_type != CONTENT_TYPE_JSON {
                log::warn!(
                    "Tool response has content_type '{}' but only '{}' is supported — parsing as JSON anyway",
                    resp.content_type, CONTENT_TYPE_JSON
                );
            }

            let output = if resp.output.is_empty() {
                None
            } else {
                serde_json::from_slice(&resp.output)
                    .map_err(|e| {
                        log::warn!("Failed to parse tool '{}' output JSON: {e}", self.name);
                        e
                    })
                    .ok()
            };

            let error = if resp.error.is_empty() {
                None
            } else {
                Some(HashMap::from([(
                    "message".to_string(),
                    Value::String(resp.error),
                )]))
            };

            Ok(ToolResult {
                success: resp.success,
                output,
                error,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[allow(dead_code)]
    fn assert_tool_trait_object(_: Arc<dyn crate::traits::Tool>) {}

    /// Compile-time check: GrpcToolBridge can be wrapped in Arc<dyn Tool>.
    #[allow(dead_code)]
    fn grpc_tool_bridge_is_tool() {
        // This function is never called, but if it compiles, the trait is satisfied.
        fn _check(bridge: GrpcToolBridge) {
            assert_tool_trait_object(Arc::new(bridge));
        }
    }
}
