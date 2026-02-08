"""Tests for REQUEST_ENVELOPE_V1 Pydantic models."""

import pytest
from amplifier_core.message_models import ChatRequest
from amplifier_core.message_models import ChatResponse
from amplifier_core.message_models import Degradation
from amplifier_core.message_models import ImageBlock
from amplifier_core.message_models import Message
from amplifier_core.message_models import ReasoningBlock
from amplifier_core.message_models import RedactedThinkingBlock
from amplifier_core.message_models import ResponseFormatJson
from amplifier_core.message_models import ResponseFormatJsonSchema
from amplifier_core.message_models import ResponseFormatText
from amplifier_core.message_models import TextBlock
from amplifier_core.message_models import ThinkingBlock
from amplifier_core.message_models import ToolCall
from amplifier_core.message_models import ToolCallBlock
from amplifier_core.message_models import ToolResultBlock
from amplifier_core.message_models import ToolSpec
from amplifier_core.message_models import Usage
from pydantic import ValidationError


class TestMessage:
    """Tests for Message model."""

    def test_system_message_string_content(self) -> None:
        """System message with string content."""
        msg = Message(role="system", content="You are a helpful assistant.")

        assert msg.role == "system"
        assert msg.content == "You are a helpful assistant."
        assert msg.name is None
        assert msg.tool_call_id is None

    def test_user_message_string_content(self) -> None:
        """User message with string content."""
        msg = Message(role="user", content="Hello!")

        assert msg.role == "user"
        assert msg.content == "Hello!"

    def test_developer_message(self) -> None:
        """Developer message with string content."""
        msg = Message(role="developer", content="Context file content")

        assert msg.role == "developer"
        assert msg.content == "Context file content"

    def test_assistant_message_with_blocks(self) -> None:
        """Assistant message with content blocks."""
        msg = Message(
            role="assistant",
            content=[
                TextBlock(text="I'll help with that."),
                ToolCallBlock(id="call_1", name="search", input={"query": "test"}),
            ],
        )

        assert msg.role == "assistant"
        assert isinstance(msg.content, list)
        assert len(msg.content) == 2
        assert isinstance(msg.content[0], TextBlock)
        assert isinstance(msg.content[1], ToolCallBlock)

    def test_function_message_with_tool_call_id(self) -> None:
        """Function message with tool_call_id."""
        msg = Message(
            role="function",
            content="Tool result",
            name="search",
            tool_call_id="call_1",
        )

        assert msg.role == "function"
        assert msg.name == "search"
        assert msg.tool_call_id == "call_1"

    def test_invalid_role(self) -> None:
        """Invalid role raises ValidationError."""
        with pytest.raises(ValidationError):
            Message(role="invalid_role", content="test")  # type: ignore[arg-type]

    def test_message_serialization(self) -> None:
        """Message serializes to dict correctly."""
        msg = Message(
            role="user",
            content=[TextBlock(text="Hello")],
        )

        data = msg.model_dump()
        assert data["role"] == "user"
        assert len(data["content"]) == 1
        assert data["content"][0]["type"] == "text"
        assert data["content"][0]["text"] == "Hello"

    def test_message_deserialization(self) -> None:
        """Message deserializes from dict correctly."""
        data = {
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Response"},
                {"type": "tool_call", "id": "call_1", "name": "search", "input": {}},
            ],
        }

        msg = Message(**data)
        assert msg.role == "assistant"
        assert isinstance(msg.content, list)
        assert len(msg.content) == 2
        assert isinstance(msg.content[0], TextBlock)
        assert isinstance(msg.content[1], ToolCallBlock)

    def test_extra_fields_preserved(self) -> None:
        """Extra fields are preserved."""
        data = {
            "role": "user",
            "content": "test",
            "custom_field": "custom_value",
        }

        msg = Message(**data)  # type: ignore[arg-type]
        dumped = msg.model_dump()
        assert dumped["custom_field"] == "custom_value"


