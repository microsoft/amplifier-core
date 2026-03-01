"""Tests for module service definitions in amplifier_module.proto.

Validates that all 5 module services are defined with correct RPCs,
request/response messages, and streaming annotations.
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


def _service_body(proto_text: str, service_name: str) -> str:
    """Extract the body of a named service block for scoped RPC matching."""
    match = re.search(rf"service {service_name}\s*\{{(.*?)\}}", proto_text, re.DOTALL)
    if not match:
        raise ValueError(f"{service_name} block not found")
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


class TestProtoCompiles:
    def test_proto_compiles_with_exit_code_0(self):
        result = _compile_proto()
        assert result.returncode == 0, (
            f"Proto compilation failed (exit {result.returncode}):\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )


class TestProviderService:
    """ProviderService with 5 RPCs: GetInfo, ListModels, Complete, CompleteStreaming, ParseToolCalls."""

    def test_provider_service_exists(self, proto_text: str):
        assert "service ProviderService" in proto_text

    def test_provider_service_get_info_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ProviderService")
        assert re.search(
            r"rpc\s+GetInfo\s*\(\s*Empty\s*\)\s+returns\s*\(\s*ProviderInfo\s*\)", body
        )

    def test_provider_service_list_models_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ProviderService")
        assert re.search(
            r"rpc\s+ListModels\s*\(\s*Empty\s*\)\s+returns\s*\(\s*ListModelsResponse\s*\)",
            body,
        )

    def test_provider_service_complete_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ProviderService")
        assert re.search(
            r"rpc\s+Complete\s*\(\s*ChatRequest\s*\)\s+returns\s*\(\s*ChatResponse\s*\)",
            body,
        )

    def test_provider_service_complete_streaming_is_server_stream(
        self, proto_text: str
    ):
        body = _service_body(proto_text, "ProviderService")
        assert re.search(
            r"rpc\s+CompleteStreaming\s*\(\s*ChatRequest\s*\)\s+returns\s*\(\s*stream\s+ChatResponse\s*\)",
            body,
        )

    def test_provider_service_parse_tool_calls_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ProviderService")
        assert re.search(
            r"rpc\s+ParseToolCalls\s*\(\s*ChatResponse\s*\)\s+returns\s*\(\s*ParseToolCallsResponse\s*\)",
            body,
        )

    def test_list_models_response_message(self, proto_text: str):
        assert "message ListModelsResponse" in proto_text
        # Should contain repeated ModelInfo
        match = re.search(r"message ListModelsResponse\s*\{([^}]+)\}", proto_text)
        assert match, "ListModelsResponse message body not found"
        assert "repeated ModelInfo" in match.group(1)

    def test_parse_tool_calls_response_message(self, proto_text: str):
        assert "message ParseToolCallsResponse" in proto_text
        match = re.search(r"message ParseToolCallsResponse\s*\{([^}]+)\}", proto_text)
        assert match, "ParseToolCallsResponse message body not found"
        assert "repeated ToolCallMessage" in match.group(1)


class TestOrchestratorService:
    """OrchestratorService with 1 RPC: Execute."""

    def test_orchestrator_service_exists(self, proto_text: str):
        assert "service OrchestratorService" in proto_text

    def test_orchestrator_execute_rpc(self, proto_text: str):
        body = _service_body(proto_text, "OrchestratorService")
        assert re.search(
            r"rpc\s+Execute\s*\(\s*OrchestratorExecuteRequest\s*\)\s+returns\s*\(\s*OrchestratorExecuteResponse\s*\)",
            body,
        )

    def test_orchestrator_execute_request_message(self, proto_text: str):
        assert "message OrchestratorExecuteRequest" in proto_text
        match = re.search(
            r"message OrchestratorExecuteRequest\s*\{([^}]+)\}", proto_text
        )
        assert match, "OrchestratorExecuteRequest body not found"
        body = match.group(1)
        assert "string prompt" in body
        assert "string session_id" in body

    def test_orchestrator_execute_response_message(self, proto_text: str):
        assert "message OrchestratorExecuteResponse" in proto_text
        match = re.search(
            r"message OrchestratorExecuteResponse\s*\{([^}]+)\}", proto_text
        )
        assert match, "OrchestratorExecuteResponse body not found"
        body = match.group(1)
        assert "string response" in body
        assert "string error" in body


class TestContextService:
    """ContextService with 5 RPCs: AddMessage, GetMessages, GetMessagesForRequest, SetMessages, Clear."""

    def test_context_service_exists(self, proto_text: str):
        assert "service ContextService" in proto_text

    def test_context_service_has_5_rpcs(self, proto_text: str):
        body = _service_body(proto_text, "ContextService")
        rpcs = re.findall(r"rpc\s+\w+", body)
        assert len(rpcs) == 5, f"Expected 5 RPCs, found {len(rpcs)}: {rpcs}"

    def test_context_service_add_message_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ContextService")
        assert re.search(
            r"rpc\s+AddMessage\s*\(\s*AddMessageRequest\s*\)\s+returns\s*\(\s*Empty\s*\)",
            body,
        )

    def test_context_service_get_messages_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ContextService")
        assert re.search(
            r"rpc\s+GetMessages\s*\(\s*Empty\s*\)\s+returns\s*\(\s*GetMessagesResponse\s*\)",
            body,
        )

    def test_context_service_get_messages_for_request_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ContextService")
        assert re.search(
            r"rpc\s+GetMessagesForRequest\s*\(\s*GetMessagesForRequestParams\s*\)\s+returns\s*\(\s*GetMessagesResponse\s*\)",
            body,
        )

    def test_context_service_set_messages_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ContextService")
        assert re.search(
            r"rpc\s+SetMessages\s*\(\s*SetMessagesRequest\s*\)\s+returns\s*\(\s*Empty\s*\)",
            body,
        )

    def test_context_service_clear_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ContextService")
        assert re.search(
            r"rpc\s+Clear\s*\(\s*Empty\s*\)\s+returns\s*\(\s*Empty\s*\)", body
        )

    def test_add_message_request_message(self, proto_text: str):
        assert "message AddMessageRequest" in proto_text
        match = re.search(r"message AddMessageRequest\s*\{([^}]+)\}", proto_text)
        assert match, "AddMessageRequest body not found"
        assert "Message" in match.group(1)

    def test_get_messages_response_message(self, proto_text: str):
        assert "message GetMessagesResponse" in proto_text
        match = re.search(r"message GetMessagesResponse\s*\{([^}]+)\}", proto_text)
        assert match, "GetMessagesResponse body not found"
        assert "repeated Message" in match.group(1)

    def test_get_messages_for_request_params_message(self, proto_text: str):
        assert "message GetMessagesForRequestParams" in proto_text
        match = re.search(
            r"message GetMessagesForRequestParams\s*\{([^}]+)\}", proto_text
        )
        assert match, "GetMessagesForRequestParams body not found"
        body = match.group(1)
        assert "int32 token_budget" in body or "token_budget" in body
        assert "string provider_name" in body

    def test_set_messages_request_message(self, proto_text: str):
        assert "message SetMessagesRequest" in proto_text
        match = re.search(r"message SetMessagesRequest\s*\{([^}]+)\}", proto_text)
        assert match, "SetMessagesRequest body not found"
        assert "repeated Message" in match.group(1)


class TestHookService:
    """HookService with 1 RPC: Handle."""

    def test_hook_service_exists(self, proto_text: str):
        assert "service HookService" in proto_text

    def test_hook_handle_rpc(self, proto_text: str):
        body = _service_body(proto_text, "HookService")
        assert re.search(
            r"rpc\s+Handle\s*\(\s*HookHandleRequest\s*\)\s+returns\s*\(\s*HookResult\s*\)",
            body,
        )

    def test_hook_handle_request_message(self, proto_text: str):
        assert "message HookHandleRequest" in proto_text
        match = re.search(r"message HookHandleRequest\s*\{([^}]+)\}", proto_text)
        assert match, "HookHandleRequest body not found"
        body = match.group(1)
        assert "string event" in body
        assert "string data_json" in body


class TestApprovalService:
    """ApprovalService with 1 RPC: RequestApproval."""

    def test_approval_service_exists(self, proto_text: str):
        assert "service ApprovalService" in proto_text

    def test_request_approval_rpc(self, proto_text: str):
        body = _service_body(proto_text, "ApprovalService")
        assert re.search(
            r"rpc\s+RequestApproval\s*\(\s*ApprovalRequest\s*\)\s+returns\s*\(\s*ApprovalResponse\s*\)",
            body,
        )


class TestToolServiceUnchanged:
    """Existing ToolService remains as-is."""

    def test_tool_service_still_exists(self, proto_text: str):
        assert "service ToolService" in proto_text

    def test_tool_service_has_get_spec(self, proto_text: str):
        body = _service_body(proto_text, "ToolService")
        assert re.search(
            r"rpc\s+GetSpec\s*\(\s*Empty\s*\)\s+returns\s*\(\s*ToolSpec\s*\)", body
        )

    def test_tool_service_has_execute(self, proto_text: str):
        body = _service_body(proto_text, "ToolService")
        assert re.search(
            r"rpc\s+Execute\s*\(\s*ToolExecuteRequest\s*\)\s+returns\s*\(\s*ToolExecuteResponse\s*\)",
            body,
        )
