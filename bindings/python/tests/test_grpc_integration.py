"""Integration test: mock gRPC ToolService loaded by the Python session.

Starts a real gRPC server in-process, connects via loader_grpc, and
verifies the full round-trip: GetSpec + Execute.
"""

import json

import pytest
import pytest_asyncio

# Skip if grpcio not installed
grpc = pytest.importorskip("grpc")
grpc_aio = pytest.importorskip("grpc.aio")


@pytest_asyncio.fixture
async def mock_tool_server():
    """Start a mock gRPC ToolService server on a random port."""
    from amplifier_core._grpc_gen import amplifier_module_pb2
    from amplifier_core._grpc_gen import amplifier_module_pb2_grpc

    class MockToolServicer(amplifier_module_pb2_grpc.ToolServiceServicer):
        async def GetSpec(self, request, context):
            return amplifier_module_pb2.ToolSpec(
                name="mock-echo",
                description="Echoes input back",
                parameters_json='{"type": "object", "properties": {"message": {"type": "string"}}}',
            )

        async def Execute(self, request, context):
            input_data = json.loads(request.input.decode("utf-8"))
            output = {"echoed": input_data.get("message", "(empty)")}
            return amplifier_module_pb2.ToolExecuteResponse(
                success=True,
                output=json.dumps(output).encode("utf-8"),
                content_type="application/json",
            )

    server = grpc_aio.server()
    amplifier_module_pb2_grpc.add_ToolServiceServicer_to_server(
        MockToolServicer(), server
    )
    port = server.add_insecure_port("[::]:0")  # Random available port
    await server.start()
    yield port
    await server.stop(grace=0)


@pytest.mark.asyncio
async def test_grpc_tool_bridge_full_roundtrip(mock_tool_server):
    """Full round-trip: connect -> GetSpec -> Execute -> verify result."""
    from amplifier_core._grpc_gen import amplifier_module_pb2
    from amplifier_core._grpc_gen import amplifier_module_pb2_grpc
    from amplifier_core.loader_grpc import GrpcToolBridge

    endpoint = f"localhost:{mock_tool_server}"
    channel = grpc_aio.insecure_channel(endpoint)
    stub = amplifier_module_pb2_grpc.ToolServiceStub(channel)

    # Fetch spec
    spec_response = await stub.GetSpec(amplifier_module_pb2.Empty())
    assert spec_response.name == "mock-echo"

    # Create bridge
    bridge = GrpcToolBridge(
        name=spec_response.name,
        description=spec_response.description,
        parameters_json=spec_response.parameters_json,
        endpoint=endpoint,
        channel=channel,
    )
    bridge._stub = stub

    # Execute
    result = await bridge.execute(message="hello world")
    assert result["success"] is True
    assert result["output"]["echoed"] == "hello world"

    # Cleanup
    await bridge.cleanup()


@pytest.mark.asyncio
async def test_grpc_tool_bridge_error_handling(mock_tool_server):
    """Bridge handles gRPC errors gracefully."""
    from amplifier_core._grpc_gen import amplifier_module_pb2_grpc
    from amplifier_core.loader_grpc import GrpcToolBridge

    # Connect to wrong port (the server is on mock_tool_server port)
    channel = grpc_aio.insecure_channel("localhost:1")  # No server here
    stub = amplifier_module_pb2_grpc.ToolServiceStub(channel)

    bridge = GrpcToolBridge(
        name="broken",
        description="broken",
        parameters_json="{}",
        endpoint="localhost:1",
        channel=channel,
    )
    bridge._stub = stub

    # Should return error result, not raise
    result = await bridge.execute(message="hello")
    assert result["success"] is False
    assert result["error"] is not None

    await channel.close()


@pytest.mark.asyncio
async def test_load_grpc_module_full_flow(mock_tool_server):
    """load_grpc_module connects, fetches spec, and returns a mount function."""
    from amplifier_core.loader_grpc import load_grpc_module

    meta = {
        "module": {"name": "mock-echo", "type": "tool", "transport": "grpc"},
        "grpc": {"endpoint": f"localhost:{mock_tool_server}"},
    }

    # Create a minimal mock coordinator
    class MockCoordinator:
        def __init__(self):
            self.mounted_tools = {}

        async def mount(self, mount_point, instance, name=None):
            self.mounted_tools[name or instance.name] = instance

    coord = MockCoordinator()
    mount_fn = await load_grpc_module("mock-echo", {}, meta, coord)

    # Mount the tool
    cleanup = await mount_fn(coord)

    assert "mock-echo" in coord.mounted_tools
    assert coord.mounted_tools["mock-echo"].name == "mock-echo"

    # Cleanup
    if cleanup:
        await cleanup()