class TestContentBlocks:
    """Tests for ContentBlock types."""

    def test_text_block(self) -> None:
        """TextBlock creation and serialization."""
        block = TextBlock(text="Hello world")

        assert block.type == "text"
        assert block.text == "Hello world"

        data = block.model_dump()
        assert data["type"] == "text"
        assert data["text"] == "Hello world"

    def test_thinking_block(self) -> None:
        """ThinkingBlock with signature."""
        block = ThinkingBlock(thinking="Let me think...", signature="abc123")

        assert block.type == "thinking"
        assert block.thinking == "Let me think..."
        assert block.signature == "abc123"

    def test_thinking_block_no_signature(self) -> None:
        """ThinkingBlock without signature."""
        block = ThinkingBlock(thinking="Thinking content")

        assert block.signature is None

    def test_redacted_thinking_block(self) -> None:
        """RedactedThinkingBlock."""
        block = RedactedThinkingBlock(data="redacted_content")

        assert block.type == "redacted_thinking"
        assert block.data == "redacted_content"

    def test_tool_call_block(self) -> None:
        """ToolCallBlock with input."""
        block = ToolCallBlock(
            id="call_123",
            name="search",
            input={"query": "test", "limit": 10},
        )

        assert block.type == "tool_call"
        assert block.id == "call_123"
        assert block.name == "search"
        assert block.input["query"] == "test"
        assert block.input["limit"] == 10

    def test_tool_result_block(self) -> None:
        """ToolResultBlock with output."""
        block = ToolResultBlock(
            tool_call_id="call_123",
            output={"results": [1, 2, 3]},
        )

        assert block.type == "tool_result"
        assert block.tool_call_id == "call_123"
        assert block.output["results"] == [1, 2, 3]

    def test_image_block(self) -> None:
        """ImageBlock with source."""
        block = ImageBlock(
            source={
                "type": "base64",
                "media_type": "image/png",
                "data": "base64data...",
            }
        )

        assert block.type == "image"
        assert block.source["type"] == "base64"
        assert block.source["media_type"] == "image/png"

    def test_reasoning_block(self) -> None:
        """ReasoningBlock with content and summary."""
        block = ReasoningBlock(
            content=[{"type": "text", "text": "Detailed reasoning"}],
            summary=[{"type": "text", "text": "Summary"}],
        )

        assert block.type == "reasoning"
        assert len(block.content) == 1
        assert len(block.summary) == 1

    def test_visibility_field(self) -> None:
        """ContentBlock visibility field."""
        block = TextBlock(text="Internal note", visibility="internal")

        assert block.visibility == "internal"

    def test_discriminated_union_from_dict(self) -> None:
        """ContentBlock discriminated union works from dict."""
        text_dict = {"type": "text", "text": "Hello"}
        tool_dict = {"type": "tool_call", "id": "1", "name": "search", "input": {}}

        msg = Message(role="assistant", content=[text_dict, tool_dict])  # type: ignore[arg-type]

        assert isinstance(msg.content, list)
        assert isinstance(msg.content[0], TextBlock)
        assert isinstance(msg.content[1], ToolCallBlock)


class TestChatRequest:
    """Tests for ChatRequest model."""

    def test_minimal_request(self) -> None:
        """Minimal request with just messages."""
        request = ChatRequest(
            messages=[
                Message(role="system", content="System prompt"),
                Message(role="user", content="Hello"),
            ]
        )

        assert len(request.messages) == 2
        assert request.tools is None
        assert request.temperature is None

    def test_full_request(self) -> None:
        """Full request with all parameters."""
        request = ChatRequest(
            messages=[Message(role="user", content="Test")],
            tools=[
                ToolSpec(
                    name="search",
                    description="Search tool",
                    parameters={
                        "type": "object",
                        "properties": {"query": {"type": "string"}},
                        "required": ["query"],
                    },
                )
            ],
            response_format=ResponseFormatJsonSchema(
                json_schema={"type": "object"},
                strict=True,
            ),
            temperature=0.7,
            top_p=0.9,
            max_output_tokens=4096,
            conversation_id="conv_123",
            stream=False,
            metadata={"user_id": "user_123"},
        )

        assert len(request.messages) == 1
        assert request.tools is not None
        assert len(request.tools) == 1
        assert request.tools[0].name == "search"
        assert request.temperature == 0.7
        assert request.top_p == 0.9
        assert request.max_output_tokens == 4096
        assert request.conversation_id == "conv_123"
        assert request.stream is False
        assert request.metadata is not None
        assert request.metadata["user_id"] == "user_123"

    def test_response_format_text(self) -> None:
        """Request with text response format."""
        request = ChatRequest(
            messages=[Message(role="user", content="Test")],
            response_format=ResponseFormatText(),
        )

        assert request.response_format is not None
        assert request.response_format.type == "text"

    def test_response_format_json(self) -> None:
        """Request with JSON response format."""
        request = ChatRequest(
            messages=[Message(role="user", content="Test")],
            response_format=ResponseFormatJson(),
        )

        assert request.response_format is not None
        assert request.response_format.type == "json"

    def test_response_format_json_schema(self) -> None:
        """Request with JSON schema response format."""
        schema = {
            "type": "object",
            "properties": {"name": {"type": "string"}},
        }

        request = ChatRequest(
            messages=[Message(role="user", content="Test")],
            response_format=ResponseFormatJsonSchema(json_schema=schema, strict=True),
        )

        assert request.response_format is not None
        assert request.response_format.type == "json_schema"
        assert isinstance(request.response_format, ResponseFormatJsonSchema)
        assert request.response_format.json_schema == schema
        assert request.response_format.strict is True

    def test_request_serialization(self) -> None:
        """ChatRequest serializes correctly."""
        request = ChatRequest(
            messages=[Message(role="user", content="Test")],
            temperature=0.5,
        )

        data = request.model_dump()
        assert len(data["messages"]) == 1
        assert data["temperature"] == 0.5

    def test_request_json_roundtrip(self) -> None:
        """ChatRequest JSON serialization roundtrip."""
        request = ChatRequest(
            messages=[
                Message(
                    role="assistant",
                    content=[TextBlock(text="Response")],
                )
            ],
            tools=[
                ToolSpec(
                    name="tool1",
                    parameters={"type": "object"},
                )
            ],
        )

        json_str = request.model_dump_json()
        loaded = ChatRequest.model_validate_json(json_str)

        assert len(loaded.messages) == 1
        assert loaded.messages[0].role == "assistant"
        assert loaded.tools is not None
        assert len(loaded.tools) == 1
        assert loaded.tools[0].name == "tool1"


