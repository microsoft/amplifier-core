//! KernelService gRPC server implementation.
//!
//! This module implements the KernelService proto as a tonic gRPC server.
//! Out-of-process modules (Go, TypeScript, etc.) call back to the kernel
//! via this service for provider/tool/hook/context access.

use std::sync::Arc;

use tonic::service::Interceptor;
use tonic::{Request, Response, Status};

use crate::coordinator::Coordinator;
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::kernel_service_server::KernelService;
use crate::generated::conversions::{
    native_chat_response_to_proto, native_hook_result_to_proto, native_message_to_proto,
    proto_chat_request_to_native, proto_message_to_native,
};

/// Shared-secret authentication interceptor for KernelService.
/// Validates the `x-amplifier-token` metadata header on every request.
#[derive(Clone)]
pub struct AuthInterceptor {
    expected_token: String,
}

impl AuthInterceptor {
    pub fn new(token: String) -> Self {
        Self {
            expected_token: token,
        }
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, request: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        let token = request
            .metadata()
            .get("x-amplifier-token")
            .and_then(|v| v.to_str().ok());

        match token {
            Some(t) if t == self.expected_token => Ok(request),
            Some(_) => Err(Status::unauthenticated("invalid token")),
            None => Err(Status::unauthenticated("missing x-amplifier-token header")),
        }
    }
}

/// Maximum allowed timeout for [`emit_hook_and_collect`] requests (5 minutes).
///
/// Values above this are silently clamped to the cap so that a misbehaving
/// module cannot hold a collect call open indefinitely.
const MAX_HOOK_COLLECT_TIMEOUT_SECS: f64 = 300.0;

/// Default timeout applied when the caller sends a non-positive or non-finite
/// value for `timeout_seconds` in an [`EmitHookAndCollectRequest`].
const DEFAULT_HOOK_COLLECT_TIMEOUT_SECS: u64 = 30;

/// Maximum allowed size for any JSON payload field received over gRPC.
///
/// Payloads exceeding this limit are rejected with `Status::invalid_argument`
/// before any parsing or coordinator work is attempted.
const MAX_JSON_PAYLOAD_BYTES: usize = 64 * 1024; // 64 KB

/// Validate that a JSON string field does not exceed [`MAX_JSON_PAYLOAD_BYTES`].
///
/// Returns `Err(Status::invalid_argument(...))` when the payload is too large,
/// so callers can use the `?` operator directly.
///
/// `tonic::Status` is unavoidably large; suppressing `result_large_err` here is
/// consistent with every other gRPC method in this file.
#[allow(clippy::result_large_err)]
fn validate_json_size(json: &str, field_name: &str) -> Result<(), Status> {
    if json.len() > MAX_JSON_PAYLOAD_BYTES {
        return Err(Status::invalid_argument(format!(
            "{field_name} exceeds maximum size of {MAX_JSON_PAYLOAD_BYTES} bytes"
        )));
    }
    Ok(())
}

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

    /// Create a new KernelServiceImpl with a randomly generated auth token.
    /// Returns `(service, token)` — the token must be passed to connecting modules.
    pub fn new_with_auth(coordinator: Arc<Coordinator>) -> (Self, String) {
        let token = uuid::Uuid::new_v4().to_string();
        (Self { coordinator }, token)
    }
}

#[tonic::async_trait]
impl KernelService for KernelServiceImpl {
    async fn complete_with_provider(
        &self,
        request: Request<amplifier_module::CompleteWithProviderRequest>,
    ) -> Result<Response<amplifier_module::ChatResponse>, Status> {
        let req = request.into_inner();
        let provider_name = &req.provider_name;

        // Look up the provider in the coordinator
        let provider = self
            .coordinator
            .get_provider(provider_name)
            .ok_or_else(|| {
                log::debug!("Provider lookup failed: {provider_name}");
                Status::not_found("Provider not available")
            })?;

        // Extract the proto ChatRequest (required field)
        let proto_chat_request = req
            .request
            .ok_or_else(|| Status::invalid_argument("Missing required field: request"))?;

        // Enforce payload size limit on the request's metadata_json field
        if !proto_chat_request.metadata_json.is_empty() {
            validate_json_size(&proto_chat_request.metadata_json, "request.metadata_json")?;
        }

        // Convert proto ChatRequest → native ChatRequest
        let native_request = proto_chat_request_to_native(proto_chat_request);

        // Call the provider
        match provider.complete(native_request).await {
            Ok(native_response) => {
                let proto_response = native_chat_response_to_proto(&native_response);
                Ok(Response::new(proto_response))
            }
            Err(e) => {
                log::error!("Provider completion failed for {provider_name}: {e}");
                Err(Status::internal("Provider completion failed"))
            }
        }
    }

