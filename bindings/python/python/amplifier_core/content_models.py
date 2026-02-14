"""Content models for event emission and streaming UI.

These simple dataclass-based content types are used by providers for:
- Event blocks emitted during streaming (event_blocks)
- Streaming UI compatibility fields (content_blocks in responses)

Note: These are DISTINCT from message_models.py which provides Pydantic models
for the request/response envelope (ChatRequest, ChatResponse). Both modules
are used together - content_models for events, message_models for envelopes.
"""

from dataclasses import dataclass
from enum import Enum
from typing import Any


class ContentBlockType(str, Enum):
    """Types of content blocks."""

    TEXT = "text"
    THINKING = "thinking"
    TOOL_CALL = "tool_call"
    TOOL_RESULT = "tool_result"


@dataclass
class ContentBlock:
    """Base class for all content blocks."""

    type: ContentBlockType
    raw: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {"type": self.type.value}


@dataclass
class TextContent(ContentBlock):
    """Regular text content from the model."""

    type: ContentBlockType = ContentBlockType.TEXT
    text: str = ""

    def to_dict(self) -> dict[str, Any]:
        result = super().to_dict()
        result["text"] = self.text
        return result


@dataclass
class ThinkingContent(ContentBlock):
    """Model reasoning/thinking content."""

    type: ContentBlockType = ContentBlockType.THINKING
    text: str = ""

    def to_dict(self) -> dict[str, Any]:
        result = super().to_dict()
        result["text"] = self.text
        return result


@dataclass
class ToolCallContent(ContentBlock):
    """Tool call request from the model."""

    type: ContentBlockType = ContentBlockType.TOOL_CALL
    id: str = ""
    name: str = ""
    arguments: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        result = super().to_dict()
        result.update({"id": self.id, "name": self.name, "arguments": self.arguments})
        return result


@dataclass
class ToolResultContent(ContentBlock):
    """Result from tool execution."""

    type: ContentBlockType = ContentBlockType.TOOL_RESULT
    tool_call_id: str = ""
    output: Any = None
    error: str | None = None

    def to_dict(self) -> dict[str, Any]:
        result = super().to_dict()
        result.update({"tool_call_id": self.tool_call_id, "output": self.output})
        if self.error:
            result["error"] = self.error
        return result
