"""
Standard interfaces for Amplifier modules.
Uses Protocol classes for structural subtyping (no inheritance required).

Related contracts (for module developers):
  - docs/contracts/PROVIDER_CONTRACT.md (Provider, lines 54-119)
  - docs/contracts/TOOL_CONTRACT.md (Tool, lines 121-146)
  - docs/contracts/HOOK_CONTRACT.md (HookHandler, lines 173-189)
  - docs/contracts/ORCHESTRATOR_CONTRACT.md (Orchestrator, lines 26-52)
  - docs/contracts/CONTEXT_CONTRACT.md (ContextManager, lines 148-180)
"""

from typing import TYPE_CHECKING
from typing import Any
from typing import Protocol
from typing import runtime_checkable

from pydantic import BaseModel
from pydantic import Field

from .message_models import ChatRequest
from .message_models import ChatResponse
from .message_models import ToolCall
from .models import HookResult
from .models import ModelInfo
from .models import ProviderInfo
from .models import ToolResult

if TYPE_CHECKING:
    from .hooks import HookRegistry


@runtime_checkable
class Orchestrator(Protocol):
    """Interface for agent loop orchestrator modules."""

    async def execute(
        self,
        prompt: str,
        context: "ContextManager",
        providers: dict[str, "Provider"],
        tools: dict[str, "Tool"],
        hooks: "HookRegistry",
    ) -> str:
        """
        Execute the agent loop with given prompt.

        Args:
            prompt: User input prompt
            context: Context manager for conversation state
            providers: Available LLM providers
            tools: Available tools
            hooks: Hook registry for lifecycle events

        Returns:
            Final response string
        """
        ...


@runtime_checkable
class Provider(Protocol):
    """
    Interface for LLM provider modules.

    Providers receive ChatRequest (typed, validated messages) and return
    ChatResponse (typed, structured content). Orchestrators handle conversion
    between context storage format (dict) and provider contract (ChatRequest).

    This maintains clean separation:
    - Storage layer (contexts) use dicts for serialization flexibility
    - Business logic layer (orchestrators) use typed models
    - Service layer (providers) have strong contracts
    """

    @property
    def name(self) -> str:
        """Provider name."""
        ...

    def get_info(self) -> ProviderInfo:
        """
        Get provider metadata.

        Returns:
            ProviderInfo with id, display_name, credential_env_vars, capabilities, defaults
        """
        ...

    async def list_models(self) -> list[ModelInfo]:
        """
        List available models for this provider.

        Provider decides implementation: API query, hardcoded list, cached response, etc.
        Returns empty list if model discovery not available (user enters model manually).

        Returns:
            List of ModelInfo for available models
        """
        ...

    async def complete(self, request: ChatRequest, **kwargs) -> ChatResponse:
        """
        Generate completion from ChatRequest.

        Args:
            request: Typed chat request with messages, tools, config
            **kwargs: Provider-specific options (override request fields)

        Returns:
            ChatResponse with content blocks, tool calls, usage
        """
        ...

    def parse_tool_calls(self, response: ChatResponse) -> list[ToolCall]:
        """
        Parse tool calls from ChatResponse.

        Args:
            response: Typed chat response

        Returns:
            List of tool calls to execute
        """
        ...


@runtime_checkable
class Tool(Protocol):
    """Interface for tool modules."""

    @property
    def name(self) -> str:
        """Tool name for invocation."""
        ...

    @property
    def description(self) -> str:
        """Human-readable tool description."""
        ...

    async def execute(self, input: dict[str, Any]) -> ToolResult:
        """
        Execute tool with given input.

        Args:
            input: Tool-specific input parameters

        Returns:
            Tool execution result
        """
        ...


@runtime_checkable
class ContextManager(Protocol):
    """
    Interface for context management modules.

    Context managers own memory policy. Orchestrators ask for messages;
    context managers decide how to fit them within limits. This maintains
    clean mechanism/policy separation - orchestrators are mechanisms that
    request messages, context managers are policies that decide what to return.
    """

    async def add_message(self, message: dict[str, Any]) -> None:
        """Add a message to the context."""
        ...

    async def get_messages_for_request(
        self,
        token_budget: int | None = None,
        provider: Any | None = None,
    ) -> list[dict[str, Any]]:
        """
        Get messages ready for an LLM request.

        The context manager handles any compaction needed internally.
        Orchestrators call this before every LLM request and trust the
        context manager to return messages that fit within limits.

        Args:
            token_budget: Optional explicit token limit (deprecated, prefer provider).
            provider: Optional provider instance for dynamic budget calculation.
                If provided, budget = context_window - max_output_tokens - safety_margin.

        Returns:
            Messages ready for LLM request, compacted if necessary.
        """
        ...

    async def get_messages(self) -> list[dict[str, Any]]:
        """Get all messages (raw, uncompacted) for transcripts/debugging."""
        ...

    async def set_messages(self, messages: list[dict[str, Any]]) -> None:
        """Set messages directly (for session resume)."""
        ...

    async def clear(self) -> None:
        """Clear all messages."""
        ...


@runtime_checkable
class HookHandler(Protocol):
    """Interface for hook handlers."""

    async def __call__(self, event: str, data: dict[str, Any]) -> HookResult:
        """
        Handle a lifecycle event.

        Args:
            event: Event name
            data: Event data

        Returns:
            Hook result indicating action to take
        """
        ...


class ApprovalRequest(BaseModel):
    """Request for user approval of a tool action."""

    tool_name: str = Field(..., description="Name of the tool requesting approval")
    action: str = Field(..., description="Human-readable description of the action")
    details: dict[str, Any] = Field(default_factory=dict, description="Tool-specific context and parameters")
    risk_level: str = Field(..., description="Risk level: low, medium, high, or critical")
    timeout: float | None = Field(default=None, description="Timeout in seconds (None = wait indefinitely)")

    def model_post_init(self, __context: Any) -> None:
        """Validate timeout if provided."""
        if self.timeout is not None and self.timeout <= 0:
            raise ValueError("Timeout must be positive or None (infinite wait)")


class ApprovalResponse(BaseModel):
    """Response to an approval request."""

    approved: bool = Field(..., description="Whether the action was approved")
    reason: str | None = Field(default=None, description="Explanation for approval/denial")
    remember: bool = Field(default=False, description="Cache this decision for future requests")


@runtime_checkable
class ApprovalProvider(Protocol):
    """Protocol for UI components that provide approval dialogs."""

    async def request_approval(self, request: ApprovalRequest) -> ApprovalResponse:
        """
        Request approval from the user.

        Args:
            request: Approval request with action details

        Returns:
            Approval decision from the user

        Raises:
            TimeoutError: If request.timeout expires without response
            Exception: If provider encounters an error
        """
        ...
