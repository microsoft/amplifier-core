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

#[tokio::test]
async fn grpc_tool_round_trip() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind to a random available port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    // Spawn the gRPC server in the background.
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(ToolServiceServer::new(EchoToolService))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    // Connect the bridge (this calls get_spec internally).
    let bridge = GrpcToolBridge::connect(&format!("http://{}", addr)).await?;

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
