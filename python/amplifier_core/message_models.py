"""Complete Pydantic models implementing REQUEST_ENVELOPE_V1 specification.

This module provides type-safe message handling across all providers following
the REQUEST_ENVELOPE_V1 specification. All models use Pydantic for validation
and serialization.

Note: content_models.py provides simpler dataclass-based types for event emission
and streaming UI. Both modules are used together - this module for request/response
envelopes, content_models for event blocks.

See:
- docs/REQUEST_ENVELOPE_MODELS.md for usage guide
- docs/specs/provider/REQUEST_ENVELOPE_V1.md for complete specification
- docs/schemas/request_envelope_v1.json for JSON schema
"""

from typing import Annotated
from typing import Any
from typing import Literal
from typing import Union

from pydantic import BaseModel
from pydantic import ConfigDict
from pydantic import Field


class TextBlock(BaseModel):
    """Regular text content."""

    model_config = ConfigDict(extra="allow")

    type: Literal["text"] = "text"
    text: str
    visibility: Literal["internal", "developer", "user"] | None = None


class ThinkingBlock(BaseModel):
    """Anthropic extended thinking block (must be preserved with signature)."""

    model_config = ConfigDict(extra="allow")

    type: Literal["thinking"] = "thinking"
    thinking: str
    signature: str | None = None
    visibility: Literal["internal", "developer", "user"] | None = None
    content: list[Any] | None = (
        None  # OpenAI reasoning state: [encrypted_content, reasoning_id]
    )


class RedactedThinkingBlock(BaseModel):
    """Anthropic redacted thinking block."""

    model_config = ConfigDict(extra="allow")

    type: Literal["redacted_thinking"] = "redacted_thinking"
    data: str
    visibility: Literal["internal", "developer", "user"] | None = None


class ToolCallBlock(BaseModel):
    """Tool call request from model."""

    model_config = ConfigDict(extra="allow")

    type: Literal["tool_call"] = "tool_call"
    id: str
    name: str
    input: dict[str, Any]
    visibility: Literal["internal", "developer", "user"] | None = None


class ToolResultBlock(BaseModel):
    """Tool execution result."""

    model_config = ConfigDict(extra="allow")

    type: Literal["tool_result"] = "tool_result"
    tool_call_id: str
    output: Any
    visibility: Literal["internal", "developer", "user"] | None = None


class ImageBlock(BaseModel):
    """Image content."""

    model_config = ConfigDict(extra="allow")

    type: Literal["image"] = "image"
    source: dict[str, Any]
    visibility: Literal["internal", "developer", "user"] | None = None


class ReasoningBlock(BaseModel):
    """OpenAI o-series reasoning content."""

    model_config = ConfigDict(extra="allow")

    type: Literal["reasoning"] = "reasoning"
    content: list[Any]
    summary: list[Any]
    visibility: Literal["internal", "developer", "user"] | None = None


ContentBlockUnion = Annotated[
    Union[
        TextBlock,
        ThinkingBlock,
        RedactedThinkingBlock,
        ToolCallBlock,
        ToolResultBlock,
        ImageBlock,
        ReasoningBlock,
    ],
    Field(discriminator="type"),
]


class Message(BaseModel):
    """Single message in conversation history.

    Messages contain role and content which can be either a string or
    a list of ContentBlocks for multimodal/structured content.
    """

    model_config = ConfigDict(extra="allow")

    role: Literal["system", "developer", "user", "assistant", "function", "tool"]
    content: Union[str, list[ContentBlockUnion]]
    name: str | None = None
    tool_call_id: str | None = None
    metadata: dict[str, Any] | None = (
        None  # Provider-specific state (e.g., OpenAI reasoning items)
    )


class ToolSpec(BaseModel):
    """Tool/function specification with JSON Schema parameters."""

    model_config = ConfigDict(extra="allow")

    name: str
    parameters: dict[str, Any]
    description: str | None = None


class ResponseFormatText(BaseModel):
    """Text response format."""

    type: Literal["text"] = "text"


class ResponseFormatJson(BaseModel):
    """JSON response format (any JSON)."""

    type: Literal["json"] = "json"


class ResponseFormatJsonSchema(BaseModel):
    """JSON Schema response format with strict mode."""

    model_config = ConfigDict(populate_by_name=True)

    type: Literal["json_schema"] = "json_schema"
    json_schema: dict[str, Any] = Field(serialization_alias="schema")
    strict: bool | None = None


ResponseFormat = Union[
    ResponseFormatText,
    ResponseFormatJson,
    ResponseFormatJsonSchema,
]


class ChatRequest(BaseModel):
    """Complete chat request to provider.

    This is the unified request format that all providers receive.
    Providers convert this to their native format.

    Optional fields (model, tool_choice, stop, reasoning_effort, timeout) give
    hooks and orchestrators a standard way to influence provider behavior.
    Providers that don't support a field ignore it. Fields that providers
    already read from **kwargs are surfaced here for hook visibility.
    """

    model_config = ConfigDict(extra="allow")

    messages: list[Message]
    tools: list[ToolSpec] | None = None
    response_format: ResponseFormat | None = None
    temperature: float | None = None
    top_p: float | None = None
    max_output_tokens: int | None = None
    conversation_id: str | None = None
    stream: bool | None = False
    metadata: dict[str, Any] | None = None
    model: str | None = Field(
        default=None,
        description=(
            "Per-request model override. Precedence relative to"
            " provider-configured defaults is provider/orchestrator policy."
        ),
    )
    tool_choice: str | dict[str, Any] | None = None
    stop: list[str] | None = None
    reasoning_effort: str | None = None
    timeout: float | None = Field(
        default=None,
        description="Per-request timeout in seconds. Complements session-level CancellationToken.",
    )


class ToolCall(BaseModel):
    """Tool call in response."""

    model_config = ConfigDict(extra="allow")

    id: str
    name: str
    arguments: dict[str, Any]


class Usage(BaseModel):
    """Token usage information.

    The three required fields (input_tokens, output_tokens, total_tokens) are
    reported by all providers. Optional fields surface commonly-available
    metrics that enable cross-provider cost tracking and cache optimization.

    Providers that don't report optional metrics leave them as None.
    Additional provider-specific metrics can be passed via extra="allow"
    (e.g., Anthropic's cache_creation_input_tokens).
    """

    model_config = ConfigDict(extra="allow")

    input_tokens: int
    output_tokens: int
    total_tokens: int
    reasoning_tokens: int | None = None
    cache_read_tokens: int | None = None
    cache_write_tokens: int | None = None


class Degradation(BaseModel):
    """Response format degradation information."""

    model_config = ConfigDict(extra="allow")

    requested: str
    actual: str
    reason: str


class ChatResponse(BaseModel):
    """Response from provider.

    This is the unified response format that providers return.
    Contains content blocks, tool calls, usage info, and metadata.
    """

    model_config = ConfigDict(extra="allow")

    content: list[ContentBlockUnion]
    tool_calls: list[ToolCall] | None = None
    usage: Usage | None = None
    degradation: Degradation | None = None
    finish_reason: str | None = None
    metadata: dict[str, Any] | None = None
