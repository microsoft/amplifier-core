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
        _request: Request<amplifier_module::EmitHookRequest>,
    ) -> Result<Response<amplifier_module::HookResult>, Status> {
        Err(Status::unimplemented("EmitHook not yet implemented"))
    }

    async fn emit_hook_and_collect(
        &self,
        _request: Request<amplifier_module::EmitHookAndCollectRequest>,
    ) -> Result<Response<amplifier_module::EmitHookAndCollectResponse>, Status> {
        Err(Status::unimplemented(
            "EmitHookAndCollect not yet implemented",
        ))
    }

    async fn get_messages(
        &self,
        _request: Request<amplifier_module::GetMessagesRequest>,
    ) -> Result<Response<amplifier_module::GetMessagesResponse>, Status> {
        Err(Status::unimplemented("GetMessages not yet implemented"))
    }

    async fn add_message(
        &self,
        _request: Request<amplifier_module::KernelAddMessageRequest>,
    ) -> Result<Response<amplifier_module::Empty>, Status> {
        Err(Status::unimplemented("AddMessage not yet implemented"))
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
}