class TestChatResponse:
    """Tests for ChatResponse model."""

    def test_text_response(self) -> None:
        """Response with text content."""
        response = ChatResponse(
            content=[TextBlock(text="Here's the answer")],
            usage=Usage(input_tokens=10, output_tokens=5, total_tokens=15),
        )

        assert len(response.content) == 1
        assert isinstance(response.content[0], TextBlock)
        assert response.usage is not None
        assert response.usage.total_tokens == 15

    def test_response_with_tool_calls(self) -> None:
        """Response with tool calls."""
        response = ChatResponse(
            content=[
                TextBlock(text="I'll search for that"),
                ToolCallBlock(id="call_1", name="search", input={"query": "test"}),
            ],
            tool_calls=[
                ToolCall(id="call_1", name="search", arguments={"query": "test"})
            ],
            usage=Usage(input_tokens=20, output_tokens=10, total_tokens=30),
        )

        assert len(response.content) == 2
        assert response.tool_calls is not None
        assert len(response.tool_calls) == 1
        assert response.tool_calls[0].name == "search"

    def test_response_with_degradation(self) -> None:
        """Response with format degradation."""
        response = ChatResponse(
            content=[TextBlock(text="Response")],
            degradation=Degradation(
                requested="json_schema",
                actual="json",
                reason="Provider doesn't support strict schema",
            ),
        )

        assert response.degradation is not None
        assert response.degradation.requested == "json_schema"
        assert response.degradation.actual == "json"

    def test_response_with_finish_reason(self) -> None:
        """Response with finish reason."""
        response = ChatResponse(
            content=[TextBlock(text="Response")],
            finish_reason="stop",
        )

        assert response.finish_reason == "stop"

    def test_response_with_metadata(self) -> None:
        """Response with metadata."""
        response = ChatResponse(
            content=[TextBlock(text="Response")],
            metadata={"model": "claude-sonnet-4-5", "latency_ms": 150},
        )

        assert response.metadata is not None
        assert response.metadata["model"] == "claude-sonnet-4-5"
        assert response.metadata["latency_ms"] == 150

    def test_response_serialization(self) -> None:
        """ChatResponse serializes correctly."""
        response = ChatResponse(
            content=[TextBlock(text="Test")],
            usage=Usage(input_tokens=5, output_tokens=3, total_tokens=8),
        )

        data = response.model_dump()
        assert len(data["content"]) == 1
        assert data["usage"] is not None
        assert data["usage"]["total_tokens"] == 8


