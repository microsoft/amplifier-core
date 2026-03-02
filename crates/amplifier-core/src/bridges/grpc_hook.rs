//! gRPC bridge for remote hook modules.
//!
//! [`GrpcHookBridge`] wraps a [`HookServiceClient`] (gRPC) and implements
//! the native [`HookHandler`] trait, making a remote hook handler
//! indistinguishable from a local one.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! use amplifier_core::bridges::grpc_hook::GrpcHookBridge;
//! use amplifier_core::traits::HookHandler;
//! use std::sync::Arc;
//!
//! let bridge = GrpcHookBridge::connect("http://localhost:50051").await?;
//! let hook: Arc<dyn HookHandler> = Arc::new(bridge);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use tonic::transport::Channel;

use crate::errors::HookError;
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::hook_service_client::HookServiceClient;
use crate::models;
use crate::traits::HookHandler;

/// A bridge that wraps a remote gRPC `HookService` as a native [`HookHandler`].
///
/// The client is held behind a [`tokio::sync::Mutex`] because
/// `HookServiceClient` methods take `&mut self` and we need to hold
/// the lock across `.await` points.
pub struct GrpcHookBridge {
    client: tokio::sync::Mutex<HookServiceClient<Channel>>,
}

impl GrpcHookBridge {
    /// Connect to a remote hook service.
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = HookServiceClient::connect(endpoint.to_string()).await?;

        Ok(Self {
            client: tokio::sync::Mutex::new(client),
        })
    }

    /// Convert a proto `HookResult` to a native [`models::HookResult`].
    fn proto_to_native_hook_result(proto: amplifier_module::HookResult) -> models::HookResult {
        let action = match proto.action {
            1 => models::HookAction::Continue,
            2 => models::HookAction::Modify,
            3 => models::HookAction::Deny,
            4 => models::HookAction::InjectContext,
            5 => models::HookAction::AskUser,
            _ => models::HookAction::Continue,
        };

        let data = serde_json::from_str(&proto.data_json).ok();

        let reason = if proto.reason.is_empty() {
            None
        } else {
            Some(proto.reason)
        };

        let context_injection = if proto.context_injection.is_empty() {
            None
        } else {
            Some(proto.context_injection)
        };

        let context_injection_role = match proto.context_injection_role {
            1 => models::ContextInjectionRole::System,
            2 => models::ContextInjectionRole::User,
            3 => models::ContextInjectionRole::Assistant,
            _ => models::ContextInjectionRole::System,
        };

        let approval_prompt = if proto.approval_prompt.is_empty() {
            None
        } else {
            Some(proto.approval_prompt)
        };

        let approval_options = if proto.approval_options.is_empty() {
            None
        } else {
            Some(proto.approval_options)
        };

        let approval_default = match proto.approval_default {
            1 => models::ApprovalDefault::Allow,
            2 => models::ApprovalDefault::Deny,
            _ => models::ApprovalDefault::Deny,
        };

        let user_message = if proto.user_message.is_empty() {
            None
        } else {
            Some(proto.user_message)
        };

        let user_message_level = match proto.user_message_level {
            1 => models::UserMessageLevel::Info,
            2 => models::UserMessageLevel::Warning,
            3 => models::UserMessageLevel::Error,
            _ => models::UserMessageLevel::Info,
        };

        let user_message_source = if proto.user_message_source.is_empty() {
            None
        } else {
            Some(proto.user_message_source)
        };

        models::HookResult {
            action,
            data,
            reason,
            context_injection,
            context_injection_role,
            ephemeral: proto.ephemeral,
            approval_prompt,
            approval_options,
            approval_timeout: proto.approval_timeout,
            approval_default,
            suppress_output: proto.suppress_output,
            user_message,
            user_message_level,
            user_message_source,
            append_to_last_tool_result: proto.append_to_last_tool_result,
            extensions: HashMap::new(),
        }
    }
}

impl HookHandler for GrpcHookBridge {
    fn handle(
        &self,
        event: &str,
        data: Value,
    ) -> Pin<Box<dyn Future<Output = Result<models::HookResult, HookError>> + Send + '_>> {
        let event = event.to_string();
        Box::pin(async move {
            let data_json = serde_json::to_string(&data).map_err(|e| HookError::Other {
                message: format!("gRPC: {}", e),
            })?;

            let request = amplifier_module::HookHandleRequest { event, data_json };

            let response = {
                let mut client = self.client.lock().await;
                client.handle(request).await.map_err(|e| HookError::Other {
                    message: format!("gRPC: {}", e),
                })?
            };

            let proto_result = response.into_inner();
            Ok(Self::proto_to_native_hook_result(proto_result))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[allow(dead_code)]
    fn assert_hook_trait_object(_: Arc<dyn crate::traits::HookHandler>) {}

    /// Compile-time check: GrpcHookBridge can be wrapped in Arc<dyn HookHandler>.
    #[allow(dead_code)]
    fn grpc_hook_bridge_is_hook_handler() {
        fn _check(bridge: GrpcHookBridge) {
            assert_hook_trait_object(Arc::new(bridge));
        }
    }
}
