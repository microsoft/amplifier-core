"""Tests for the gRPC module loader."""

import json


def test_grpc_loader_module_exists():
    """The loader_grpc module is importable."""
    from amplifier_core import loader_grpc

    assert hasattr(loader_grpc, "GrpcToolBridge")
    assert hasattr(loader_grpc, "load_grpc_module")


def test_grpc_tool_bridge_init():
    """GrpcToolBridge can be constructed with spec data."""
    from amplifier_core.loader_grpc import GrpcToolBridge

    bridge = GrpcToolBridge(
        name="test-tool",
        description="A test tool",
        parameters_json='{"type": "object", "properties": {"query": {"type": "string"}}}',
        endpoint="localhost:50052",
        channel=None,  # No real connection in unit tests
    )
    assert bridge.name == "test-tool"
    assert bridge.description == "A test tool"


def test_grpc_tool_bridge_get_spec():
    """GrpcToolBridge.get_spec() returns a dict with name, description, parameters."""
    from amplifier_core.loader_grpc import GrpcToolBridge

    bridge = GrpcToolBridge(
        name="search",
        description="Search the web",
        parameters_json='{"type": "object", "properties": {"query": {"type": "string"}}}',
        endpoint="localhost:50052",
        channel=None,
    )
    spec = bridge.get_spec()
    assert spec["name"] == "search"
    assert spec["description"] == "Search the web"
    assert "properties" in spec["parameters"]


def test_grpc_tool_bridge_serialize_input():
    """GrpcToolBridge._serialize_input encodes dict to JSON bytes."""
    from amplifier_core.loader_grpc import GrpcToolBridge

    bridge = GrpcToolBridge(
        name="test",
        description="test",
        parameters_json="{}",
        endpoint="localhost:50052",
        channel=None,
    )
    input_dict = {"query": "hello world"}
    data, content_type = bridge._serialize_input(input_dict)
    assert content_type == "application/json"
    assert json.loads(data) == {"query": "hello world"}


def test_grpc_tool_bridge_deserialize_output():
    """GrpcToolBridge._deserialize_output decodes JSON bytes to dict."""
    from amplifier_core.loader_grpc import GrpcToolBridge

    bridge = GrpcToolBridge(
        name="test",
        description="test",
        parameters_json="{}",
        endpoint="localhost:50052",
        channel=None,
    )
    output_bytes = json.dumps({"result": "found it"}).encode("utf-8")
    result = bridge._deserialize_output(output_bytes, "application/json")
    assert result == {"result": "found it"}


def test_grpc_tool_bridge_deserialize_empty_output():
    """Empty output bytes returns empty dict."""
    from amplifier_core.loader_grpc import GrpcToolBridge

    bridge = GrpcToolBridge(
        name="test",
        description="test",
        parameters_json="{}",
        endpoint="localhost:50052",
        channel=None,
    )
    result = bridge._deserialize_output(b"", "application/json")
    assert result == {}


def test_load_grpc_module_reads_endpoint():
    """load_grpc_module extracts endpoint from meta dict."""
    from amplifier_core.loader_grpc import _extract_endpoint

    meta = {
        "module": {"name": "my-tool", "type": "tool", "transport": "grpc"},
        "grpc": {"endpoint": "localhost:50099"},
    }
    endpoint = _extract_endpoint(meta, "my-tool")
    assert endpoint == "localhost:50099"


def test_load_grpc_module_default_endpoint():
    """When no endpoint specified, uses default localhost:50051."""
    from amplifier_core.loader_grpc import _extract_endpoint

    meta = {
        "module": {"name": "my-tool", "type": "tool", "transport": "grpc"},
    }
    endpoint = _extract_endpoint(meta, "my-tool")
    assert endpoint == "localhost:50051"
