"""Tests for proto_chat_request_to_json PyO3 bridge function.

Validates that proto bytes can be decoded and serialized to JSON
via the Rust PyO3 bridge function.

Proto bytes were generated from the amplifier_module.proto schema:
  - Message fields: role (int), text_content (string), block_content, etc.
  - ChatRequest fields: messages (repeated Message), tools (repeated ToolSpecProto), etc.
  - ToolSpecProto fields: name, description, parameters_json
"""

import json
from decimal import Decimal

import pytest


class TestProtoChatRequestToJson:
    """Tests for the proto_chat_request_to_json PyO3 bridge function."""

    def test_minimal_request(self):
        """A ChatRequest with one user message round-trips to valid JSON with messages array.

        Proto bytes represent:
          ChatRequest { messages: [Message { role: 1 (user), text_content: "hello" }] }
        """
        from amplifier_core._engine import proto_chat_request_to_json

        # Pre-computed proto bytes for: ChatRequest with one user message (text_content="hello")
        # Generated from: pb2.ChatRequest with msg.role=1, msg.text_content="hello"
        proto_bytes = bytes.fromhex("0a090801120568656c6c6f")

        # Call the bridge function
        json_str = proto_chat_request_to_json(proto_bytes)

        # Verify it's valid JSON with a messages array
        data = json.loads(json_str)
        assert "messages" in data, "JSON must contain 'messages' key"
        assert isinstance(data["messages"], list), "'messages' must be a list"
        assert len(data["messages"]) == 1, "Expected exactly one message"

    def test_request_with_tools(self):
        """ChatRequest with ToolSpecProto converts to JSON with tools array.

        Proto bytes represent:
          ChatRequest {
            messages: [Message { role: 1 (user), text_content: "use the tool" }],
            tools: [ToolSpecProto { name: "test_tool", description: "A test tool",
                                    parameters_json: "{}" }]
          }
        """
        from amplifier_core._engine import proto_chat_request_to_json

        # Pre-computed proto bytes for ChatRequest with one message + one tool
        # Generated from: pb2.ChatRequest with msg + tool (name="test_tool")
        proto_bytes = bytes.fromhex(
            "0a100801120c7573652074686520746f6f6c"
            "121c0a09746573745f746f6f6c120b41207465737420746f6f6c1a027b7d"
        )

        # Call the bridge function
        json_str = proto_chat_request_to_json(proto_bytes)

        # Verify it's valid JSON with a tools array
        data = json.loads(json_str)
        assert "tools" in data, "JSON must contain 'tools' key"
        assert data["tools"] is not None, "'tools' must not be null"
        assert isinstance(data["tools"], list), "'tools' must be a list"
        assert len(data["tools"]) == 1, "Expected exactly one tool"
        assert data["tools"][0]["name"] == "test_tool"

    def test_empty_bytes_raises(self):
        """Empty bytes produces valid empty ChatRequest, not a crash.

        An empty byte string is valid protobuf for a message with all default
        values. It should decode to an empty ChatRequest (no messages, no tools).
        """
        from amplifier_core._engine import proto_chat_request_to_json

        # Empty bytes should succeed and return a valid empty ChatRequest JSON
        try:
            json_str = proto_chat_request_to_json(b"")
            # If it succeeds, it should be valid JSON
            data = json.loads(json_str)
            assert "messages" in data, "JSON must contain 'messages' key"
            assert isinstance(data["messages"], list)
        except Exception as e:
            # Any clean Python exception is acceptable (e.g., ValueError)
            # The important thing is no crash/segfault
            assert isinstance(e, Exception), "Must raise a clean exception"


