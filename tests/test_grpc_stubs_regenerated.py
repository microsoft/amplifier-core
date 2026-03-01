"""Tests that regenerated Python gRPC stubs contain all expected symbols.

Validates that amplifier_module_pb2 has key message types from the expanded
proto (all 8 services), and amplifier_module_pb2_grpc has all service stubs.
"""

import pytest


class TestPb2MessageAttributes:
    """Verify key message types exist in the pb2 module."""

    @pytest.fixture(scope="class")
    def pb2(self):
        from amplifier_core._grpc_gen import amplifier_module_pb2

        return amplifier_module_pb2

    def test_has_tool_spec(self, pb2):
        """ToolSpec message must exist (from ToolService)."""
        assert hasattr(pb2, "ToolSpec"), "pb2 missing ToolSpec"

    def test_has_chat_request(self, pb2):
        """ChatRequest message must exist (from ProviderService)."""
        assert hasattr(pb2, "ChatRequest"), "pb2 missing ChatRequest"

    def test_has_hook_result(self, pb2):
        """HookResult message must exist (from HookService)."""
        assert hasattr(pb2, "HookResult"), "pb2 missing HookResult"


class TestPb2GrpcServiceStubs:
    """Verify all 8 service stubs exist in the pb2_grpc module."""

    @pytest.fixture(scope="class")
    def pb2_grpc(self):
        from amplifier_core._grpc_gen import amplifier_module_pb2_grpc

        return amplifier_module_pb2_grpc

    def test_has_tool_service_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "ToolServiceStub"), "Missing ToolServiceStub"

    def test_has_provider_service_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "ProviderServiceStub"), "Missing ProviderServiceStub"

    def test_has_orchestrator_service_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "OrchestratorServiceStub"), (
            "Missing OrchestratorServiceStub"
        )

    def test_has_context_service_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "ContextServiceStub"), "Missing ContextServiceStub"

    def test_has_hook_service_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "HookServiceStub"), "Missing HookServiceStub"

    def test_has_approval_service_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "ApprovalServiceStub"), "Missing ApprovalServiceStub"

    def test_has_kernel_service_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "KernelServiceStub"), "Missing KernelServiceStub"

    def test_has_module_lifecycle_stub(self, pb2_grpc):
        assert hasattr(pb2_grpc, "ModuleLifecycleStub"), "Missing ModuleLifecycleStub"
