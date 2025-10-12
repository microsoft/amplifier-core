"""Content models for unified content handling across providers."""

from dataclasses import dataclass
from enum import Enum
from typing import Any


class ContentBlockType(str, Enum):
    """Types of content blocks."""

    TEXT = "text"
    THINKING = "thinking"  # Reasoning/thinking blocks (Claude thinking, OpenAI reasoning)
    TOOL_CALL = "tool_call"
    TOOL_RESULT = "tool_result"
    # Future: IMAGE, AUDIO, etc.


@dataclass
class ContentBlock:
    """Base class for all content blocks.

    Provides common structure and raw data preservation.
    """

    type: ContentBlockType
    raw: dict[str, Any] | None = None  # Original provider-specific data

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        result: dict[str, Any] = {"type": self.type.value}
        # Don't include raw field as it may contain non-serializable provider objects
        return result


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
    """Model reasoning/thinking content.

    This represents the model's internal reasoning process.
    Should be displayed without truncation to preserve full context.
    """

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
