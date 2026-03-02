"""Tests that the proto-generated Python code compiles and exposes expected symbols.

Validates backward compatibility of ToolService messages, completeness of new
service messages, all 8 gRPC service stubs, and enum value assignments.
"""


class TestProtoCompilation:
    """Verify proto-generated modules import cleanly and expose expected symbols."""

    def test_pb2_module_imports(self):
        """The pb2 module imports without error."""
        from amplifier_core._grpc_gen import amplifier_module_pb2  # noqa: F401

    def test_pb2_grpc_module_imports(self):
        """The pb2_grpc module imports without error."""
        from amplifier_core._grpc_gen import amplifier_module_pb2_grpc  # noqa: F401

    def test_tool_service_messages_exist(self):
        """Backward-compat: original ToolService messages are present."""
        from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2

        for name in ("ToolSpec", "ToolExecuteRequest", "ToolExecuteResponse", "Empty"):
            assert hasattr(pb2, name), f"Missing backward-compat message: {name}"

    def test_new_services_messages_exist(self):
        """All 20+ messages added across the new services are present."""
        from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2

        expected_messages = [
            "ModuleInfo",
            "MountRequest",
            "MountResponse",
            "HealthCheckResponse",
            "ConfigField",
            "ProviderError",
            "ToolError",
            "HookError",
            "AmplifierError",
            "ChatRequest",
            "ChatResponse",
            "Message",
            "ContentBlock",
            "Usage",
            "ToolResult",
            "HookResult",
            "ModelInfo",
            "ProviderInfo",
            "ApprovalRequest",
            "ApprovalResponse",
            "CompleteWithProviderRequest",
            "ExecuteToolRequest",
            "EmitHookRequest",
        ]
        assert len(expected_messages) >= 20, "Sanity: list must contain 20+ messages"
        for name in expected_messages:
            assert hasattr(pb2, name), f"Missing message: {name}"

    def test_service_stubs_exist(self):
        """All 8 gRPC service stubs are generated."""
        from amplifier_core._grpc_gen import amplifier_module_pb2_grpc as grpc_mod

        expected_stubs = [
            "ToolServiceStub",
            "ProviderServiceStub",
            "OrchestratorServiceStub",
            "ContextServiceStub",
            "HookServiceStub",
            "ApprovalServiceStub",
            "KernelServiceStub",
            "ModuleLifecycleStub",
        ]
        for name in expected_stubs:
            attr = getattr(grpc_mod, name, None)
            assert attr is not None, f"Missing stub: {name}"
            assert callable(attr), f"Stub {name} is not callable"

    def test_enum_values_present(self):
        """Key enum values have the expected numeric assignments."""
        from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2

        assert pb2.MODULE_TYPE_TOOL == 2  # type: ignore[attr-defined]
        assert pb2.MODULE_TYPE_PROVIDER == 1  # type: ignore[attr-defined]
        assert pb2.HOOK_ACTION_CONTINUE == 1  # type: ignore[attr-defined]
        assert pb2.HOOK_ACTION_DENY == 3  # type: ignore[attr-defined]
        assert pb2.PROVIDER_ERROR_TYPE_RATE_LIMIT == 2  # type: ignore[attr-defined]
        assert pb2.PROVIDER_ERROR_TYPE_TIMEOUT == 7  # type: ignore[attr-defined]