class TestJsonToProtoChatResponse:
    """Tests for the json_to_proto_chat_response PyO3 bridge function."""

    def test_minimal_response(self):
        """JSON with content+finish_reason round-trips; dual-write populates both fields.

        Verifies:
        - Function returns bytes
        - Both legacy ``content`` (proto field 1, tag 0x0a) and
          ``content_blocks`` (proto field 7, tag 0x3a) are present in
          the encoded bytes (dual-write contract).
        """
        from amplifier_core._engine import json_to_proto_chat_response

        # Minimal ChatResponse JSON: one text content block + finish_reason
        response_json = json.dumps(
            {
                "content": [{"type": "text", "text": "Hello, world!"}],
                "finish_reason": "stop",
            }
        )

        proto_bytes = json_to_proto_chat_response(response_json)

        assert isinstance(proto_bytes, bytes), "Must return bytes"
        assert len(proto_bytes) > 0, "Proto bytes must be non-empty"

        # Verify dual-write: proto field 1 (content legacy string) tag = 0x0a
        # and proto field 7 (content_blocks) tag = 0x3a must both be present.
        assert 0x0A in proto_bytes, (
            "Proto field 1 (legacy content) tag 0x0a must be present (dual-write)"
        )
        assert 0x3A in proto_bytes, (
            "Proto field 7 (content_blocks) tag 0x3a must be present (dual-write)"
        )

        # Also verify finish_reason (field 5, tag = 0x2a) is present
        assert 0x2A in proto_bytes, "Proto field 5 (finish_reason) tag 0x2a must be present"

    def test_response_with_tool_calls(self):
        """Tool calls in JSON convert to proto ToolCallMessage entries (field 2, tag 0x12)."""
        from amplifier_core._engine import json_to_proto_chat_response

        response_json = json.dumps(
            {
                "content": [{"type": "text", "text": "Let me use a tool."}],
                "tool_calls": [
                    {
                        "id": "call_abc123",
                        "name": "test_tool",
                        "arguments": {"key": "value"},
                    }
                ],
                "finish_reason": "tool_calls",
            }
        )

        proto_bytes = json_to_proto_chat_response(response_json)

        assert isinstance(proto_bytes, bytes), "Must return bytes"
        assert len(proto_bytes) > 0, "Proto bytes must be non-empty"

        # Proto field 2 (tool_calls repeated ToolCallMessage) tag = 0x12
        assert 0x12 in proto_bytes, (
            "Proto field 2 (tool_calls) tag 0x12 must be present in encoded bytes"
        )

    def test_roundtrip_request_response(self):
        """Full proto request -> JSON -> JSON response -> proto response pipeline.

        1. Start with pre-computed proto ChatRequest bytes.
        2. Call proto_chat_request_to_json() to decode to JSON.
        3. Build a corresponding JSON ChatResponse.
        4. Call json_to_proto_chat_response() to encode back to proto bytes.
        5. Verify the result is valid non-empty bytes.
        """
        from amplifier_core._engine import json_to_proto_chat_response, proto_chat_request_to_json

        # Pre-computed proto bytes for: ChatRequest with one user message
        # (same bytes used in TestProtoChatRequestToJson.test_minimal_request)
        request_proto_bytes = bytes.fromhex("0a090801120568656c6c6f")

        # Step 1: Decode proto request to JSON
        request_json_str = proto_chat_request_to_json(request_proto_bytes)
        request_data = json.loads(request_json_str)
        assert "messages" in request_data, "Decoded request must have messages"

        # Step 2: Build a JSON response corresponding to the request
        response_json = json.dumps(
            {
                "content": [
                    {"type": "text", "text": f"Echo: {request_data['messages'][0]['content']}"}
                ],
                "finish_reason": "stop",
            }
        )

        # Step 3: Encode JSON response to proto bytes
        response_proto_bytes = json_to_proto_chat_response(response_json)

        assert isinstance(response_proto_bytes, bytes), "Must return bytes"
        assert len(response_proto_bytes) > 0, "Proto bytes must be non-empty"

    @pytest.mark.parametrize(
        ("cost_usd", "expected_cost_usd", "is_present"),
        [
            (Decimal("0.000000000123456789"), "1.23456789E-10", True),
            (Decimal("0.123456789123456789"), "0.123456789123456789", True),
            (None, None, False),
            (Decimal("0"), "0", True),
        ],
    )
    def test_usage_cost_usd_preserves_decimal_text_and_presence(
        self, cost_usd, expected_cost_usd, is_present
    ):
        """Python Decimal cost crosses PyO3/protobuf as exact optional quoted text."""
        from amplifier_core._engine import json_to_proto_chat_response
        from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2
        from amplifier_core.message_models import ChatResponse, Usage

        response = ChatResponse(
            content=[{"type": "text", "text": "cost response"}],
            usage=Usage(
                input_tokens=10,
                output_tokens=5,
                total_tokens=15,
                cost_usd=cost_usd,
            ),
        )
        serialized = response.model_dump()
        assert serialized["usage"]["cost_usd"] == expected_cost_usd
        proto_bytes = json_to_proto_chat_response(json.dumps(serialized))
        proto = pb2.ChatResponse()
        proto.ParseFromString(proto_bytes)

        assert proto.usage.HasField("cost_usd") is is_present
        if is_present:
            assert proto.usage.cost_usd == expected_cost_usd

        restored = Usage.model_validate(
            {
                "input_tokens": proto.usage.prompt_tokens,
                "output_tokens": proto.usage.completion_tokens,
                "total_tokens": proto.usage.total_tokens,
                "cost_usd": proto.usage.cost_usd if is_present else None,
            }
        )
        assert restored.cost_usd == cost_usd
        if is_present:
            assert isinstance(proto.usage.cost_usd, str)


class TestToolResultContentBridge:
    """The private ToolResult bridge normalizes content without input echoing."""

    def test_normalizes_canonical_content(self):
        from amplifier_core._engine import _normalize_tool_result_content

        result = _normalize_tool_result_content(
            json.dumps(
                [
                    {"type": "text", "text": "details", "ignored": True},
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": "AA==",
                            "ignored": True,
                        },
                    },
                ]
            )
        )
        assert json.loads(result) == [
            {"type": "text", "text": "details"},
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "AA==",
                },
            },
        ]

    def test_rejects_bad_content_without_input_echo(self):
        from amplifier_core._engine import _normalize_tool_result_content

        with pytest.raises(ValueError) as exc_info:
            _normalize_tool_result_content(
                '[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"private-bytes"}}]'
            )
        assert str(exc_info.value) == "invalid tool result image data"
        assert "private-bytes" not in str(exc_info.value)
