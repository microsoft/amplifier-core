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

    def test_proto_tool_result_has_content_blocks(self):
        tr = pb2.ToolResult()
        assert hasattr(tr, "content_blocks")

    def test_proto_tool_result_field_count(self):
        """ToolResult must have four fields, including canonical rich content."""
        tr = pb2.ToolResult()
        fields = [f.name for f in tr.DESCRIPTOR.fields]
        assert fields == ["success", "output_json", "error_json", "content_blocks"]


class TestHookResultEquivalence:
    """Proto HookResult has all 16 fields matching native HookResult."""

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
            "context_injections",
        ]
        assert [field.name for field in hr.DESCRIPTOR.fields] == expected_fields

    def test_proto_hook_result_field_count(self):
        """HookResult must have exactly 16 append-only fields."""
        hr = pb2.HookResult()
        expected_numbers = {
            "action": 1,
            "data_json": 2,
            "reason": 3,
            "context_injection": 4,
            "context_injection_role": 5,
            "ephemeral": 6,
            "approval_prompt": 7,
            "approval_options": 8,
            "approval_timeout": 9,
            "approval_default": 10,
            "suppress_output": 11,
            "user_message": 12,
            "user_message_level": 13,
            "user_message_source": 14,
            "append_to_last_tool_result": 15,
            "context_injections": 16,
        }
        assert {field.name: field.number for field in hr.DESCRIPTOR.fields} == expected_numbers

        injections = hr.DESCRIPTOR.fields_by_name["context_injections"]
        assert injections.is_repeated
        assert injections.type == injections.TYPE_MESSAGE
        assert injections.message_type.full_name == pb2.ContextInjection.DESCRIPTOR.full_name


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
