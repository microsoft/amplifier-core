//! KernelService gRPC server implementation.
//!
//! This module implements the KernelService proto as a tonic gRPC server.
//! Out-of-process modules (Go, TypeScript, etc.) call back to the kernel
//! via this service for provider/tool/hook/context access.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::coordinator::Coordinator;
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::kernel_service_server::KernelService;
use crate::generated::conversions::{
    native_hook_result_to_proto, native_message_to_proto, proto_message_to_native,
};

/// Implementation of the KernelService gRPC server.
///
/// Wraps an `Arc<Coordinator>` and translates proto requests into
/// coordinator operations.
pub struct KernelServiceImpl {
    coordinator: Arc<Coordinator>,
}

impl KernelServiceImpl {
    /// Create a new KernelServiceImpl wrapping the given coordinator.
    pub fn new(coordinator: Arc<Coordinator>) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl KernelService for KernelServiceImpl {
    async fn complete_with_provider(
        &self,
        _request: Request<amplifier_module::CompleteWithProviderRequest>,
    ) -> Result<Response<amplifier_module::ChatResponse>, Status> {
        Err(Status::unimplemented(
            "CompleteWithProvider not yet implemented",
        ))
    }

    type CompleteWithProviderStreamingStream =
        tokio_stream::wrappers::ReceiverStream<Result<amplifier_module::ChatResponse, Status>>;

    async fn complete_with_provider_streaming(
        &self,
        _request: Request<amplifier_module::CompleteWithProviderRequest>,
    ) -> Result<Response<Self::CompleteWithProviderStreamingStream>, Status> {
        Err(Status::unimplemented(
            "CompleteWithProviderStreaming not yet implemented",
        ))
    }

    async fn execute_tool(
        &self,
        request: Request<amplifier_module::ExecuteToolRequest>,
    ) -> Result<Response<amplifier_module::ToolResult>, Status> {
        let req = request.into_inner();
        let tool_name = &req.tool_name;

        // Look up the tool in the coordinator
        let tool = self
            .coordinator
            .get_tool(tool_name)
            .ok_or_else(|| Status::not_found(format!("Tool not found: {tool_name}")))?;

        // Parse input JSON
        let input: serde_json::Value = serde_json::from_str(&req.input_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid input JSON: {e}")))?;

        // Execute the tool
        match tool.execute(input).await {
            Ok(result) => {
                let output_json = result
                    .output
                    .map(|v| {
                        serde_json::to_string(&v).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize tool result output to JSON: {e}");
                            String::new()
                        })
                    })
                    .unwrap_or_default();
                let error_json = result
                    .error
                    .map(|e| {
                        serde_json::to_string(&e).unwrap_or_else(|ser_err| {
                            log::warn!("Failed to serialize tool result error to JSON: {ser_err}");
                            String::new()
                        })
                    })
                    .unwrap_or_default();
                Ok(Response::new(amplifier_module::ToolResult {
                    success: result.success,
                    output_json,
                    error_json,
                }))
            }
            Err(e) => Err(Status::internal(format!("Tool execution failed: {e}"))),
        }
    }

