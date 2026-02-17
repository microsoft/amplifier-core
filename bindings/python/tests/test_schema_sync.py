"""Schema sync tests — verify Rust and Python data models stay in sync.

These tests ensure the Rust _engine module exports match expectations and
that Python Pydantic models can round-trip through JSON (the bridge boundary).
"""

import json


def test_rust_engine_has_version():
    """Verify _engine exposes __version__."""
    from amplifier_core._engine import __version__

    assert __version__ == "1.0.0"


def test_rust_engine_has_types():
    """Verify _engine exposes the four Rust wrapper types."""
    from amplifier_core._engine import (
        RustCancellationToken,
        RustCoordinator,
        RustHookRegistry,
        RustSession,
    )

    assert RustSession is not None
    assert RustHookRegistry is not None
    assert RustCancellationToken is not None
    assert RustCoordinator is not None


def test_hook_result_fields_present():
    """Verify Python HookResult has the fields the Rust side must handle."""
    from amplifier_core import HookResult

    result = HookResult()
    # Core fields
    assert hasattr(result, "action")
    assert hasattr(result, "data")
    assert hasattr(result, "reason")
    # Context injection fields
    assert hasattr(result, "context_injection")
    assert hasattr(result, "context_injection_role")
    assert hasattr(result, "ephemeral")
    # Approval gate fields
    assert hasattr(result, "approval_prompt")
    assert hasattr(result, "approval_options")
    assert hasattr(result, "approval_timeout")
    assert hasattr(result, "approval_default")
    # Output control fields
    assert hasattr(result, "suppress_output")
    assert hasattr(result, "user_message")
    assert hasattr(result, "user_message_level")

    # Verify defaults
    assert result.action == "continue"
    assert result.data is None
    assert result.reason is None


def test_tool_result_fields_present():
    """Verify ToolResult construction and default values."""
    from amplifier_core import ToolResult

    result = ToolResult(output="test")
    assert result.success is True
    assert result.output == "test"
    assert result.error is None


def test_chat_request_serialization():
    """Verify ChatRequest can round-trip through JSON (the bridge boundary)."""
    from amplifier_core import ChatRequest, Message

    request = ChatRequest(
        messages=[Message(role="user", content="hello")],
        model="test-model",
    )
    json_str = request.model_dump_json()
    parsed = json.loads(json_str)
    assert parsed["model"] == "test-model"
    assert len(parsed["messages"]) == 1
    assert parsed["messages"][0]["role"] == "user"


def test_chat_response_serialization():
    """Verify ChatResponse can round-trip through JSON."""
    from amplifier_core import ChatResponse, TextBlock, Usage

    response = ChatResponse(
        content=[TextBlock(text="hi there")],
        usage=Usage(input_tokens=10, output_tokens=5, total_tokens=15),
    )
    json_str = response.model_dump_json()
    parsed = json.loads(json_str)
    assert parsed["content"][0]["type"] == "text"
    assert parsed["content"][0]["text"] == "hi there"
    assert parsed["usage"]["total_tokens"] == 15
    assert parsed["usage"]["input_tokens"] == 10


def test_event_constants_match():
    """Verify Python event constants haven't drifted."""
    from amplifier_core.events import (
        ALL_EVENTS,
        SESSION_START,
        SESSION_END,
        TOOL_PRE,
        TOOL_POST,
        TOOL_ERROR,
        CANCEL_REQUESTED,
        CANCEL_COMPLETED,
    )

    assert SESSION_START == "session:start"
    assert SESSION_END == "session:end"
    assert TOOL_PRE == "tool:pre"
    assert TOOL_POST == "tool:post"
    assert TOOL_ERROR == "tool:error"
    assert CANCEL_REQUESTED == "cancel:requested"
    assert CANCEL_COMPLETED == "cancel:completed"
    assert len(ALL_EVENTS) == 48


def test_hook_result_json_roundtrip():
    """Verify HookResult survives JSON serialization (used at the Rust bridge)."""
    from amplifier_core import HookResult

    original = HookResult(
        action="inject_context",
        context_injection="Lint error on line 42",
        context_injection_role="system",
        suppress_output=True,
        user_message="Found 1 issue",
    )
    json_str = original.model_dump_json()
    parsed = json.loads(json_str)
    restored = HookResult.model_validate(parsed)

    assert restored.action == "inject_context"
    assert restored.context_injection == "Lint error on line 42"
    assert restored.suppress_output is True
    assert restored.user_message == "Found 1 issue"