class TestRoundTripSerialization:
    """Tests for round-trip serialization."""

    def test_message_roundtrip(self) -> None:
        """Message survives dict roundtrip."""
        original = Message(
            role="assistant",
            content=[
                TextBlock(text="Text"),
                ToolCallBlock(id="call_1", name="tool", input={"arg": "value"}),
            ],
        )

        data = original.model_dump()
        restored = Message(**data)

        assert restored.role == original.role
        assert len(restored.content) == len(original.content)
        assert isinstance(restored.content[0], TextBlock)
        assert isinstance(restored.content[1], ToolCallBlock)

    def test_chat_request_roundtrip(self) -> None:
        """ChatRequest survives dict roundtrip."""
        original = ChatRequest(
            messages=[Message(role="user", content="Test")],
            tools=[ToolSpec(name="tool1", parameters={})],
            temperature=0.8,
        )

        data = original.model_dump()
        restored = ChatRequest(**data)

        assert len(restored.messages) == len(original.messages)
        assert original.tools is not None
        assert restored.tools is not None
        assert len(restored.tools) == len(original.tools)
        assert restored.temperature == original.temperature

    def test_chat_response_roundtrip(self) -> None:
        """ChatResponse survives dict roundtrip."""
        original = ChatResponse(
            content=[TextBlock(text="Response")],
            tool_calls=[ToolCall(id="1", name="tool", arguments={})],
            usage=Usage(input_tokens=10, output_tokens=5, total_tokens=15),
        )

        data = original.model_dump()
        restored = ChatResponse(**data)

        assert len(restored.content) == len(original.content)
        assert original.tool_calls is not None
        assert restored.tool_calls is not None
        assert len(restored.tool_calls) == len(original.tool_calls)
        assert original.usage is not None
        assert restored.usage is not None
        assert restored.usage.total_tokens == original.usage.total_tokens

    def test_extra_fields_preserved_roundtrip(self) -> None:
        """Extra fields survive roundtrip."""
        original = Message(
            role="user",
            content="Test",
            custom_field="custom_value",  # type: ignore[call-arg]
            nested={"key": "value"},  # type: ignore[call-arg]
        )

        data = original.model_dump()
        Message(**data)

        assert data["custom_field"] == "custom_value"
        assert data["nested"]["key"] == "value"


class TestValidation:
    """Tests for Pydantic validation."""

    def test_message_requires_role_and_content(self) -> None:
        """Message requires role and content."""
        with pytest.raises(ValidationError):
            Message()  # type: ignore[call-arg]

    def test_tool_spec_requires_name_and_parameters(self) -> None:
        """ToolSpec requires name and parameters."""
        with pytest.raises(ValidationError):
            ToolSpec(name="tool1")  # type: ignore[call-arg]

    def test_tool_call_block_requires_all_fields(self) -> None:
        """ToolCallBlock requires id, name, input."""
        with pytest.raises(ValidationError):
            ToolCallBlock(id="call_1", name="tool")  # type: ignore[call-arg]

    def test_usage_requires_all_fields(self) -> None:
        """Usage requires all token counts."""
        with pytest.raises(ValidationError):
            Usage(input_tokens=10, output_tokens=5)  # type: ignore[call-arg]


class TestUsageOptionalFields:
    """Tests for optional Usage fields (reasoning_tokens, cache tokens)."""

    def test_usage_defaults_none(self) -> None:
        """Optional Usage fields default to None."""
        usage = Usage(input_tokens=100, output_tokens=50, total_tokens=150)
        assert usage.reasoning_tokens is None
        assert usage.cache_read_tokens is None
        assert usage.cache_write_tokens is None

    def test_usage_with_reasoning_tokens(self) -> None:
        """Usage accepts reasoning_tokens."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            reasoning_tokens=30,
        )
        assert usage.reasoning_tokens == 30

    def test_usage_with_cache_tokens(self) -> None:
        """Usage accepts cache read and write tokens."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            cache_read_tokens=80,
            cache_write_tokens=20,
        )
        assert usage.cache_read_tokens == 80
        assert usage.cache_write_tokens == 20

    def test_usage_all_optional_fields(self) -> None:
        """Usage accepts all optional fields together."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            reasoning_tokens=30,
            cache_read_tokens=80,
            cache_write_tokens=20,
        )
        assert usage.reasoning_tokens == 30
        assert usage.cache_read_tokens == 80
        assert usage.cache_write_tokens == 20

    def test_usage_optional_fields_serialization(self) -> None:
        """Optional fields serialize correctly."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            reasoning_tokens=30,
        )
        data = usage.model_dump()
        assert data["reasoning_tokens"] == 30
        assert data["cache_read_tokens"] is None
        assert data["cache_write_tokens"] is None

    def test_usage_optional_fields_exclude_none(self) -> None:
        """Optional fields can be excluded when None."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            reasoning_tokens=30,
        )
        data = usage.model_dump(exclude_none=True)
        assert "reasoning_tokens" in data
        assert "cache_read_tokens" not in data
        assert "cache_write_tokens" not in data

    def test_usage_extra_fields_still_work(self) -> None:
        """Provider-specific extras still pass through alongside named fields."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            reasoning_tokens=30,
            cache_creation_input_tokens=20,  # type: ignore[call-arg]
        )
        data = usage.model_dump()
        assert data["reasoning_tokens"] == 30
        assert data["cache_creation_input_tokens"] == 20

    def test_usage_roundtrip_with_optional_fields(self) -> None:
        """Usage with optional fields survives dict roundtrip."""
        original = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            reasoning_tokens=30,
            cache_read_tokens=80,
        )
        data = original.model_dump()
        restored = Usage(**data)
        assert restored.reasoning_tokens == original.reasoning_tokens
        assert restored.cache_read_tokens == original.cache_read_tokens
        assert restored.cache_write_tokens is None

    def test_existing_usage_construction_unchanged(self) -> None:
        """Existing Usage(input_tokens, output_tokens, total_tokens) still works."""
        usage = Usage(input_tokens=10, output_tokens=5, total_tokens=15)
        assert usage.input_tokens == 10
        assert usage.output_tokens == 5
        assert usage.total_tokens == 15


