"""
Core data models for Amplifier.
Uses Pydantic for validation and serialization.
"""

import json
import re
from datetime import datetime
from typing import Any
from typing import Literal

from pydantic import BaseModel
from pydantic import Field


def _sanitize_for_llm(text: str) -> str:
    """Sanitize text content for safe transmission to LLM APIs.

    Removes control characters that can cause API errors while preserving
    common whitespace (tab, newline, carriage return). Also handles
    problematic Unicode sequences.

    This prevents "Internal server error" from providers when tool results
    contain unexpected control characters from source code or LSP responses.
    """
    # Remove control characters except tab (\x09), newline (\x0a), carriage return (\x0d)
    # Control chars are \x00-\x1f and \x7f-\x9f
    sanitized = re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x9f]", "", text)

    # Remove lone UTF-16 surrogates (invalid in JSON, can cause API errors)
    # Surrogate pairs: \uD800-\uDFFF should only appear in valid pairs
    sanitized = re.sub(r"[\ud800-\udfff]", "", sanitized)

    return sanitized


class ToolResult(BaseModel):
    """Result from tool execution."""

    success: bool = Field(default=True, description="Whether execution succeeded")
    output: Any | None = Field(default=None, description="Tool output data")
    error: dict[str, Any] | None = Field(
        default=None, description="Error details if failed"
    )

    def __str__(self) -> str:
        if self.success:
            return str(self.output) if self.output else "Success"
        return (
            f"Error: {self.error.get('message', 'Unknown error')}"
            if self.error
            else "Failed"
        )

    def get_serialized_output(self) -> str:
        """Get output serialized appropriately for LLM context.

        Returns JSON for dict/list outputs (proper format for LLM parsing),
        otherwise returns string representation. This ensures structured data
        like {"stdout": ..., "stderr": ..., "returncode": ...} is serialized
        as valid JSON rather than Python repr format.

        Note: For tools like bash that populate output even on failure (with
        stdout/stderr/returncode), we serialize the output regardless of the
        success flag - the output contains the actual error information.

        Content is sanitized to remove control characters that can cause
        LLM API errors (e.g., Anthropic "Internal server error").
        """
        # If output exists and is structured data, always serialize it
        # (even on failure - bash tools put error info in output.stderr)
        if self.output is not None:
            if isinstance(self.output, (dict, list)):
                result = json.dumps(self.output)
            else:
                result = str(self.output)
            # Sanitize to prevent LLM API errors from control characters
            return _sanitize_for_llm(result)

        # No output - check if this is an error case
        if not self.success:
            return f"Error: {self.error.get('message', 'Unknown error') if self.error else 'Failed'}"

        # Success with no output
        return "Success"


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

        Injections are unlimited by default (configurable via session.injection_size_limit), audited, and tagged with provenance metadata.

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
        default=None,
        description="Modified event data (for action='modify'). Changes chain through handlers.",
    )
    reason: str | None = Field(
        default=None,
        description="Explanation for deny/modification. Shown to agent when operation is blocked.",
    )

    # Context injection fields
    context_injection: str | None = Field(
        default=None,
        description=(
            "Text to inject into agent's conversation context (for action='inject_context'). "
            "Agent sees this content and can respond to it. Enables automated feedback loops. "
            "Unlimited by default (configurable via session.injection_size_limit). "
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
    user_message_source: str | None = Field(
        default=None,
        description=(
            "Source name for user_message display (e.g., 'python-check'). "
            "If None, falls back to the hook_name passed by the orchestrator. "
            "Use to provide a meaningful label when hook_name is generic (like tool name)."
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

    id: str = Field(
        ..., description="Model identifier (e.g., 'claude-sonnet-4-5', 'gpt-5.2')"
    )
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


class ConfigField(BaseModel):
    """A configuration field that a provider needs, with prompt metadata.

    Providers define their configuration needs through these fields. The app-cli
    renders them generically into interactive prompts, keeping all provider-specific
    logic in the provider modules.
    """

    id: str = Field(..., description="Field identifier (used as key in config dict)")
    display_name: str = Field(..., description="Human-readable label for prompts")
    field_type: Literal["text", "secret", "choice", "boolean"] = Field(
        default="text",
        description="Field type: 'text' for plain input, 'secret' for masked input, 'choice' for selection, 'boolean' for yes/no",
    )
    prompt: str = Field(..., description="Question to ask the user")
    env_var: str | None = Field(
        default=None, description="Environment variable to check/set"
    )
    choices: list[str] | None = Field(
        default=None, description="Valid choices (for field_type='choice')"
    )
    required: bool = Field(default=True, description="Whether this field is required")
    default: str | None = Field(
        default=None, description="Default value if not provided"
    )
    show_when: dict[str, str] | None = Field(
        default=None,
        description="Conditional visibility: show this field only when another field has a specific value (e.g., {'model': 'claude-sonnet-4-5'})",
    )
    requires_model: bool = Field(
        default=False,
        description="If True, this field is shown after model selection (enables show_when to reference the selected model)",
    )


class ProviderInfo(BaseModel):
    """Provider metadata.

    Describes capabilities, authentication requirements, and defaults for a provider.
    """

    id: str = Field(
        ..., description="Provider identifier (e.g., 'anthropic', 'openai')"
    )
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
    config_fields: list[ConfigField] = Field(
        default_factory=list,
        description="Configuration fields for interactive setup. Provider defines all fields it needs.",
    )


class ModuleInfo(BaseModel):
    """Module metadata."""

    id: str = Field(..., description="Module identifier")
    name: str = Field(..., description="Module display name")
    version: str = Field(..., description="Module version")
    type: Literal["orchestrator", "provider", "tool", "context", "hook", "resolver"] = (
        Field(..., description="Module type")
    )
    mount_point: str = Field(..., description="Where module should be mounted")
    description: str = Field(..., description="Module description")
    config_schema: dict[str, Any] | None = Field(
        default=None, description="JSON schema for module configuration"
    )


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
