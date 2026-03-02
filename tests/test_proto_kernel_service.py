"""Tests for KernelService definition in amplifier_module.proto.

Validates that KernelService is defined with 10 RPCs, correct
request/response messages, and streaming annotations.
KernelService is hosted by the Rust kernel, called by out-of-process modules.
"""

import re
import subprocess
import sys
from pathlib import Path

import pytest

PROTO_PATH = Path(__file__).parent.parent / "proto" / "amplifier_module.proto"


@pytest.fixture(scope="module")
def proto_text() -> str:
    """Read proto file once per test module."""
    return PROTO_PATH.read_text()


@pytest.fixture(scope="module")
def kernel_service_body(proto_text: str) -> str:
    """Extract the KernelService block body for scoped RPC matching."""
    match = re.search(r"service KernelService\s*\{(.*?)\}", proto_text, re.DOTALL)
    if not match:
        raise ValueError("KernelService block not found")
    return match.group(1)


def _compile_proto() -> subprocess.CompletedProcess[str]:
    """Compile the proto file using protoc and return the result."""
    proto_dir = PROTO_PATH.parent
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            f"--proto_path={proto_dir}",
            f"--python_out={proto_dir}",
            f"--grpc_python_out={proto_dir}",
            str(PROTO_PATH.name),
        ],
        capture_output=True,
        text=True,
        cwd=str(proto_dir),
    )
    return result


class TestProtoStillCompiles:
    def test_proto_compiles_with_exit_code_0(self):
        result = _compile_proto()
        assert result.returncode == 0, (
            f"Proto compilation failed (exit {result.returncode}):\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )


class TestKernelServiceExists:
    """KernelService must be defined after module services."""

    def test_kernel_service_exists(self, proto_text: str):
        assert "service KernelService" in proto_text

    def test_kernel_service_has_10_rpcs(self, kernel_service_body: str):
        rpcs = re.findall(r"rpc\s+\w+", kernel_service_body)
        assert len(rpcs) == 10, f"Expected 10 RPCs, found {len(rpcs)}: {rpcs}"

    def test_kernel_service_after_module_services(self, proto_text: str):
        # KernelService should appear after ApprovalService (the last module service)
        approval_pos = proto_text.find("service ApprovalService")
        kernel_pos = proto_text.find("service KernelService")
        assert approval_pos >= 0, "ApprovalService not found"
        assert kernel_pos >= 0, "KernelService not found"
        assert kernel_pos > approval_pos, (
            "KernelService should appear after module services"
        )


class TestKernelServiceRPCs:
    """Each of the 10 RPCs with correct signatures."""

    def test_complete_with_provider_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+CompleteWithProvider\s*\(\s*CompleteWithProviderRequest\s*\)\s+returns\s*\(\s*ChatResponse\s*\)",
            kernel_service_body,
        )

    def test_complete_with_provider_streaming_is_server_stream(
        self, kernel_service_body: str
    ):
        assert re.search(
            r"rpc\s+CompleteWithProviderStreaming\s*\(\s*CompleteWithProviderRequest\s*\)\s+returns\s*\(\s*stream\s+ChatResponse\s*\)",
            kernel_service_body,
        )

    def test_execute_tool_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+ExecuteTool\s*\(\s*ExecuteToolRequest\s*\)\s+returns\s*\(\s*ToolResult\s*\)",
            kernel_service_body,
        )

    def test_emit_hook_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+EmitHook\s*\(\s*EmitHookRequest\s*\)\s+returns\s*\(\s*HookResult\s*\)",
            kernel_service_body,
        )

    def test_emit_hook_and_collect_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+EmitHookAndCollect\s*\(\s*EmitHookAndCollectRequest\s*\)\s+returns\s*\(\s*EmitHookAndCollectResponse\s*\)",
            kernel_service_body,
        )

    def test_get_messages_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+GetMessages\s*\(\s*GetMessagesRequest\s*\)\s+returns\s*\(\s*GetMessagesResponse\s*\)",
            kernel_service_body,
        )

    def test_add_message_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+AddMessage\s*\(\s*KernelAddMessageRequest\s*\)\s+returns\s*\(\s*Empty\s*\)",
            kernel_service_body,
        )

    def test_get_mounted_module_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+GetMountedModule\s*\(\s*GetMountedModuleRequest\s*\)\s+returns\s*\(\s*GetMountedModuleResponse\s*\)",
            kernel_service_body,
        )

    def test_register_capability_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+RegisterCapability\s*\(\s*RegisterCapabilityRequest\s*\)\s+returns\s*\(\s*Empty\s*\)",
            kernel_service_body,
        )

    def test_get_capability_rpc(self, kernel_service_body: str):
        assert re.search(
            r"rpc\s+GetCapability\s*\(\s*GetCapabilityRequest\s*\)\s+returns\s*\(\s*GetCapabilityResponse\s*\)",
            kernel_service_body,
        )