    type CompleteWithProviderStreamingStream =
        tokio_stream::wrappers::ReceiverStream<Result<amplifier_module::ChatResponse, Status>>;

    async fn complete_with_provider_streaming(
        &self,
        request: Request<amplifier_module::CompleteWithProviderRequest>,
    ) -> Result<Response<Self::CompleteWithProviderStreamingStream>, Status> {
        let req = request.into_inner();
        let provider_name = &req.provider_name;

        // Look up the provider in the coordinator
        let provider = self
            .coordinator
            .get_provider(provider_name)
            .ok_or_else(|| {
                log::debug!("Provider lookup failed: {provider_name}");
                Status::not_found("Provider not available")
            })?;

        // Extract the proto ChatRequest (required field)
        let proto_chat_request = req
            .request
            .ok_or_else(|| Status::invalid_argument("Missing required field: request"))?;

        // Enforce payload size limit on the request's metadata_json field
        if !proto_chat_request.metadata_json.is_empty() {
            validate_json_size(&proto_chat_request.metadata_json, "request.metadata_json")?;
        }

        // Convert proto ChatRequest → native ChatRequest
        let native_request = proto_chat_request_to_native(proto_chat_request);

        // Call the provider
        let native_response = provider.complete(native_request).await.map_err(|e| {
            log::error!("Provider completion failed for {provider_name}: {e}");
            Status::internal("Provider completion failed")
        })?;

        let proto_response = native_chat_response_to_proto(&native_response);

        // NOTE: This is a one-shot "streaming" endpoint — it awaits the full provider
        // response, then sends it as a single stream element. True token-level streaming
        // requires provider.complete_stream() → Stream<Item = ChatResponse>, which is
        // not yet implemented. This endpoint exists for proto/gRPC API compatibility
        // so clients can use the streaming RPC shape ahead of the real implementation.
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        if tx.send(Ok(proto_response)).await.is_err() {
            log::debug!("Streaming client disconnected before response was sent");
        }
        // `tx` is dropped here, closing the channel and ending the stream.

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn execute_tool(
        &self,
        request: Request<amplifier_module::ExecuteToolRequest>,
    ) -> Result<Response<amplifier_module::ToolResult>, Status> {
        let req = request.into_inner();
        let tool_name = &req.tool_name;

        // Look up the tool in the coordinator
        let tool = self.coordinator.get_tool(tool_name).ok_or_else(|| {
            log::debug!("Tool lookup failed: {tool_name}");
            Status::not_found("Tool not available")
        })?;

        // Enforce payload size limit before parsing
        validate_json_size(&req.input_json, "input_json")?;

        // Parse input JSON
        let input: serde_json::Value = serde_json::from_str(&req.input_json).map_err(|e| {
            log::debug!("Tool input JSON parse error for {tool_name}: {e}");
            Status::invalid_argument("Invalid input JSON")
        })?;

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
            Err(e) => {
                log::error!("Tool execution failed for {tool_name}: {e}");
                Err(Status::internal("Tool execution failed"))
            }
        }
    }

    async fn emit_hook(
        &self,
        request: Request<amplifier_module::EmitHookRequest>,
    ) -> Result<Response<amplifier_module::HookResult>, Status> {
        let req = request.into_inner();

        // Enforce payload size limit before parsing
        if !req.data_json.is_empty() {
            validate_json_size(&req.data_json, "data_json")?;
        }

        let data: serde_json::Value = if req.data_json.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&req.data_json).map_err(|e| {
                log::debug!(
                    "emit_hook data_json parse error for event '{}': {e}",
                    req.event
                );
                Status::invalid_argument("Invalid data_json")
            })?
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

        // Enforce payload size limit before parsing
        if !req.data_json.is_empty() {
            validate_json_size(&req.data_json, "data_json")?;
        }

