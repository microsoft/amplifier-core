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
    """
    Result from hook execution with enhanced capabilities.

    Hooks can now not only observe and block operations, but also inject context to the agent,
    request user approval, and control output visibility. These capabilities enable hooks to
    participate in the agent's cognitive loop.

    Actions:
        continue: Proceed normally with the operation
        deny: Block the operation (short-circuits handler chain)
        modify: Modify event data (chains through handlers)
        inject_context: Add content to agent's context (enables feedback loops)
        ask_user: Request user approval before proceeding (dynamic permissions)

    Context Injection:
        Hooks can inject text directly into the agent's conversation context, enabling
        automated feedback loops. For example, a linter hook can inject error messages
        that the agent sees and fixes immediately within the same turn.

        The injected content appears as a message with the specified role (system/user/assistant).
        System role (default) is recommended for environmental feedback.

        Injections are size-limited (default 10 KB, configurable via session.injection_size_limit), audited, and tagged with provenance metadata.

    Approval Gates:
        Hooks can request user approval for operations, enabling dynamic permission logic
        that goes beyond the kernel's built-in approval system. The user sees a prompt
        with configurable options and timeout behavior.

        Approvals are session-scoped cached (e.g., "Allow always" remembered this session).
        On timeout, the configured default action is taken (deny by default for security).

    Output Control:
        Hooks can control visibility of their own output and display targeted messages
        to the user. This enables clean UX by hiding verbose hook processing while
        showing important alerts or warnings.

        Note: Hooks can only suppress their own output, not tool output (security).

    Example - Context Injection:
        ```python
        HookResult(
            action="inject_context",
            context_injection="Linter found error on line 42: Line too long",
            context_injection_role="system",  # Appears as system message
            user_message="Found 3 linting issues",  # User sees this
            suppress_output=True  # Hide verbose linter output
        )
        ```

    Example - Approval Gate:
        ```python
        HookResult(
            action="ask_user",
            approval_prompt="Allow write to production/config.py?",
            approval_options=["Allow once", "Allow always", "Deny"],
            approval_timeout=300.0,  # 5 minutes
            approval_default="deny",  # Safe default
            reason="Production file requires explicit approval"
        )
        ```

    Example - Output Control Only:
        ```python
        HookResult(
            action="continue",
            user_message="Processed 10 files successfully",
            user_message_level="info",
            suppress_output=True  # Hide processing details
        )
        ```
    """

    # Core action
    action: Literal["continue", "deny", "modify", "inject_context", "ask_user"] = Field(
        default="continue",
        description=(
            "Action to take: 'continue' (proceed normally), 'deny' (block operation), "
            "'modify' (modify event data), 'inject_context' (add to agent's context), "
            "'ask_user' (request user approval)"
        ),
    )

    # Existing fields
    data: dict[str, Any] | None = Field(
        default=None, description="Modified event data (for action='modify'). Changes chain through handlers."
    )
    reason: str | None = Field(
        default=None, description="Explanation for deny/modification. Shown to agent when operation is blocked."
    )

    # Context injection fields
    context_injection: str | None = Field(
        default=None,
        description=(
            "Text to inject into agent's conversation context (for action='inject_context'). "
            "Agent sees this content and can respond to it. Enables automated feedback loops. "
            "Default 10 KB limit per injection (configurable via session.injection_size_limit). "
            "Content is audited and tagged with source hook."
        ),
    )
    context_injection_role: Literal["system", "user", "assistant"] = Field(
        default="system",
        description=(
            "Role for injected message in conversation. 'system' (default) for environmental feedback, "
            "'user' to simulate user input, 'assistant' for agent self-talk. "
            "System role recommended for most use cases."
        ),
    )
    ephemeral: bool = Field(
        default=False,
        description=(
            "If True, injection is temporary (only for current LLM call, not stored in history). "
            "Use for transient state like todo reminders that update frequently. "
            "Orchestrator must append ephemeral injection to messages without storing in context."
        ),
    )

    # Approval gate fields
    approval_prompt: str | None = Field(
        default=None,
        description=(
            "Question to ask user (for action='ask_user'). Displayed in approval UI. "
            "Should clearly explain what operation requires approval and why."
        ),
    )
    approval_options: list[str] | None = Field(
        default=None,
        description=(
            "User choice options for approval (for action='ask_user'). "
            "If None, defaults to ['Allow', 'Deny']. "
            "Can include 'Allow once', 'Allow always', 'Deny' for flexible permission control."
        ),
    )
    approval_timeout: float = Field(
        default=300.0,
        description=(
            "Seconds to wait for user response (for action='ask_user'). "
            "Default 300.0 (5 minutes). On timeout, approval_default action is taken."
        ),
    )
    approval_default: Literal["allow", "deny"] = Field(
        default="deny",
        description=(
            "Default decision on timeout or error (for action='ask_user'). "
            "'deny' (default) is safer for security-sensitive operations. "
            "'allow' may be appropriate for low-risk operations."
        ),
    )

    # Output control fields
    suppress_output: bool = Field(
        default=False,
        description=(
            "Hide hook's stdout/stderr from user transcript. "
            "Use to prevent verbose processing output from cluttering the UI. "
            "Note: Only suppresses hook's own output, not tool output (security)."
        ),
    )
    user_message: str | None = Field(
        default=None,
        description=(
            "Message to display to user (separate from context_injection). "
            "Use for alerts, warnings, or status updates that user should see. "
            "Displayed with specified severity level."
        ),
    )
    user_message_level: Literal["info", "warning", "error"] = Field(
        default="info",
        description=(
            "Severity level for user_message. "
            "'info' for status updates, 'warning' for non-critical issues, 'error' for failures."
        ),
    )

    # Injection placement control
    append_to_last_tool_result: bool = Field(
        default=False,
        description=(
            "If True and ephemeral=True, append context_injection to the last tool result message "
            "instead of creating a new message. Use for contextual reminders that relate to the "
            "tool that just executed. Falls back to new message if last message isn't a tool result. "
            "Only applicable when action='inject_context' and ephemeral=True."
        ),
    )


class ModelInfo(BaseModel):
    """Model metadata for provider models.

    Describes capabilities and defaults for a specific model available from a provider.
    """

    id: str = Field(..., description="Model identifier (e.g., 'claude-sonnet-4-5', 'gpt-4o')")
    display_name: str = Field(..., description="Human-readable model name")
    context_window: int = Field(..., description="Maximum context window in tokens")
    max_output_tokens: int = Field(..., description="Maximum output tokens")
    capabilities: list[str] = Field(
        default_factory=list,
        description="Extensible capability list (e.g., 'tools', 'vision', 'thinking', 'streaming', 'json_mode')",
    )
    defaults: dict[str, Any] = Field(
        default_factory=dict,
        description="Model-specific default config values (e.g., temperature, max_tokens)",
    )


class ProviderInfo(BaseModel):
    """Provider metadata.

    Describes capabilities, authentication requirements, and defaults for a provider.
    """

    id: str = Field(..., description="Provider identifier (e.g., 'anthropic', 'openai')")
    display_name: str = Field(..., description="Human-readable provider name")
    credential_env_vars: list[str] = Field(
        default_factory=list,
        description="Environment variables for credentials (e.g., ['ANTHROPIC_API_KEY'])",
    )
    capabilities: list[str] = Field(
        default_factory=list,
        description="Extensible capability list (e.g., 'streaming', 'batch', 'embeddings')",
    )
    defaults: dict[str, Any] = Field(
        default_factory=dict,
        description="Provider-level default config values (e.g., timeout, max_retries)",
    )


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
