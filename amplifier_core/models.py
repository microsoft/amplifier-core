"""
Core data models for Amplifier.
Uses Pydantic for validation and serialization.
"""

from datetime import datetime
from typing import Any
from typing import Literal

from pydantic import BaseModel
from pydantic import Field


class ToolCall(BaseModel):
    """Represents a tool invocation request."""

    tool: str = Field(..., description="Tool name to invoke")
    arguments: dict[str, Any] = Field(default_factory=dict, description="Tool arguments")
    id: str | None = Field(default=None, description="Unique tool call ID")


class ToolResult(BaseModel):
    """Result from tool execution."""

    success: bool = Field(default=True, description="Whether execution succeeded")
    output: Any | None = Field(default=None, description="Tool output data")
    error: dict[str, Any] | None = Field(default=None, description="Error details if failed")

    def __str__(self) -> str:
        if self.success:
            return str(self.output) if self.output else "Success"
        return f"Error: {self.error.get('message', 'Unknown error')}" if self.error else "Failed"


class HookResult(BaseModel):
    """Result from hook execution."""

    action: Literal["continue", "deny", "modify"] = Field(
        default="continue", description="Action to take: continue normally, deny operation, or modify data"
    )
    data: dict[str, Any] | None = Field(default=None, description="Modified data if action is 'modify'")
    reason: str | None = Field(default=None, description="Reason for deny or modification")


class AgentResult(BaseModel):
    """Result from agent execution."""

    success: bool = Field(default=True, description="Whether agent succeeded")
    text: str | None = Field(default=None, description="Agent response text")
    data: dict[str, Any] | None = Field(default=None, description="Structured output data")
    error: dict[str, Any] | None = Field(default=None, description="Error details if failed")


class ProviderResponse(BaseModel):
    """Response from LLM provider."""

    content: str = Field(..., description="Response text content")
    raw: Any | None = Field(default=None, description="Raw provider response object")
    usage: dict[str, int] | None = Field(default=None, description="Token usage statistics")
    tool_calls: list[ToolCall] | None = Field(default=None, description="Parsed tool calls from response")


class ModuleInfo(BaseModel):
    """Module metadata."""

    id: str = Field(..., description="Module identifier")
    name: str = Field(..., description="Module display name")
    version: str = Field(..., description="Module version")
    type: Literal["orchestrator", "provider", "tool", "agent", "context", "hook"] = Field(
        ..., description="Module type"
    )
    mount_point: str = Field(..., description="Where module should be mounted")
    description: str = Field(..., description="Module description")
    config_schema: dict[str, Any] | None = Field(default=None, description="JSON schema for module configuration")


class SessionStatus(BaseModel):
    """Session status and metadata."""

    session_id: str = Field(..., description="Unique session ID")
    started_at: datetime = Field(default_factory=datetime.now)
    ended_at: datetime | None = None
    status: Literal["running", "completed", "failed", "cancelled"] = "running"

    # Counters
    total_messages: int = 0
    tool_invocations: int = 0
    tool_successes: int = 0
    tool_failures: int = 0

    # Token usage
    total_input_tokens: int = 0
    total_output_tokens: int = 0

    # Cost tracking (if available)
    estimated_cost: float | None = None

    # Last activity
    last_activity: datetime | None = None
    last_error: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to JSON-serializable dict."""
        return self.model_dump(mode="json", exclude_none=True)
