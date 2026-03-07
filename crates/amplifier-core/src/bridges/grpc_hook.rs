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
    pub(crate) fn proto_to_native_hook_result(
        proto: amplifier_module::HookResult,
    ) -> models::HookResult {
        let action = match amplifier_module::HookAction::try_from(proto.action) {
            Ok(amplifier_module::HookAction::Continue) => models::HookAction::Continue,
            Ok(amplifier_module::HookAction::Modify) => models::HookAction::Modify,
            Ok(amplifier_module::HookAction::Deny) => models::HookAction::Deny,
            Ok(amplifier_module::HookAction::InjectContext) => models::HookAction::InjectContext,
            Ok(amplifier_module::HookAction::AskUser) => models::HookAction::AskUser,
            Ok(amplifier_module::HookAction::Unspecified) | Err(_) => {
                if proto.action != 0 {
                    log::warn!(
                        "Unknown hook action variant {}, defaulting to Continue",
                        proto.action
                    );
                }
                models::HookAction::Continue
            }
        };

        let data = serde_json::from_str(&proto.data_json)
            .map_err(|e| {
                if !proto.data_json.is_empty() {
                    log::warn!("Failed to parse hook result data_json: {e}");
                }
                e
            })
            .ok();

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

        let context_injection_role =
            match amplifier_module::ContextInjectionRole::try_from(proto.context_injection_role) {
                Ok(amplifier_module::ContextInjectionRole::System) => {
                    models::ContextInjectionRole::System
                }
                Ok(amplifier_module::ContextInjectionRole::User) => {
                    models::ContextInjectionRole::User
                }
                Ok(amplifier_module::ContextInjectionRole::Assistant) => {
                    models::ContextInjectionRole::Assistant
                }
                Ok(amplifier_module::ContextInjectionRole::Unspecified) | Err(_) => {
                    if proto.context_injection_role != 0 {
                        log::warn!(
                            "Unknown context injection role variant {}, defaulting to System",
                            proto.context_injection_role
                        );
                    }
                    models::ContextInjectionRole::System
                }
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

        let approval_default =
            match amplifier_module::ApprovalDefault::try_from(proto.approval_default) {
                Ok(amplifier_module::ApprovalDefault::Approve) => models::ApprovalDefault::Allow,
                Ok(amplifier_module::ApprovalDefault::Deny) => models::ApprovalDefault::Deny,
                Ok(amplifier_module::ApprovalDefault::Unspecified) | Err(_) => {
                    if proto.approval_default != 0 {
                        log::warn!(
                            "Unknown approval default variant {}, defaulting to Deny",
                            proto.approval_default
                        );
                    }
                    models::ApprovalDefault::Deny
                }
            };

        let user_message = if proto.user_message.is_empty() {
            None
        } else {
            Some(proto.user_message)
        };

        let user_message_level =
            match amplifier_module::UserMessageLevel::try_from(proto.user_message_level) {
                Ok(amplifier_module::UserMessageLevel::Info) => models::UserMessageLevel::Info,
                Ok(amplifier_module::UserMessageLevel::Warning) => {
                    models::UserMessageLevel::Warning
                }
                Ok(amplifier_module::UserMessageLevel::Error) => models::UserMessageLevel::Error,
                Ok(amplifier_module::UserMessageLevel::Unspecified) | Err(_) => {
                    if proto.user_message_level != 0 {
                        log::warn!(
                            "Unknown user message level variant {}, defaulting to Info",
                            proto.user_message_level
                        );
                    }
                    models::UserMessageLevel::Info
                }
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
            approval_timeout: proto.approval_timeout.unwrap_or(300.0),
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

    /// Helper to build a default proto HookResult for testing.
    fn default_proto_hook_result() -> amplifier_module::HookResult {
        amplifier_module::HookResult {
            action: 0,
            data_json: String::new(),
            reason: String::new(),
            context_injection: String::new(),
            context_injection_role: 0,
            ephemeral: false,
            approval_prompt: String::new(),
            approval_options: vec![],
            approval_timeout: None,
            approval_default: 0,
            suppress_output: false,
            user_message: String::new(),
            user_message_level: 0,
            user_message_source: String::new(),
            append_to_last_tool_result: false,
        }
    }

    // ---- E-1: Typed enum matching via try_from() ----

    #[test]
    fn hook_action_known_variants_map_correctly() {
        // HookAction::Continue = 1
        let mut proto = default_proto_hook_result();
        proto.action = amplifier_module::HookAction::Continue as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.action, models::HookAction::Continue);

        // HookAction::Modify = 2
        let mut proto = default_proto_hook_result();
        proto.action = amplifier_module::HookAction::Modify as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.action, models::HookAction::Modify);

        // HookAction::Deny = 3
        let mut proto = default_proto_hook_result();
        proto.action = amplifier_module::HookAction::Deny as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.action, models::HookAction::Deny);

        // HookAction::InjectContext = 4
        let mut proto = default_proto_hook_result();
        proto.action = amplifier_module::HookAction::InjectContext as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.action, models::HookAction::InjectContext);

        // HookAction::AskUser = 5
        let mut proto = default_proto_hook_result();
        proto.action = amplifier_module::HookAction::AskUser as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.action, models::HookAction::AskUser);
    }

    #[test]
    fn hook_action_unspecified_defaults_to_continue() {
        // Unspecified (0) should default to Continue
        let proto = default_proto_hook_result(); // action = 0
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.action, models::HookAction::Continue);
    }

    #[test]
    fn hook_action_unknown_defaults_to_continue() {
        // Unknown value (99) should default to Continue
        let mut proto = default_proto_hook_result();
        proto.action = 99;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.action, models::HookAction::Continue);
    }

    #[test]
    fn context_injection_role_known_variants_map_correctly() {
        let mut proto = default_proto_hook_result();
        proto.context_injection_role = amplifier_module::ContextInjectionRole::System as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(
            result.context_injection_role,
            models::ContextInjectionRole::System
        );

        let mut proto = default_proto_hook_result();
        proto.context_injection_role = amplifier_module::ContextInjectionRole::User as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(
            result.context_injection_role,
            models::ContextInjectionRole::User
        );

        let mut proto = default_proto_hook_result();
        proto.context_injection_role = amplifier_module::ContextInjectionRole::Assistant as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(
            result.context_injection_role,
            models::ContextInjectionRole::Assistant
        );
    }

    #[test]
    fn context_injection_role_unknown_defaults_to_system() {
        let mut proto = default_proto_hook_result();
        proto.context_injection_role = 99;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(
            result.context_injection_role,
            models::ContextInjectionRole::System
        );
    }

    #[test]
    fn approval_default_known_variants_map_correctly() {
        let mut proto = default_proto_hook_result();
        proto.approval_default = amplifier_module::ApprovalDefault::Approve as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.approval_default, models::ApprovalDefault::Allow);

        let mut proto = default_proto_hook_result();
        proto.approval_default = amplifier_module::ApprovalDefault::Deny as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.approval_default, models::ApprovalDefault::Deny);
    }

    #[test]
    fn approval_default_unknown_defaults_to_deny() {
        let mut proto = default_proto_hook_result();
        proto.approval_default = 99;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.approval_default, models::ApprovalDefault::Deny);
    }

    #[test]
    fn user_message_level_known_variants_map_correctly() {
        let mut proto = default_proto_hook_result();
        proto.user_message_level = amplifier_module::UserMessageLevel::Info as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.user_message_level, models::UserMessageLevel::Info);

        let mut proto = default_proto_hook_result();
        proto.user_message_level = amplifier_module::UserMessageLevel::Warning as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.user_message_level, models::UserMessageLevel::Warning);

        let mut proto = default_proto_hook_result();
        proto.user_message_level = amplifier_module::UserMessageLevel::Error as i32;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.user_message_level, models::UserMessageLevel::Error);
    }

    #[test]
    fn user_message_level_unknown_defaults_to_info() {
        let mut proto = default_proto_hook_result();
        proto.user_message_level = 99;
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.user_message_level, models::UserMessageLevel::Info);
    }

    // ---- P1-11: data_json parse failure logging ----

    #[test]
    fn data_json_valid_json_parses_correctly() {
        let mut proto = default_proto_hook_result();
        proto.data_json = r#"{"key": "value"}"#.to_string();
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        let expected: HashMap<String, Value> = serde_json::from_str(r#"{"key": "value"}"#).unwrap();
        assert_eq!(result.data, Some(expected));
    }

    #[test]
    fn data_json_empty_string_returns_none() {
        let proto = default_proto_hook_result(); // data_json = ""
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        assert_eq!(result.data, None);
    }

    #[test]
    fn data_json_invalid_json_returns_none() {
        let mut proto = default_proto_hook_result();
        proto.data_json = "not valid json{".to_string();
        let result = GrpcHookBridge::proto_to_native_hook_result(proto);
        // Should return None (parse failure logged but still returns None)
        assert_eq!(result.data, None);
    }
}