class TestChatRequestNewFields:
    """Tests for new optional ChatRequest fields."""

    def test_new_fields_default_none(self) -> None:
        """New ChatRequest fields default to None."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
        )
        assert request.model is None
        assert request.tool_choice is None
        assert request.stop is None
        assert request.reasoning_effort is None
        assert request.timeout is None

    def test_model_field(self) -> None:
        """ChatRequest accepts model field."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            model="claude-sonnet-4-5",
        )
        assert request.model == "claude-sonnet-4-5"

    def test_tool_choice_string(self) -> None:
        """ChatRequest accepts tool_choice as string."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            tool_choice="required",
        )
        assert request.tool_choice == "required"

    def test_tool_choice_dict(self) -> None:
        """ChatRequest accepts tool_choice as dict for named tool."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            tool_choice={"name": "search"},
        )
        assert request.tool_choice == {"name": "search"}

    def test_tool_choice_nested_dict(self) -> None:
        """ChatRequest accepts nested tool_choice (OpenAI function-forcing form)."""
        tool_choice = {"type": "function", "function": {"name": "get_weather"}}
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            tool_choice=tool_choice,
        )
        assert request.tool_choice == tool_choice
        assert request.tool_choice["function"]["name"] == "get_weather"

    def test_stop_sequences(self) -> None:
        """ChatRequest accepts stop sequences."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            stop=["END", "STOP"],
        )
        assert request.stop == ["END", "STOP"]

    def test_reasoning_effort(self) -> None:
        """ChatRequest accepts reasoning_effort."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            reasoning_effort="high",
        )
        assert request.reasoning_effort == "high"

    def test_timeout(self) -> None:
        """ChatRequest accepts timeout in seconds."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            timeout=30.0,
        )
        assert request.timeout == 30.0

    def test_all_new_fields_together(self) -> None:
        """All new fields work together with existing fields."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            temperature=0.7,
            model="gpt-5.2",
            tool_choice="auto",
            stop=["END"],
            reasoning_effort="medium",
            timeout=60.0,
        )
        assert request.temperature == 0.7
        assert request.model == "gpt-5.2"
        assert request.tool_choice == "auto"
        assert request.stop == ["END"]
        assert request.reasoning_effort == "medium"
        assert request.timeout == 60.0

    def test_new_fields_serialize(self) -> None:
        """New fields serialize correctly."""
        request = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            model="claude-sonnet-4-5",
            reasoning_effort="high",
        )
        data = request.model_dump()
        assert data["model"] == "claude-sonnet-4-5"
        assert data["reasoning_effort"] == "high"
        assert data["tool_choice"] is None

    def test_new_fields_roundtrip(self) -> None:
        """New fields survive dict roundtrip."""
        original = ChatRequest(
            messages=[Message(role="user", content="Hello")],
            model="gpt-5.2",
            tool_choice="required",
            stop=["END"],
            reasoning_effort="low",
            timeout=30.0,
        )
        data = original.model_dump()
        restored = ChatRequest(**data)
        assert restored.model == original.model
        assert restored.tool_choice == original.tool_choice
        assert restored.stop == original.stop
        assert restored.reasoning_effort == original.reasoning_effort
        assert restored.timeout == original.timeout

    def test_existing_construction_unchanged(self) -> None:
        """Existing ChatRequest construction patterns still work."""
        request = ChatRequest(
            messages=[Message(role="user", content="Test")],
            tools=[ToolSpec(name="search", parameters={"type": "object"})],
            temperature=0.5,
        )
        assert len(request.messages) == 1
        assert request.tools is not None
        assert len(request.tools) == 1
        assert request.temperature == 0.5
        assert request.model is None