    async fn emit_hook(
        &self,
        request: Request<amplifier_module::EmitHookRequest>,
    ) -> Result<Response<amplifier_module::HookResult>, Status> {
        let req = request.into_inner();

        let data: serde_json::Value = if req.data_json.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&req.data_json)
                .map_err(|e| Status::invalid_argument(format!("Invalid data_json: {e}")))?
        };

        let result = self.coordinator.hooks().emit(&req.event, data).await;
        let proto_result = native_hook_result_to_proto(&result);
        Ok(Response::new(proto_result))
    }

    async fn emit_hook_and_collect(
        &self,
        request: Request<amplifier_module::EmitHookAndCollectRequest>,
    ) -> Result<Response<amplifier_module::EmitHookAndCollectResponse>, Status> {
        let req = request.into_inner();

        let data: serde_json::Value = if req.data_json.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&req.data_json)
                .map_err(|e| Status::invalid_argument(format!("Invalid data_json: {e}")))?
        };

        let timeout = if req.timeout_seconds > 0.0 {
            std::time::Duration::from_secs_f64(req.timeout_seconds)
        } else {
            std::time::Duration::from_secs(30)
        };

        let results = self
            .coordinator
            .hooks()
            .emit_and_collect(&req.event, data, timeout)
            .await;

        let responses_json: Vec<String> = results
            .iter()
            .map(|map| {
                serde_json::to_string(map).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize hook collect result to JSON: {e}");
                    String::new()
                })
            })
            .collect();

        Ok(Response::new(amplifier_module::EmitHookAndCollectResponse {
            responses_json,
        }))
    }

    async fn get_messages(
        &self,
        _request: Request<amplifier_module::GetMessagesRequest>,
    ) -> Result<Response<amplifier_module::GetMessagesResponse>, Status> {
        let context = self
            .coordinator
            .context()
            .ok_or_else(|| Status::failed_precondition("No context manager mounted"))?;

        let values = context
            .get_messages()
            .await
            .map_err(|e| Status::internal(format!("Failed to get messages: {e}")))?;

        let messages: Vec<amplifier_module::Message> = values
            .into_iter()
            .filter_map(|v| {
                serde_json::from_value::<crate::messages::Message>(v)
                    .map(native_message_to_proto)
                    .map_err(|e| {
                        log::warn!("Skipping message that failed to deserialize: {e}");
                        e
                    })
                    .ok()
            })
            .collect();

        Ok(Response::new(amplifier_module::GetMessagesResponse {
            messages,
        }))
    }

    async fn add_message(
        &self,
        request: Request<amplifier_module::KernelAddMessageRequest>,
    ) -> Result<Response<amplifier_module::Empty>, Status> {
        let req = request.into_inner();

        let proto_message = req
            .message
            .ok_or_else(|| Status::invalid_argument("Missing required field: message"))?;

        let native_message = proto_message_to_native(proto_message)
            .map_err(|e| Status::invalid_argument(format!("Invalid message: {e}")))?;

        let value = serde_json::to_value(native_message)
            .map_err(|e| Status::internal(format!("Failed to serialize message: {e}")))?;

        let context = self
            .coordinator
            .context()
            .ok_or_else(|| Status::failed_precondition("No context manager mounted"))?;

        context
            .add_message(value)
            .await
            .map_err(|e| Status::internal(format!("Failed to add message: {e}")))?;

        Ok(Response::new(amplifier_module::Empty {}))
    }

    async fn get_mounted_module(
        &self,
        request: Request<amplifier_module::GetMountedModuleRequest>,
    ) -> Result<Response<amplifier_module::GetMountedModuleResponse>, Status> {
        let req = request.into_inner();
        let module_name = &req.module_name;
        let module_type = amplifier_module::ModuleType::try_from(req.module_type)
            .unwrap_or(amplifier_module::ModuleType::Unspecified);

        let found_info: Option<amplifier_module::ModuleInfo> = match module_type {
            amplifier_module::ModuleType::Tool => {
                self.coordinator.get_tool(module_name).map(|tool| {
                    amplifier_module::ModuleInfo {
                        name: tool.name().to_string(),
                        module_type: amplifier_module::ModuleType::Tool as i32,
                        ..Default::default()
                    }
                })
            }
            amplifier_module::ModuleType::Provider => {
                self.coordinator.get_provider(module_name).map(|provider| {
                    amplifier_module::ModuleInfo {
                        name: provider.name().to_string(),
                        module_type: amplifier_module::ModuleType::Provider as i32,
                        ..Default::default()
                    }
                })
            }
            amplifier_module::ModuleType::Unspecified => {
                // Search tools first, then providers
                if let Some(tool) = self.coordinator.get_tool(module_name) {
                    Some(amplifier_module::ModuleInfo {
                        name: tool.name().to_string(),
                        module_type: amplifier_module::ModuleType::Tool as i32,
                        ..Default::default()
                    })
                } else {
                    self.coordinator.get_provider(module_name).map(|provider| {
                        amplifier_module::ModuleInfo {
                            name: provider.name().to_string(),
                            module_type: amplifier_module::ModuleType::Provider as i32,
                            ..Default::default()
                        }
                    })
                }
            }
            // Hook, Memory, Guardrail, Approval — not yet stored by name in Coordinator
            _ => None,
        };

        match found_info {
            Some(info) => Ok(Response::new(amplifier_module::GetMountedModuleResponse {
                found: true,
                info: Some(info),
            })),
            None => Ok(Response::new(amplifier_module::GetMountedModuleResponse {
                found: false,
                info: None,
            })),
        }
    }

    async fn register_capability(
        &self,
        request: Request<amplifier_module::RegisterCapabilityRequest>,
    ) -> Result<Response<amplifier_module::Empty>, Status> {
        let req = request.into_inner();
        let value: serde_json::Value = serde_json::from_str(&req.value_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid value_json: {e}")))?;
        self.coordinator.register_capability(&req.name, value);
        Ok(Response::new(amplifier_module::Empty {}))
    }

    async fn get_capability(
        &self,
        request: Request<amplifier_module::GetCapabilityRequest>,
    ) -> Result<Response<amplifier_module::GetCapabilityResponse>, Status> {
        let req = request.into_inner();
        match self.coordinator.get_capability(&req.name) {
            Some(value) => {
                let value_json = serde_json::to_string(&value)
                    .map_err(|e| Status::internal(format!("Failed to serialize capability: {e}")))?;
                Ok(Response::new(amplifier_module::GetCapabilityResponse {
                    found: true,
                    value_json,
                }))
            }
            None => Ok(Response::new(amplifier_module::GetCapabilityResponse {
                found: false,
                value_json: String::new(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_service_impl_compiles() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let _service = KernelServiceImpl::new(coord);
    }

    // -----------------------------------------------------------------------
    // EmitHook tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn emit_hook_with_no_handlers_returns_continue() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookRequest {
            event: "test:event".to_string(),
            data_json: r#"{"key": "value"}"#.to_string(),
        });

        let result = service.emit_hook(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");
        let inner = result.unwrap().into_inner();
        assert_eq!(inner.action, amplifier_module::HookAction::Continue as i32);
    }

    #[tokio::test]
    async fn emit_hook_calls_registered_handler() {
        use crate::testing::FakeHookHandler;

        let coord = Arc::new(Coordinator::new(Default::default()));
        let handler = Arc::new(FakeHookHandler::new());
        coord
            .hooks()
            .register("test:event", handler.clone(), 0, Some("test-hook".into()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookRequest {
            event: "test:event".to_string(),
            data_json: r#"{"key": "value"}"#.to_string(),
        });

        let result = service.emit_hook(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");

        let events = handler.recorded_events();
        assert_eq!(events.len(), 1, "Handler should have been called once");
        assert_eq!(events[0].0, "test:event");
    }

    #[tokio::test]
    async fn emit_hook_returns_handler_result() {
        use crate::models::{HookAction, HookResult};
        use crate::testing::FakeHookHandler;

        let coord = Arc::new(Coordinator::new(Default::default()));
        let deny_result = HookResult {
            action: HookAction::Deny,
            reason: Some("blocked by test".into()),
            ..Default::default()
        };
        let handler = Arc::new(FakeHookHandler::with_result(deny_result));
        coord
            .hooks()
            .register("test:event", handler, 0, Some("deny-hook".into()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
        });

        let result = service.emit_hook(request).await.unwrap();
        let inner = result.into_inner();
        assert_eq!(
            inner.action,
            amplifier_module::HookAction::Deny as i32,
            "Expected Deny action from handler"
        );
        assert_eq!(inner.reason, "blocked by test");
    }

    #[tokio::test]
    async fn emit_hook_invalid_json_returns_invalid_argument() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookRequest {
            event: "test:event".to_string(),
            data_json: "not-valid-json{{{".to_string(),
        });

        let result = service.emit_hook(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn emit_hook_empty_data_json_uses_empty_object() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookRequest {
            event: "test:event".to_string(),
            data_json: String::new(), // empty → should default to {}
        });

        // With no handlers, should still succeed (Continue result)
        let result = service.emit_hook(request).await;
        assert!(
            result.is_ok(),
            "Empty data_json should succeed, got: {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // EmitHookAndCollect tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn emit_hook_and_collect_with_no_handlers_returns_empty() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
            timeout_seconds: 5.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");
        let inner = result.unwrap().into_inner();
        assert!(
            inner.responses_json.is_empty(),
            "Expected empty responses with no handlers"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_returns_data_from_handlers() {
        use crate::models::HookResult;
        use crate::testing::FakeHookHandler;
        use std::collections::HashMap;

        let coord = Arc::new(Coordinator::new(Default::default()));

        let mut data_map = HashMap::new();
        data_map.insert("result".to_string(), serde_json::json!("from-handler"));
        let result_with_data = HookResult {
            data: Some(data_map),
            ..Default::default()
        };
        let handler = Arc::new(FakeHookHandler::with_result(result_with_data));
        coord
            .hooks()
            .register("collect:event", handler, 0, Some("data-hook".into()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "collect:event".to_string(),
            data_json: r#"{"input": "test"}"#.to_string(),
            timeout_seconds: 5.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");
        let inner = result.unwrap().into_inner();
        assert_eq!(inner.responses_json.len(), 1, "Expected 1 response from handler");

        let parsed: serde_json::Value =
            serde_json::from_str(&inner.responses_json[0]).expect("response must be valid JSON");
        assert_eq!(parsed["result"], serde_json::json!("from-handler"));
    }

    #[tokio::test]
    async fn emit_hook_and_collect_multiple_handlers_returns_all_data() {
        use crate::models::HookResult;
        use crate::testing::FakeHookHandler;
        use std::collections::HashMap;

        let coord = Arc::new(Coordinator::new(Default::default()));

        for i in 0..3u32 {
            let mut data_map = HashMap::new();
            data_map.insert("handler_id".to_string(), serde_json::json!(i));
            let result_with_data = HookResult {
                data: Some(data_map),
                ..Default::default()
            };
            let handler = Arc::new(FakeHookHandler::with_result(result_with_data));
            coord.hooks().register(
                "multi:event",
                handler,
                i as i32,
                Some(format!("handler-{i}")),
            );
        }

        let service = KernelServiceImpl::new(coord);
        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "multi:event".to_string(),
            data_json: String::new(),
            timeout_seconds: 5.0,
        });

        let result = service.emit_hook_and_collect(request).await.unwrap();
        let inner = result.into_inner();
        assert_eq!(
            inner.responses_json.len(),
            3,
            "Expected 3 responses from 3 handlers"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_invalid_json_returns_invalid_argument() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: "bad-json{{".to_string(),
            timeout_seconds: 5.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    // -----------------------------------------------------------------------
    // RegisterCapability tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn register_capability_stores_value() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord.clone());

        let request = Request::new(amplifier_module::RegisterCapabilityRequest {
            name: "my-cap".to_string(),
            value_json: r#"{"key":"value"}"#.to_string(),
        });

        let result = service.register_capability(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");

        // Verify the capability is actually stored
        let stored = coord.get_capability("my-cap");
        assert_eq!(stored, Some(serde_json::json!({"key": "value"})));
    }

    #[tokio::test]
    async fn register_capability_invalid_json_returns_invalid_argument() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::RegisterCapabilityRequest {
            name: "my-cap".to_string(),
            value_json: "not-valid-json{{{".to_string(),
        });

        let result = service.register_capability(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    // -----------------------------------------------------------------------
    // GetCapability tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn get_capability_returns_found_true_when_registered() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.register_capability("streaming", serde_json::json!(true));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetCapabilityRequest {
            name: "streaming".to_string(),
        });

        let result = service.get_capability(request).await.unwrap();
        let inner = result.into_inner();
        assert!(inner.found);
        let parsed: serde_json::Value = serde_json::from_str(&inner.value_json).unwrap();
        assert_eq!(parsed, serde_json::json!(true));
    }

    #[tokio::test]
    async fn get_capability_returns_found_false_when_missing() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetCapabilityRequest {
            name: "nonexistent".to_string(),
        });

        let result = service.get_capability(request).await.unwrap();
        let inner = result.into_inner();
        assert!(!inner.found);
        assert!(inner.value_json.is_empty());
    }

    #[tokio::test]
    async fn register_then_get_capability_roundtrip() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        // Register
        let reg_request = Request::new(amplifier_module::RegisterCapabilityRequest {
            name: "config".to_string(),
            value_json: r#"{"model":"gpt-4","max_tokens":1000}"#.to_string(),
        });
        service.register_capability(reg_request).await.unwrap();

        // Get
        let get_request = Request::new(amplifier_module::GetCapabilityRequest {
            name: "config".to_string(),
        });
        let result = service.get_capability(get_request).await.unwrap();
        let inner = result.into_inner();
        assert!(inner.found);
        let parsed: serde_json::Value = serde_json::from_str(&inner.value_json).unwrap();
        assert_eq!(parsed["model"], serde_json::json!("gpt-4"));
        assert_eq!(parsed["max_tokens"], serde_json::json!(1000));
    }

    // -----------------------------------------------------------------------
    // GetMountedModule tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn get_mounted_module_finds_tool_by_name() {
        use crate::testing::FakeTool;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_tool("my-tool", Arc::new(FakeTool::new("my-tool", "a test tool")));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMountedModuleRequest {
            module_name: "my-tool".to_string(),
            module_type: amplifier_module::ModuleType::Tool as i32,
        });

        let result = service.get_mounted_module(request).await.unwrap();
        let inner = result.into_inner();
        assert!(inner.found, "Expected found=true for mounted tool");
        let info = inner.info.expect("Expected ModuleInfo to be present");
        assert_eq!(info.name, "my-tool");
        assert_eq!(
            info.module_type,
            amplifier_module::ModuleType::Tool as i32
        );
    }

    #[tokio::test]
    async fn get_mounted_module_returns_not_found_for_missing_tool() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMountedModuleRequest {
            module_name: "nonexistent-tool".to_string(),
            module_type: amplifier_module::ModuleType::Tool as i32,
        });

        let result = service.get_mounted_module(request).await.unwrap();
        let inner = result.into_inner();
        assert!(!inner.found, "Expected found=false for missing tool");
        assert!(inner.info.is_none());
    }

    #[tokio::test]
    async fn get_mounted_module_finds_provider_by_name() {
        use crate::testing::FakeProvider;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_provider("openai", Arc::new(FakeProvider::new("openai", "hello")));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMountedModuleRequest {
            module_name: "openai".to_string(),
            module_type: amplifier_module::ModuleType::Provider as i32,
        });

        let result = service.get_mounted_module(request).await.unwrap();
        let inner = result.into_inner();
        assert!(inner.found, "Expected found=true for mounted provider");
        let info = inner.info.expect("Expected ModuleInfo to be present");
        assert_eq!(info.name, "openai");
        assert_eq!(
            info.module_type,
            amplifier_module::ModuleType::Provider as i32
        );
    }

    #[tokio::test]
    async fn get_mounted_module_unspecified_type_finds_tool() {
        use crate::testing::FakeTool;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_tool("bash", Arc::new(FakeTool::new("bash", "runs bash")));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMountedModuleRequest {
            module_name: "bash".to_string(),
            module_type: amplifier_module::ModuleType::Unspecified as i32,
        });

        let result = service.get_mounted_module(request).await.unwrap();
        let inner = result.into_inner();
        assert!(inner.found, "UNSPECIFIED type should find a mounted tool");
        let info = inner.info.expect("Expected ModuleInfo to be present");
        assert_eq!(info.name, "bash");
        assert_eq!(
            info.module_type,
            amplifier_module::ModuleType::Tool as i32
        );
    }

    #[tokio::test]
    async fn get_mounted_module_unspecified_type_finds_provider() {
        use crate::testing::FakeProvider;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_provider("anthropic", Arc::new(FakeProvider::new("anthropic", "hi")));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMountedModuleRequest {
            module_name: "anthropic".to_string(),
            module_type: amplifier_module::ModuleType::Unspecified as i32,
        });

        let result = service.get_mounted_module(request).await.unwrap();
        let inner = result.into_inner();
        assert!(inner.found, "UNSPECIFIED type should find a mounted provider");
        let info = inner.info.expect("Expected ModuleInfo to be present");
        assert_eq!(info.name, "anthropic");
        assert_eq!(
            info.module_type,
            amplifier_module::ModuleType::Provider as i32
        );
    }

    #[tokio::test]
    async fn get_mounted_module_wrong_type_returns_not_found() {
        use crate::testing::FakeTool;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_tool("my-tool", Arc::new(FakeTool::new("my-tool", "a test tool")));
        let service = KernelServiceImpl::new(coord);

        // Tool is mounted but we query as PROVIDER type — should not find it
        let request = Request::new(amplifier_module::GetMountedModuleRequest {
            module_name: "my-tool".to_string(),
            module_type: amplifier_module::ModuleType::Provider as i32,
        });

        let result = service.get_mounted_module(request).await.unwrap();
        let inner = result.into_inner();
        assert!(
            !inner.found,
            "Querying a tool name as PROVIDER type should return not found"
        );
    }

    // -----------------------------------------------------------------------
    // AddMessage tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn add_message_stores_message_in_context() {
        use crate::testing::FakeContextManager;
        use crate::traits::ContextManager as _;
        let coord = Arc::new(Coordinator::new(Default::default()));
        let ctx = Arc::new(FakeContextManager::new());
        coord.set_context(ctx.clone());
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::KernelAddMessageRequest {
            session_id: String::new(),
            message: Some(amplifier_module::Message {
                role: amplifier_module::Role::User as i32,
                content: Some(amplifier_module::message::Content::TextContent(
                    "Hello from gRPC".to_string(),
                )),
                name: String::new(),
                tool_call_id: String::new(),
                metadata_json: String::new(),
            }),
        });

        let result = service.add_message(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");

        // Verify message was stored in context
        let messages = ctx.get_messages().await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    #[tokio::test]
    async fn add_message_no_context_returns_failed_precondition() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::KernelAddMessageRequest {
            session_id: String::new(),
            message: Some(amplifier_module::Message {
                role: amplifier_module::Role::User as i32,
                content: Some(amplifier_module::message::Content::TextContent(
                    "Hello".to_string(),
                )),
                name: String::new(),
                tool_call_id: String::new(),
                metadata_json: String::new(),
            }),
        });

        let result = service.add_message(request).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::FailedPrecondition,
            "Should return FailedPrecondition when no context mounted"
        );
    }

    #[tokio::test]
    async fn add_message_missing_message_field_returns_invalid_argument() {
        use crate::testing::FakeContextManager;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.set_context(Arc::new(FakeContextManager::new()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::KernelAddMessageRequest {
            session_id: String::new(),
            message: None, // no message
        });

        let result = service.add_message(request).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "Should return InvalidArgument when message field is missing"
        );
    }

    // -----------------------------------------------------------------------
    // GetMessages tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn get_messages_returns_stored_messages() {
        use crate::testing::FakeContextManager;
        use crate::traits::ContextManager as _;
        let coord = Arc::new(Coordinator::new(Default::default()));
        let ctx = Arc::new(FakeContextManager::new());
        // Pre-populate context with two messages via Value
        ctx.add_message(serde_json::json!({"role": "user", "content": "hi"}))
            .await
            .unwrap();
        ctx.add_message(serde_json::json!({"role": "assistant", "content": "hello"}))
            .await
            .unwrap();
        coord.set_context(ctx);
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMessagesRequest {
            session_id: String::new(),
        });

        let result = service.get_messages(request).await.unwrap();
        let inner = result.into_inner();
        assert_eq!(inner.messages.len(), 2, "Expected 2 messages");
    }

    #[tokio::test]
    async fn get_messages_empty_context_returns_empty_list() {
        use crate::testing::FakeContextManager;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.set_context(Arc::new(FakeContextManager::new()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMessagesRequest {
            session_id: String::new(),
        });

        let result = service.get_messages(request).await.unwrap();
        let inner = result.into_inner();
        assert!(inner.messages.is_empty(), "Expected empty messages list");
    }

    #[tokio::test]
    async fn get_messages_no_context_returns_failed_precondition() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::GetMessagesRequest {
            session_id: String::new(),
        });

        let result = service.get_messages(request).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::FailedPrecondition,
            "Should return FailedPrecondition when no context mounted"
        );
    }

    #[tokio::test]
    async fn add_then_get_messages_roundtrip() {
        use crate::testing::FakeContextManager;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.set_context(Arc::new(FakeContextManager::new()));
        let service = KernelServiceImpl::new(coord);

        // Add a message
        let add_request = Request::new(amplifier_module::KernelAddMessageRequest {
            session_id: String::new(),
            message: Some(amplifier_module::Message {
                role: amplifier_module::Role::User as i32,
                content: Some(amplifier_module::message::Content::TextContent(
                    "Test message content".to_string(),
                )),
                name: String::new(),
                tool_call_id: String::new(),
                metadata_json: String::new(),
            }),
        });
        service.add_message(add_request).await.unwrap();

        // Get messages back
        let get_request = Request::new(amplifier_module::GetMessagesRequest {
            session_id: String::new(),
        });
        let result = service.get_messages(get_request).await.unwrap();
        let inner = result.into_inner();
        assert_eq!(inner.messages.len(), 1);
        assert_eq!(inner.messages[0].role, amplifier_module::Role::User as i32);
        // Verify content is a text block
        match &inner.messages[0].content {
            Some(amplifier_module::message::Content::TextContent(text)) => {
                assert_eq!(text, "Test message content");
            }
            other => panic!("Expected TextContent, got: {other:?}"),
        }
    }
}
