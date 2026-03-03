//! End-to-end integration test for the gRPC tool bridge.
//!
//! Spins up a tonic gRPC server implementing `ToolService` (an "echo" tool),
//! connects via `GrpcToolBridge::connect`, and verifies spec discovery and
//! tool execution round-trip.

use amplifier_core::bridges::grpc_tool::GrpcToolBridge;
use amplifier_core::generated::amplifier_module::{
    self,
    tool_service_server::{ToolService, ToolServiceServer},
};
use amplifier_core::traits::Tool;

/// A trivial tool service that echoes its input back unchanged.
struct EchoToolService;

#[tonic::async_trait]
impl ToolService for EchoToolService {
    async fn get_spec(
        &self,
        _request: tonic::Request<amplifier_module::Empty>,
    ) -> Result<tonic::Response<amplifier_module::ToolSpec>, tonic::Status> {
        Ok(tonic::Response::new(amplifier_module::ToolSpec {
            name: "echo".to_string(),
            description: "Echoes input back".to_string(),
            parameters_json: "{}".to_string(),
        }))
    }

    async fn execute(
        &self,
        request: tonic::Request<amplifier_module::ToolExecuteRequest>,
    ) -> Result<tonic::Response<amplifier_module::ToolExecuteResponse>, tonic::Status> {
        let req = request.into_inner();
        Ok(tonic::Response::new(
            amplifier_module::ToolExecuteResponse {
                success: true,
                output: req.input,
                content_type: req.content_type,
                error: String::new(),
            },
        ))
    }
}

/// Helper: bind to random port, spawn gRPC server, return address string.
async fn spawn_tool_server(svc: ToolServiceServer<impl ToolService>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(svc)
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });
    format!("http://{}", addr)
}

// ---------------------------------------------------------------------------
// P1-13: Tool with invalid (non-empty) parameters_json
// ---------------------------------------------------------------------------

/// A tool service that returns invalid (non-empty) parameters_json.
struct InvalidParamsToolService;

#[tonic::async_trait]
impl ToolService for InvalidParamsToolService {
    async fn get_spec(
        &self,
        _request: tonic::Request<amplifier_module::Empty>,
    ) -> Result<tonic::Response<amplifier_module::ToolSpec>, tonic::Status> {
        Ok(tonic::Response::new(amplifier_module::ToolSpec {
            name: "bad_params".to_string(),
            description: "Tool with bad params JSON".to_string(),
            parameters_json: "NOT VALID JSON!!!".to_string(),
        }))
    }

    async fn execute(
        &self,
        _request: tonic::Request<amplifier_module::ToolExecuteRequest>,
    ) -> Result<tonic::Response<amplifier_module::ToolExecuteResponse>, tonic::Status> {
        Ok(tonic::Response::new(
            amplifier_module::ToolExecuteResponse {
                success: true,
                output: b"{}".to_vec(),
                content_type: "application/json".to_string(),
                error: String::new(),
            },
        ))
    }
}

// ---------------------------------------------------------------------------
// P1-12: Tool that returns non-JSON output bytes
// ---------------------------------------------------------------------------

/// A tool service that returns binary (non-JSON) output.
struct BinaryOutputToolService;

#[tonic::async_trait]
impl ToolService for BinaryOutputToolService {
    async fn get_spec(
        &self,
        _request: tonic::Request<amplifier_module::Empty>,
    ) -> Result<tonic::Response<amplifier_module::ToolSpec>, tonic::Status> {
        Ok(tonic::Response::new(amplifier_module::ToolSpec {
            name: "binary_out".to_string(),
            description: "Returns non-JSON output".to_string(),
            parameters_json: "{}".to_string(),
        }))
    }

    async fn execute(
        &self,
        _request: tonic::Request<amplifier_module::ToolExecuteRequest>,
    ) -> Result<tonic::Response<amplifier_module::ToolExecuteResponse>, tonic::Status> {
        Ok(tonic::Response::new(
            amplifier_module::ToolExecuteResponse {
                success: true,
                output: vec![0xFF, 0xFE, 0x00, 0x01], // not valid JSON
                content_type: "application/json".to_string(),
                error: String::new(),
            },
        ))
    }
}

