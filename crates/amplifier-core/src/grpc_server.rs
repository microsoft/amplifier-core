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
                    .map(|v| serde_json::to_string(&v).unwrap_or_default())
                    .unwrap_or_default();
                let error_json = result
                    .error
                    .map(|e| serde_json::to_string(&e).unwrap_or_default())
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
        _request: Request<amplifier_module::GetMountedModuleRequest>,
    ) -> Result<Response<amplifier_module::GetMountedModuleResponse>, Status> {
        Err(Status::unimplemented(
            "GetMountedModule not yet implemented",
        ))
    }

    async fn register_capability(
        &self,
        _request: Request<amplifier_module::RegisterCapabilityRequest>,
    ) -> Result<Response<amplifier_module::Empty>, Status> {
        Err(Status::unimplemented(
            "RegisterCapability not yet implemented",
        ))
    }

    async fn get_capability(
        &self,
        _request: Request<amplifier_module::GetCapabilityRequest>,
    ) -> Result<Response<amplifier_module::GetCapabilityResponse>, Status> {
        Err(Status::unimplemented("GetCapability not yet implemented"))
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
}
