"""gRPC module loader for polyglot Amplifier modules.

Connects to a running gRPC ToolService and wraps it as a Python
Protocol-compatible object that can be mounted on the coordinator
like any Python module.

The gRPC transport uses proto/amplifier_module.proto as the contract.
Any language with gRPC support can implement a tool module.
"""

import json
import logging
from typing import Any

logger = logging.getLogger(__name__)


def _extract_endpoint(meta: dict[str, Any], module_id: str) -> str:
    """Extract gRPC endpoint from module metadata.

    Args:
        meta: Parsed amplifier.toml contents
        module_id: Module identifier for logging

    Returns:
        Endpoint string like "localhost:50051"
    """
    grpc_config = meta.get("grpc", {})
    endpoint = grpc_config.get("endpoint", "localhost:50051")
    logger.debug(f"gRPC endpoint for '{module_id}': {endpoint}")
    return endpoint


class GrpcToolBridge:
    """Wraps a remote gRPC ToolService as a Python tool object.

    From the coordinator's perspective, this is indistinguishable from
    a Python-native tool. It has name, description, get_spec(), and
    execute() -- the same interface as any Python Tool Protocol.

    Args:
        name: Tool name (from GetSpec response)
        description: Tool description (from GetSpec response)
        parameters_json: JSON Schema string (from GetSpec response)
        endpoint: gRPC endpoint string
        channel: grpc.Channel (or None for unit tests)
    """

    def __init__(
        self,
        name: str,
        description: str,
        parameters_json: str,
        endpoint: str,
        channel: Any | None = None,
    ) -> None:
        self._name = name
        self._description = description
        self._parameters_json = parameters_json
        self._endpoint = endpoint
        self._channel = channel
        self._stub: Any | None = None

    @property
    def name(self) -> str:
        return self._name

    @property
    def description(self) -> str:
        return self._description

    def get_spec(self) -> dict[str, Any]:
        """Return tool spec as a dict matching the Python ToolSpec pattern."""
        params = json.loads(self._parameters_json) if self._parameters_json else {}
        return {
            "name": self._name,
            "description": self._description,
            "parameters": params,
        }

    def _serialize_input(self, input_dict: dict[str, Any]) -> tuple[bytes, str]:
        """Serialize tool input to bytes with content type.

        Returns:
            Tuple of (payload_bytes, content_type_string)
        """
        data = json.dumps(input_dict).encode("utf-8")
        return data, "application/json"

    def _deserialize_output(self, output_bytes: bytes, content_type: str) -> Any:
        """Deserialize tool output bytes to Python object.

        Args:
            output_bytes: Raw output payload
            content_type: MIME type of the payload

        Returns:
            Deserialized Python object (dict, list, str, etc.)
        """
        if not output_bytes:
            return {}
        if content_type == "application/json" or not content_type:
            return json.loads(output_bytes.decode("utf-8"))
        # Future: handle application/msgpack
        logger.warning(f"Unknown content type '{content_type}', attempting JSON decode")
        return json.loads(output_bytes.decode("utf-8"))

    async def execute(self, **kwargs: Any) -> dict[str, Any]:
        """Execute the tool via gRPC.

        Args:
            **kwargs: Tool input arguments

        Returns:
            ToolResult-compatible dict with success, output, error keys
        """
        if self._stub is None:
            raise RuntimeError(
                f"gRPC channel not connected for tool '{self._name}'. "
                "Call connect() first or use load_grpc_module()."
            )

        input_bytes, content_type = self._serialize_input(kwargs)

        try:
            # Import proto types lazily to avoid hard dependency
            from amplifier_core._grpc_gen import amplifier_module_pb2

            request = amplifier_module_pb2.ToolExecuteRequest(
                input=input_bytes,
                content_type=content_type,
            )
            response = await self._stub.Execute(request)

            if response.success:
                output = self._deserialize_output(
                    response.output, response.content_type
                )
                return {"success": True, "output": output, "error": None}
            else:
                return {
                    "success": False,
                    "output": None,
                    "error": {"message": response.error},
                }

        except Exception as e:
            logger.error(f"gRPC tool execution failed for '{self._name}': {e}")
            return {"success": False, "output": None, "error": {"message": str(e)}}

    async def cleanup(self) -> None:
        """Close the gRPC channel."""
        if self._channel:
            await self._channel.close()
            logger.debug(f"Closed gRPC channel for tool '{self._name}'")


async def load_grpc_module(
    module_id: str,
    config: dict[str, Any] | None,
    meta: dict[str, Any],
    coordinator: Any,
) -> Any:
    """Load a gRPC module and return a mount function.

    Connects to the gRPC service, fetches the tool spec via GetSpec,
    and returns a mount function compatible with the module loading chain.

    Args:
        module_id: Module identifier
        config: Optional module configuration
        meta: Parsed amplifier.toml contents
        coordinator: The coordinator instance

    Returns:
        Async mount function that registers the tool on the coordinator
    """
    endpoint = _extract_endpoint(meta, module_id)

    try:
        import grpc.aio
    except ImportError:
        raise ImportError(
            "grpcio is required for gRPC module loading. "
            "Install it with: pip install grpcio grpcio-tools"
        )

    # Connect to the gRPC service
    channel = grpc.aio.insecure_channel(endpoint)

    try:
        # Import generated proto stubs
        from amplifier_core._grpc_gen import amplifier_module_pb2
        from amplifier_core._grpc_gen import amplifier_module_pb2_grpc
    except ImportError:
        raise ImportError(
            "gRPC proto stubs not generated. Run: "
            "python -m grpc_tools.protoc -I proto --python_out=python/amplifier_core/_grpc_gen "
            "--grpc_python_out=python/amplifier_core/_grpc_gen proto/amplifier_module.proto"
        )

    stub = amplifier_module_pb2_grpc.ToolServiceStub(channel)

    # Fetch tool spec
    spec_response = await stub.GetSpec(amplifier_module_pb2.Empty())

    # Create bridge
    bridge = GrpcToolBridge(
        name=spec_response.name,
        description=spec_response.description,
        parameters_json=spec_response.parameters_json,
        endpoint=endpoint,
        channel=channel,
    )
    bridge._stub = stub

    logger.info(f"Connected to gRPC tool '{bridge.name}' at {endpoint}")

    # Return mount function matching the Python module loading pattern
    async def mount(coord: Any) -> Any:
        """Mount the gRPC tool bridge on the coordinator."""
        await coord.mount("tools", bridge, name=bridge.name)
        logger.info(f"Mounted gRPC tool '{bridge.name}' on coordinator")
        return bridge.cleanup  # Return cleanup function

    return mount