// ---------------------------------------------------------------------------
// P1-14: Tool that returns a non-JSON content_type
// ---------------------------------------------------------------------------

/// A tool service that returns content_type "text/plain" with valid JSON output.
struct NonJsonContentTypeToolService;

#[tonic::async_trait]
impl ToolService for NonJsonContentTypeToolService {
    async fn get_spec(
        &self,
        _request: tonic::Request<amplifier_module::Empty>,
    ) -> Result<tonic::Response<amplifier_module::ToolSpec>, tonic::Status> {
        Ok(tonic::Response::new(amplifier_module::ToolSpec {
            name: "text_tool".to_string(),
            description: "Returns text/plain content_type".to_string(),
            parameters_json: "{}".to_string(),
        }))
    }

    async fn execute(
        &self,
        _request: tonic::Request<amplifier_module::ToolExecuteRequest>,
    ) -> Result<tonic::Response<amplifier_module::ToolExecuteResponse>, tonic::Status> {
        Ok(tonic::Response::new(
            amplifier_module::ToolExecuteResponse {
                success: true,
                output: b"{\"result\": 42}".to_vec(),
                content_type: "text/plain".to_string(),
                error: String::new(),
            },
        ))
    }
}

#[tokio::test]
async fn grpc_tool_round_trip() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = spawn_tool_server(ToolServiceServer::new(EchoToolService)).await;

    // Connect the bridge (this calls get_spec internally).
    let bridge = GrpcToolBridge::connect(&endpoint).await?;

    // Verify spec discovery.
    assert_eq!(bridge.name(), "echo");
    assert!(bridge.description().contains("Echo"));

    // Execute the tool and verify the echo response.
    let input = serde_json::json!({"message": "hello"});
    let result = bridge.execute(input.clone()).await?;

    assert!(result.success);
    assert_eq!(result.output, Some(input));

    Ok(())
}

/// P1-13: Connect succeeds with invalid (non-empty) parameters_json — parameters
/// fall back to empty map and a warning is logged (logging verified by code review).
#[tokio::test]
async fn grpc_tool_invalid_parameters_json_falls_back_to_empty(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = spawn_tool_server(ToolServiceServer::new(InvalidParamsToolService)).await;

    let bridge = GrpcToolBridge::connect(&endpoint).await?;

    assert_eq!(bridge.name(), "bad_params");
    // Parameters should be empty despite invalid JSON.
    let spec = bridge.get_spec();
    assert!(
        spec.parameters.is_empty(),
        "invalid parameters_json should fall back to empty map"
    );

    Ok(())
}

/// P1-12: Execute gracefully handles non-JSON output bytes — output is None
/// and a warning is logged (logging verified by code review).
#[tokio::test]
async fn grpc_tool_binary_output_returns_none(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = spawn_tool_server(ToolServiceServer::new(BinaryOutputToolService)).await;

    let bridge = GrpcToolBridge::connect(&endpoint).await?;

    let input = serde_json::json!({"anything": true});
    let result = bridge.execute(input).await?;

    assert!(result.success);
    assert_eq!(
        result.output, None,
        "non-JSON output bytes should result in None"
    );

    Ok(())
}

/// P1-14: Execute succeeds when tool returns non-JSON content_type with valid JSON
/// output — a warning is logged but parsing proceeds.
#[tokio::test]
async fn grpc_tool_non_json_content_type_still_parses(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = spawn_tool_server(ToolServiceServer::new(NonJsonContentTypeToolService)).await;

    let bridge = GrpcToolBridge::connect(&endpoint).await?;

    let input = serde_json::json!({"anything": true});
    let result = bridge.execute(input).await?;

    assert!(result.success);
    assert_eq!(
        result.output,
        Some(serde_json::json!({"result": 42})),
        "valid JSON with non-JSON content_type should still parse successfully"
    );

    Ok(())
}