class TestKernelServiceMessages:
    """All request/response messages for KernelService RPCs."""

    def test_complete_with_provider_request_message(self, proto_text: str):
        assert "message CompleteWithProviderRequest" in proto_text
        match = re.search(
            r"message CompleteWithProviderRequest\s*\{([^}]+)\}", proto_text
        )
        assert match, "CompleteWithProviderRequest body not found"
        body = match.group(1)
        assert "string provider_name" in body
        assert "ChatRequest request" in body

    def test_execute_tool_request_message(self, proto_text: str):
        assert "message ExecuteToolRequest" in proto_text
        match = re.search(r"message ExecuteToolRequest\s*\{([^}]+)\}", proto_text)
        assert match, "ExecuteToolRequest body not found"
        body = match.group(1)
        assert "string tool_name" in body
        assert "string input_json" in body

    def test_emit_hook_request_message(self, proto_text: str):
        assert "message EmitHookRequest" in proto_text
        match = re.search(r"message EmitHookRequest\s*\{([^}]+)\}", proto_text)
        assert match, "EmitHookRequest body not found"
        body = match.group(1)
        assert "string event" in body
        assert "string data_json" in body

    def test_emit_hook_and_collect_request_message(self, proto_text: str):
        assert "message EmitHookAndCollectRequest" in proto_text
        match = re.search(
            r"message EmitHookAndCollectRequest\s*\{([^}]+)\}", proto_text
        )
        assert match, "EmitHookAndCollectRequest body not found"
        body = match.group(1)
        assert "string event" in body
        assert "string data_json" in body
        assert "double timeout_seconds" in body

    def test_emit_hook_and_collect_response_message(self, proto_text: str):
        assert "message EmitHookAndCollectResponse" in proto_text
        match = re.search(
            r"message EmitHookAndCollectResponse\s*\{([^}]+)\}", proto_text
        )
        assert match, "EmitHookAndCollectResponse body not found"
        body = match.group(1)
        assert "repeated string responses_json" in body

    def test_get_messages_request_message(self, proto_text: str):
        assert "message GetMessagesRequest" in proto_text
        match = re.search(r"message GetMessagesRequest\s*\{([^}]+)\}", proto_text)
        assert match, "GetMessagesRequest body not found"
        body = match.group(1)
        assert "string session_id" in body

    def test_kernel_add_message_request_message(self, proto_text: str):
        assert "message KernelAddMessageRequest" in proto_text
        match = re.search(r"message KernelAddMessageRequest\s*\{([^}]+)\}", proto_text)
        assert match, "KernelAddMessageRequest body not found"
        body = match.group(1)
        assert "string session_id" in body
        assert "Message message" in body

    def test_get_mounted_module_request_message(self, proto_text: str):
        assert "message GetMountedModuleRequest" in proto_text
        match = re.search(r"message GetMountedModuleRequest\s*\{([^}]+)\}", proto_text)
        assert match, "GetMountedModuleRequest body not found"
        body = match.group(1)
        assert "string module_name" in body
        assert "ModuleType module_type" in body

    def test_get_mounted_module_response_message(self, proto_text: str):
        assert "message GetMountedModuleResponse" in proto_text
        match = re.search(r"message GetMountedModuleResponse\s*\{([^}]+)\}", proto_text)
        assert match, "GetMountedModuleResponse body not found"
        body = match.group(1)
        assert "bool found" in body
        assert "ModuleInfo info" in body

    def test_register_capability_request_message(self, proto_text: str):
        assert "message RegisterCapabilityRequest" in proto_text
        match = re.search(
            r"message RegisterCapabilityRequest\s*\{([^}]+)\}", proto_text
        )
        assert match, "RegisterCapabilityRequest body not found"
        body = match.group(1)
        assert "string name" in body
        assert "string value_json" in body

    def test_get_capability_request_message(self, proto_text: str):
        assert "message GetCapabilityRequest" in proto_text
        match = re.search(r"message GetCapabilityRequest\s*\{([^}]+)\}", proto_text)
        assert match, "GetCapabilityRequest body not found"
        body = match.group(1)
        assert "string name" in body

    def test_get_capability_response_message(self, proto_text: str):
        assert "message GetCapabilityResponse" in proto_text
        match = re.search(r"message GetCapabilityResponse\s*\{([^}]+)\}", proto_text)
        assert match, "GetCapabilityResponse body not found"
        body = match.group(1)
        assert "bool found" in body
        assert "string value_json" in body


class TestExistingServicesUnchanged:
    """Existing module services must remain intact."""

    def test_tool_service_still_exists(self, proto_text: str):
        assert "service ToolService" in proto_text

    def test_provider_service_still_exists(self, proto_text: str):
        assert "service ProviderService" in proto_text

    def test_orchestrator_service_still_exists(self, proto_text: str):
        assert "service OrchestratorService" in proto_text

    def test_context_service_still_exists(self, proto_text: str):
        assert "service ContextService" in proto_text

    def test_hook_service_still_exists(self, proto_text: str):
        assert "service HookService" in proto_text

    def test_approval_service_still_exists(self, proto_text: str):
        assert "service ApprovalService" in proto_text