        let data: serde_json::Value = if req.data_json.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&req.data_json).map_err(|e| {
                log::debug!(
                    "emit_hook_and_collect data_json parse error for event '{}': {e}",
                    req.event
                );
                Status::invalid_argument("Invalid data_json")
            })?
        };

        let timeout = if req.timeout_seconds.is_finite() && req.timeout_seconds > 0.0 {
            std::time::Duration::from_secs_f64(req.timeout_seconds.min(MAX_HOOK_COLLECT_TIMEOUT_SECS))
        } else {
            std::time::Duration::from_secs(DEFAULT_HOOK_COLLECT_TIMEOUT_SECS)
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

        Ok(Response::new(
            amplifier_module::EmitHookAndCollectResponse { responses_json },
        ))
    }

    /// Get all conversation messages from the context manager.
    ///
    /// ## Session routing
    ///
    /// Session routing is implicit — each `KernelServiceImpl` is scoped to one
    /// `Coordinator`. The `session_id` field is logged but not validated.
    /// Cross-session isolation requires deploying separate `KernelService`
    /// instances per session.
    async fn get_messages(
        &self,
        request: Request<amplifier_module::GetMessagesRequest>,
    ) -> Result<Response<amplifier_module::GetMessagesResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "get_messages: session_id={:?} (routing is implicit — \
             each KernelServiceImpl is scoped to one Coordinator)",
            req.session_id
        );

        let context = self
            .coordinator
            .context()
            .ok_or_else(|| Status::failed_precondition("No context manager mounted"))?;

        let values = context.get_messages().await.map_err(|e| {
            log::error!("Failed to get messages from context: {e}");
            Status::internal("Failed to get messages")
        })?;

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

    /// Add a message to the context manager.
    ///
    /// ## Session routing
    ///
    /// Session routing is implicit — each `KernelServiceImpl` is scoped to one
    /// `Coordinator`. The `session_id` field is logged but not validated.
    /// Cross-session isolation requires deploying separate `KernelService`
    /// instances per session.
    async fn add_message(
        &self,
        request: Request<amplifier_module::KernelAddMessageRequest>,
    ) -> Result<Response<amplifier_module::Empty>, Status> {
        let req = request.into_inner();
        log::debug!(
            "add_message: session_id={:?} (routing is implicit — \
             each KernelServiceImpl is scoped to one Coordinator)",
            req.session_id
        );

        let proto_message = req
            .message
            .ok_or_else(|| Status::invalid_argument("Missing required field: message"))?;

        // Enforce payload size limit on the message's metadata_json field
        if !proto_message.metadata_json.is_empty() {
            validate_json_size(&proto_message.metadata_json, "message.metadata_json")?;
        }

        let native_message = proto_message_to_native(proto_message).map_err(|e| {
            log::debug!("Message conversion error: {e}");
            Status::invalid_argument("Invalid message")
        })?;

        let value = serde_json::to_value(native_message).map_err(|e| {
            log::error!("Failed to serialize message to JSON: {e}");
            Status::internal("Failed to serialize message")
        })?;

        let context = self
            .coordinator
            .context()
            .ok_or_else(|| Status::failed_precondition("No context manager mounted"))?;

        context.add_message(value).await.map_err(|e| {
            log::error!("Failed to add message to context: {e}");
            Status::internal("Failed to add message")
        })?;

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
                self.coordinator
                    .get_tool(module_name)
                    .map(|tool| amplifier_module::ModuleInfo {
                        name: tool.name().to_string(),
                        module_type: amplifier_module::ModuleType::Tool as i32,
                        ..Default::default()
                    })
            }
            amplifier_module::ModuleType::Provider => self
                .coordinator
                .get_provider(module_name)
                .map(|provider| amplifier_module::ModuleInfo {
                    name: provider.name().to_string(),
                    module_type: amplifier_module::ModuleType::Provider as i32,
                    ..Default::default()
                }),
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

        // Enforce payload size limit before parsing
        validate_json_size(&req.value_json, "value_json")?;

        let value: serde_json::Value = serde_json::from_str(&req.value_json).map_err(|e| {
            log::debug!(
                "register_capability value_json parse error for '{}': {e}",
                req.name
            );
            Status::invalid_argument("Invalid value_json")
        })?;
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
                let value_json = serde_json::to_string(&value).map_err(|e| {
                    log::error!("Failed to serialize capability '{}': {e}", req.name);
                    Status::internal("Failed to serialize capability")
                })?;
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
        assert_eq!(
            inner.responses_json.len(),
            1,
            "Expected 1 response from handler"
        );

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

    #[tokio::test]
    async fn emit_hook_and_collect_normal_timeout_passes_through() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
            timeout_seconds: 10.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(result.is_ok(), "10s timeout should succeed, got: {result:?}");
    }

    #[tokio::test]
    async fn emit_hook_and_collect_huge_timeout_is_clamped_to_max() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
            timeout_seconds: 999_999.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "Huge timeout should be clamped, not rejected: {result:?}"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_nan_timeout_falls_back_to_default() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
            timeout_seconds: f64::NAN,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "NaN timeout should fall back to default, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_infinity_timeout_falls_back_to_default() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
            timeout_seconds: f64::INFINITY,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "Infinity timeout should fall back to default, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_negative_timeout_falls_back_to_default() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
            timeout_seconds: -5.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "Negative timeout should fall back to default, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_neg_infinity_timeout_falls_back_to_default() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: String::new(),
            timeout_seconds: f64::NEG_INFINITY,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "NEG_INFINITY timeout should fall back to default, got: {result:?}"
        );
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
        assert_eq!(info.module_type, amplifier_module::ModuleType::Tool as i32);
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
        assert_eq!(info.module_type, amplifier_module::ModuleType::Tool as i32);
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
        assert!(
            inner.found,
            "UNSPECIFIED type should find a mounted provider"
        );
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

    // -----------------------------------------------------------------------
    // H-04: Session ID routing documentation
    // -----------------------------------------------------------------------

    /// Session ID is received and logged but does NOT affect routing.
    /// Each KernelServiceImpl is scoped to one Coordinator — cross-session
    /// isolation requires deploying separate KernelService instances per session.
    #[tokio::test]
    async fn get_messages_session_id_received_but_does_not_affect_routing() {
        use crate::testing::FakeContextManager;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.set_context(Arc::new(FakeContextManager::new()));
        let service = KernelServiceImpl::new(coord);

        // Even with an explicit session_id, routing is to the single scoped coordinator
        let request = Request::new(amplifier_module::GetMessagesRequest {
            session_id: "explicit-session-abc123".to_string(),
        });

        let result = service.get_messages(request).await;
        assert!(
            result.is_ok(),
            "get_messages must succeed regardless of session_id value; got: {result:?}"
        );
    }

    /// add_message: session_id is received and logged but does NOT affect routing.
    #[tokio::test]
    async fn add_message_session_id_received_but_does_not_affect_routing() {
        use crate::testing::FakeContextManager;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.set_context(Arc::new(FakeContextManager::new()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::KernelAddMessageRequest {
            session_id: "explicit-session-abc123".to_string(),
            message: Some(amplifier_module::Message {
                role: amplifier_module::Role::User as i32,
                content: Some(amplifier_module::message::Content::TextContent(
                    "hello".to_string(),
                )),
                name: String::new(),
                tool_call_id: String::new(),
                metadata_json: String::new(),
            }),
        });

        let result = service.add_message(request).await;
        assert!(
            result.is_ok(),
            "add_message must succeed regardless of session_id value; got: {result:?}"
        );
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

    // -----------------------------------------------------------------------
    // CompleteWithProvider tests
    // -----------------------------------------------------------------------

    /// Build a minimal proto ChatRequest with a single user message.
    fn make_chat_request(text: &str) -> amplifier_module::ChatRequest {
        amplifier_module::ChatRequest {
            messages: vec![amplifier_module::Message {
                role: amplifier_module::Role::User as i32,
                content: Some(amplifier_module::message::Content::TextContent(
                    text.to_string(),
                )),
                name: String::new(),
                tool_call_id: String::new(),
                metadata_json: String::new(),
            }],
            tools: vec![],
            response_format: None,
            temperature: 0.0,
            top_p: 0.0,
            max_output_tokens: 0,
            conversation_id: String::new(),
            stream: false,
            metadata_json: String::new(),
            model: String::new(),
            tool_choice: String::new(),
            stop: vec![],
            reasoning_effort: String::new(),
            timeout: 0.0,
        }
    }

    #[tokio::test]
    async fn complete_with_provider_returns_response_from_mounted_provider() {
        use crate::testing::FakeProvider;

        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_provider(
            "openai",
            Arc::new(FakeProvider::new("openai", "hello from openai")),
        );
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "openai".to_string(),
            request: Some(make_chat_request("ping")),
        });

        let result = service.complete_with_provider(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");
        let inner = result.unwrap().into_inner();
        // The content field contains JSON-serialized ContentBlocks
        assert!(!inner.content.is_empty(), "Expected non-empty content");
        assert!(
            inner.content.contains("hello from openai"),
            "Expected response to contain provider text, got: {}",
            inner.content
        );
    }

    #[tokio::test]
    async fn complete_with_provider_not_found_returns_not_found_status() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "nonexistent-provider".to_string(),
            request: Some(make_chat_request("hello")),
        });

        let result = service.complete_with_provider(request).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::NotFound,
            "Should return NotFound when provider is not mounted"
        );
    }

    #[tokio::test]
    async fn complete_with_provider_missing_request_returns_invalid_argument() {
        use crate::testing::FakeProvider;

        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_provider("openai", Arc::new(FakeProvider::new("openai", "response")));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "openai".to_string(),
            request: None, // missing request
        });

        let result = service.complete_with_provider(request).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "Should return InvalidArgument when request field is missing"
        );
    }

    #[tokio::test]
    async fn complete_with_provider_records_call_in_provider() {
        use crate::testing::FakeProvider;

        let coord = Arc::new(Coordinator::new(Default::default()));
        let fake_provider = Arc::new(FakeProvider::new("anthropic", "recorded response"));
        coord.mount_provider("anthropic", fake_provider.clone());
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "anthropic".to_string(),
            request: Some(make_chat_request("test message")),
        });

        let result = service.complete_with_provider(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");

        let calls = fake_provider.recorded_calls();
        assert_eq!(calls.len(), 1, "Provider should have been called once");
        assert_eq!(calls[0].messages.len(), 1);
    }

    // -----------------------------------------------------------------------
    // CompleteWithProviderStreaming tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn complete_with_provider_streaming_returns_single_response() {
        use crate::testing::FakeProvider;
        use tokio_stream::StreamExt as _;

        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_provider(
            "openai",
            Arc::new(FakeProvider::new("openai", "streamed hello")),
        );
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "openai".to_string(),
            request: Some(make_chat_request("ping")),
        });

        let result = service.complete_with_provider_streaming(request).await;
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");

        let mut stream = result.unwrap().into_inner();
        let mut chunks = Vec::new();
        while let Some(item) = stream.next().await {
            chunks.push(item);
        }

        assert_eq!(chunks.len(), 1, "Expected exactly one streamed chunk");
        let response = chunks.remove(0).expect("Expected Ok chunk");
        assert!(
            response.content.contains("streamed hello"),
            "Expected response to contain provider text, got: {}",
            response.content
        );
    }

    #[tokio::test]
    async fn complete_with_provider_streaming_not_found_returns_error() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "nonexistent".to_string(),
            request: Some(make_chat_request("ping")),
        });

        let result = service.complete_with_provider_streaming(request).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::NotFound,
            "Should return NotFound when provider is not mounted"
        );
    }

    #[tokio::test]
    async fn complete_with_provider_streaming_missing_request_returns_invalid_argument() {
        use crate::testing::FakeProvider;

        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_provider("openai", Arc::new(FakeProvider::new("openai", "response")));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "openai".to_string(),
            request: None, // missing request
        });

        let result = service.complete_with_provider_streaming(request).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "Should return InvalidArgument when request field is missing"
        );
    }

    // -----------------------------------------------------------------------
    // AuthInterceptor tests (C-01)
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // H-07: JSON payload size limits
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn execute_tool_rejects_oversized_input_json() {
        use crate::testing::FakeTool;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_tool("my-tool", Arc::new(FakeTool::new("my-tool", "a test tool")));
        let service = KernelServiceImpl::new(coord);

        // 128 KB of JSON — exceeds the 64 KB limit
        let big_value = "x".repeat(128 * 1024);
        let oversized_json = format!("\"{}\"", big_value);

        let request = Request::new(amplifier_module::ExecuteToolRequest {
            tool_name: "my-tool".to_string(),
            input_json: oversized_json,
        });

        let result = service.execute_tool(request).await;
        assert!(result.is_err(), "Expected error for oversized input_json");
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "Expected InvalidArgument for oversized payload"
        );
    }

    #[tokio::test]
    async fn emit_hook_rejects_oversized_data_json() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let big_value = "x".repeat(128 * 1024);
        let oversized_json = format!("\"{}\"", big_value);

        let request = Request::new(amplifier_module::EmitHookRequest {
            event: "test:event".to_string(),
            data_json: oversized_json,
        });

        let result = service.emit_hook(request).await;
        assert!(result.is_err(), "Expected error for oversized data_json");
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "Expected InvalidArgument for oversized payload"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_rejects_oversized_data_json() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let big_value = "x".repeat(128 * 1024);
        let oversized_json = format!("\"{}\"", big_value);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: oversized_json,
            timeout_seconds: 5.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(result.is_err(), "Expected error for oversized data_json");
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "Expected InvalidArgument for oversized payload"
        );
    }

    #[tokio::test]
    async fn register_capability_rejects_oversized_value_json() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let big_value = "x".repeat(128 * 1024);
        let oversized_json = format!("\"{}\"", big_value);

        let request = Request::new(amplifier_module::RegisterCapabilityRequest {
            name: "my-cap".to_string(),
            value_json: oversized_json,
        });

        let result = service.register_capability(request).await;
        assert!(result.is_err(), "Expected error for oversized value_json");
        assert_eq!(
            result.unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "Expected InvalidArgument for oversized payload"
        );
    }

    /// Payloads at or under 64 KB must still be accepted.
    #[tokio::test]
    async fn execute_tool_accepts_payload_at_size_limit() {
        use crate::testing::FakeTool;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_tool("my-tool", Arc::new(FakeTool::new("my-tool", "a test tool")));
        let service = KernelServiceImpl::new(coord);

        // Exactly 64 KB of quoted string content  ← should succeed
        let at_limit = "x".repeat(64 * 1024 - 2); // subtract 2 for the surrounding quotes
        let at_limit_json = format!("\"{}\"", at_limit);
        assert_eq!(at_limit_json.len(), 64 * 1024);

        let request = Request::new(amplifier_module::ExecuteToolRequest {
            tool_name: "my-tool".to_string(),
            input_json: at_limit_json,
        });

        let result = service.execute_tool(request).await;
        assert!(
            result.is_ok(),
            "Payload at exactly the size limit must be accepted; got: {result:?}"
        );
    }

    #[test]
    fn auth_interceptor_rejects_missing_token() {
        use tonic::service::Interceptor as _;

        let mut interceptor = AuthInterceptor::new("secret-token".to_string());
        // Request with no metadata header
        let request = tonic::Request::new(());
        let result = interceptor.call(request);

        assert!(result.is_err(), "Expected Err for missing token");
        let status = result.unwrap_err();
        assert_eq!(
            status.code(),
            tonic::Code::Unauthenticated,
            "Expected Unauthenticated, got: {status:?}"
        );
        assert!(
            status.message().contains("missing"),
            "Expected 'missing' in message, got: {}",
            status.message()
        );
    }

    #[test]
    fn auth_interceptor_rejects_wrong_token() {
        use tonic::service::Interceptor as _;

        let mut interceptor = AuthInterceptor::new("correct-token".to_string());
        let mut request = tonic::Request::new(());
        request
            .metadata_mut()
            .insert("x-amplifier-token", "wrong-token".parse().unwrap());
        let result = interceptor.call(request);

        assert!(result.is_err(), "Expected Err for wrong token");
        let status = result.unwrap_err();
        assert_eq!(
            status.code(),
            tonic::Code::Unauthenticated,
            "Expected Unauthenticated, got: {status:?}"
        );
        assert!(
            status.message().contains("invalid"),
            "Expected 'invalid' in message, got: {}",
            status.message()
        );
    }

    #[test]
    fn auth_interceptor_accepts_correct_token() {
        use tonic::service::Interceptor as _;

        let token = "my-shared-secret";
        let mut interceptor = AuthInterceptor::new(token.to_string());
        let mut request = tonic::Request::new(());
        request
            .metadata_mut()
            .insert("x-amplifier-token", token.parse().unwrap());
        let result = interceptor.call(request);

        assert!(
            result.is_ok(),
            "Expected Ok for correct token, got: {result:?}"
        );
    }

    #[test]
    fn new_with_auth_returns_service_and_nonempty_token() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let (svc, token) = KernelServiceImpl::new_with_auth(coord);
        assert!(!token.is_empty(), "Token must not be empty");
        // Sanity-check: UUID v4 is 36 chars (8-4-4-4-12 with dashes)
        assert_eq!(
            token.len(),
            36,
            "Expected UUID-format token (len 36), got: {token}"
        );
        // Verify the service is usable
        let _ = svc;
    }

    #[test]
    fn new_with_auth_tokens_are_unique() {
        let coord1 = Arc::new(Coordinator::new(Default::default()));
        let coord2 = Arc::new(Coordinator::new(Default::default()));
        let (_, token1) = KernelServiceImpl::new_with_auth(coord1);
        let (_, token2) = KernelServiceImpl::new_with_auth(coord2);
        assert_ne!(token1, token2, "Each call must produce a unique token");
    }

    // -----------------------------------------------------------------------
    // H-02: Error message sanitization — no internal details leaked to caller
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn complete_with_provider_not_found_message_is_generic() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "secret-internal-provider".to_string(),
            request: Some(make_chat_request("ping")),
        });

        let status = service.complete_with_provider(request).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
        // Provider name must NOT leak to the caller
        assert!(
            !status.message().contains("secret-internal-provider"),
            "Provider name must not appear in error message, got: {}",
            status.message()
        );
    }

    #[tokio::test]
    async fn complete_with_provider_streaming_not_found_message_is_generic() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::CompleteWithProviderRequest {
            provider_name: "secret-internal-provider".to_string(),
            request: Some(make_chat_request("ping")),
        });

        let status = service
            .complete_with_provider_streaming(request)
            .await
            .unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert!(
            !status.message().contains("secret-internal-provider"),
            "Provider name must not appear in error message, got: {}",
            status.message()
        );
    }

    #[tokio::test]
    async fn execute_tool_not_found_message_is_generic() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::ExecuteToolRequest {
            tool_name: "secret-internal-tool".to_string(),
            input_json: "{}".to_string(),
        });

        let status = service.execute_tool(request).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert!(
            !status.message().contains("secret-internal-tool"),
            "Tool name must not appear in error message, got: {}",
            status.message()
        );
    }

    #[tokio::test]
    async fn emit_hook_invalid_json_message_has_no_serde_details() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookRequest {
            event: "test:event".to_string(),
            data_json: "not-valid-json{{{".to_string(),
        });

        let status = service.emit_hook(request).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        // The message should be exactly "Invalid data_json" with no serde details
        assert_eq!(
            status.message(),
            "Invalid data_json",
            "Expected generic message, got: {}",
            status.message()
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_invalid_json_message_has_no_serde_details() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:event".to_string(),
            data_json: "bad-json{{".to_string(),
            timeout_seconds: 5.0,
        });

        let status = service.emit_hook_and_collect(request).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert_eq!(
            status.message(),
            "Invalid data_json",
            "Expected generic message, got: {}",
            status.message()
        );
    }

    #[tokio::test]
    async fn execute_tool_invalid_input_json_message_has_no_serde_details() {
        use crate::testing::FakeTool;
        let coord = Arc::new(Coordinator::new(Default::default()));
        coord.mount_tool("my-tool", Arc::new(FakeTool::new("my-tool", "a test tool")));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::ExecuteToolRequest {
            tool_name: "my-tool".to_string(),
            input_json: "not-valid-json{{{".to_string(),
        });

        let status = service.execute_tool(request).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert_eq!(
            status.message(),
            "Invalid input JSON",
            "Expected generic message, got: {}",
            status.message()
        );
    }

    #[tokio::test]
    async fn register_capability_invalid_json_message_has_no_serde_details() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::RegisterCapabilityRequest {
            name: "my-cap".to_string(),
            value_json: "not-valid-json{{{".to_string(),
        });

        let status = service.register_capability(request).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert_eq!(
            status.message(),
            "Invalid value_json",
            "Expected generic message, got: {}",
            status.message()
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
