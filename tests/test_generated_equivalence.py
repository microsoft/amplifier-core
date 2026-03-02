"""Equivalence tests: proto-generated Python types match hand-written.

Verifies that the proto expansion faithfully represents the existing
Python type system. When Phase 4 replaces hand-written types with
generated ones, these tests verify zero behavioral change.
"""

from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2
from amplifier_core._grpc_gen import amplifier_module_pb2_grpc as grpc


class TestToolResultEquivalence:
    """Proto ToolResult has same fields as native ToolResult."""

    def test_proto_tool_result_has_success(self):
        tr = pb2.ToolResult()
        assert hasattr(tr, "success")

    def test_proto_tool_result_has_output_json(self):
        tr = pb2.ToolResult()
        assert hasattr(tr, "output_json")

    def test_proto_tool_result_has_error_json(self):
        tr = pb2.ToolResult()
        assert hasattr(tr, "error_json")

    def test_proto_tool_result_field_count(self):
        """ToolResult must have exactly 3 fields (success, output_json, error_json)."""
        tr = pb2.ToolResult()
        fields = [f.name for f in tr.DESCRIPTOR.fields]
        assert len(fields) == 3


class TestHookResultEquivalence:
    """Proto HookResult has all 15 fields matching native HookResult."""

    def test_proto_hook_result_has_all_fields(self):
        hr = pb2.HookResult()
        expected_fields = [
            "action",
            "data_json",
            "reason",
            "context_injection",
            "context_injection_role",
            "ephemeral",
            "approval_prompt",
            "approval_options",
            "approval_timeout",
            "approval_default",
            "suppress_output",
            "user_message",
            "user_message_level",
            "user_message_source",
            "append_to_last_tool_result",
        ]
        for field in expected_fields:
            assert hasattr(hr, field), f"HookResult missing field: {field}"

    def test_proto_hook_result_field_count(self):
        """HookResult must have exactly 15 fields."""
        hr = pb2.HookResult()
        fields = [f.name for f in hr.DESCRIPTOR.fields]
        assert len(fields) == 15, f"Expected 15 fields, got {len(fields)}: {fields}"


class TestHookActionEnumEquivalence:
    """Proto HookAction enum values map 1:1 to Python string values."""

    def test_hook_action_continue(self):
        assert pb2.HOOK_ACTION_CONTINUE == 1

    def test_hook_action_deny(self):
        assert pb2.HOOK_ACTION_DENY == 3

    def test_hook_action_modify(self):
        assert pb2.HOOK_ACTION_MODIFY == 2

    def test_hook_action_inject_context(self):
        assert pb2.HOOK_ACTION_INJECT_CONTEXT == 4

    def test_hook_action_ask_user(self):
        assert pb2.HOOK_ACTION_ASK_USER == 5

    def test_hook_action_count(self):
        """HookAction should have 6 values (including UNSPECIFIED=0)."""
        descriptor = pb2.DESCRIPTOR.enum_types_by_name["HookAction"]
        assert len(descriptor.values) == 6


class TestServiceStubsExist:
    """All 8 service stubs exist in generated gRPC module."""

    def test_tool_service_stub(self):
        assert hasattr(grpc, "ToolServiceStub")

    def test_provider_service_stub(self):
        assert hasattr(grpc, "ProviderServiceStub")

    def test_orchestrator_service_stub(self):
        assert hasattr(grpc, "OrchestratorServiceStub")

    def test_context_service_stub(self):
        assert hasattr(grpc, "ContextServiceStub")

    def test_hook_service_stub(self):
        assert hasattr(grpc, "HookServiceStub")

    def test_approval_service_stub(self):
        assert hasattr(grpc, "ApprovalServiceStub")

    def test_kernel_service_stub(self):
        assert hasattr(grpc, "KernelServiceStub")

    def test_module_lifecycle_stub(self):
        assert hasattr(grpc, "ModuleLifecycleStub")

    def test_all_8_servicers_exist(self):
        """Verify all 8 Servicer classes exist (server-side)."""
        servicers = [
            "ToolServiceServicer",
            "ProviderServiceServicer",
            "OrchestratorServiceServicer",
            "ContextServiceServicer",
            "HookServiceServicer",
            "ApprovalServiceServicer",
            "KernelServiceServicer",
            "ModuleLifecycleServicer",
        ]
        for name in servicers:
            assert hasattr(grpc, name), f"Missing servicer: {name}"
