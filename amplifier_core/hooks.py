"""
Hook system for lifecycle events.
Provides deterministic execution with priority ordering.
"""

import asyncio
import logging
from collections import defaultdict
from collections.abc import Awaitable
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from .models import HookResult

logger = logging.getLogger(__name__)


@dataclass
class HookHandler:
    """Registered hook handler with priority."""

    handler: Callable[[str, dict[str, Any]], Awaitable[HookResult]]
    priority: int = 0
    name: str | None = None

    def __lt__(self, other: "HookHandler") -> bool:
        """Sort by priority (lower number = higher priority)."""
        return self.priority < other.priority


class HookRegistry:
    """
    Manages lifecycle hooks with deterministic execution.
    Hooks execute sequentially by priority with short-circuit on deny.
    """

    # Standard lifecycle events
    SESSION_START = "session:start"
    SESSION_END = "session:end"
    PROMPT_SUBMIT = "prompt:submit"
    TOOL_PRE = "tool:pre"
    TOOL_POST = "tool:post"
    CONTEXT_PRE_COMPACT = "context:pre-compact"
    AGENT_SPAWN = "agent:spawn"
    AGENT_COMPLETE = "agent:complete"
    ORCHESTRATOR_COMPLETE = "orchestrator:complete"
    USER_NOTIFICATION = "user:notification"

    # Decision events
    DECISION_TOOL_RESOLUTION = "decision:tool_resolution"
    DECISION_AGENT_RESOLUTION = "decision:agent_resolution"
    DECISION_CONTEXT_RESOLUTION = "decision:context_resolution"

    # Error events
    ERROR_TOOL = "error:tool"
    ERROR_PROVIDER = "error:provider"
    ERROR_ORCHESTRATION = "error:orchestration"

    def __init__(self):
        """Initialize empty hook registry."""
        self._handlers: dict[str, list[HookHandler]] = defaultdict(list)

    def register(
        self,
        event: str,
        handler: Callable[[str, dict[str, Any]], Awaitable[HookResult]],
        priority: int = 0,
        name: str | None = None,
    ) -> Callable[[], None]:
        """
        Register a hook handler for an event.

        Args:
            event: Event name to hook into
            handler: Async function that handles the event
            priority: Execution priority (lower = earlier)
            name: Optional handler name for debugging

        Returns:
            Unregister function
        """
        hook_handler = HookHandler(handler=handler, priority=priority, name=name or handler.__name__)

        self._handlers[event].append(hook_handler)
        self._handlers[event].sort()  # Keep sorted by priority

        logger.debug(f"Registered hook '{hook_handler.name}' for event '{event}' with priority {priority}")

        def unregister():
            """Remove this handler from the registry."""
            if hook_handler in self._handlers[event]:
                self._handlers[event].remove(hook_handler)
                logger.debug(f"Unregistered hook '{hook_handler.name}' from event '{event}'")

        return unregister

    # Alias for backwards compatibility
    on = register

    def set_default_fields(self, **defaults):
        """
        Set default fields that will be merged with all emitted events.

        Args:
            **defaults: Key-value pairs to include in all events
        """
        self._defaults = defaults
        logger.debug(f"Set default fields: {list(defaults.keys())}")

    async def emit(self, event: str, data: dict[str, Any]) -> HookResult:
        """
        Emit an event to all registered handlers.

        Handlers execute sequentially by priority with:
        - Short-circuit on 'deny' action
        - Data modification chaining on 'modify' action
        - Continue on 'continue' action

        Args:
            event: Event name
            data: Event data (may be modified by handlers)

        Returns:
            Final hook result after all handlers
        """
        handlers = self._handlers.get(event, [])

        if not handlers:
            logger.debug(f"No handlers for event '{event}'")
            return HookResult(action="continue", data=data)

        logger.debug(f"Emitting event '{event}' to {len(handlers)} handlers")

        # Merge default fields (e.g., session_id) with explicit event data.
        # Explicit event data takes precedence over defaults.
        defaults = getattr(self, "_defaults", {})
        current_data = {**(defaults or {}), **(data or {})}

        # Track special actions to return
        special_result = None

        for hook_handler in handlers:
            try:
                # Call handler with event and current data
                result = await hook_handler.handler(event, current_data)

                if not isinstance(result, HookResult):
                    logger.warning(f"Handler '{hook_handler.name}' returned invalid result type")
                    continue

                if result.action == "deny":
                    logger.info(f"Event '{event}' denied by handler '{hook_handler.name}': {result.reason}")
                    return result

                if result.action == "modify" and result.data is not None:
                    current_data = result.data
                    logger.debug(f"Handler '{hook_handler.name}' modified event data")

                # Preserve special actions (inject_context, ask_user) to return
                if result.action in ("inject_context", "ask_user") and special_result is None:
                    special_result = result
                    logger.debug(f"Handler '{hook_handler.name}' returned special action: {result.action}")

            except Exception as e:
                logger.error(f"Error in hook handler '{hook_handler.name}' for event '{event}': {e}")
                # Continue with other handlers even if one fails

        # Return special action if any hook requested it, otherwise continue
        if special_result:
            return special_result

        # Return final result with potentially modified data
        return HookResult(action="continue", data=current_data)

    async def emit_and_collect(self, event: str, data: dict[str, Any], timeout: float = 1.0) -> list[Any]:
        """
        Emit event and collect all handler responses.

        Unlike emit() which does fire-and-forget, this method:
        - Emits event to all handlers
        - Collects their responses
        - Returns list of responses for decision reduction
        - Has timeout to prevent blocking

        Args:
            event: Event name
            data: Event data
            timeout: Max time to wait for each handler (seconds)

        Returns:
            List of responses from handlers (non-None HookResult.data values)
        """
        handlers = self._handlers.get(event, [])

        if not handlers:
            logger.debug(f"No handlers for event '{event}'")
            return []

        logger.debug(f"Collecting responses for event '{event}' from {len(handlers)} handlers")

        responses = []
        for hook_handler in handlers:
            try:
                # Call handler with timeout
                result = await asyncio.wait_for(hook_handler.handler(event, data), timeout=timeout)

                if not isinstance(result, HookResult):
                    logger.warning(f"Handler '{hook_handler.name}' returned invalid result type")
                    continue

                # Collect response data if present
                if result.data is not None:
                    responses.append(result.data)
                    logger.debug(f"Collected response from handler '{hook_handler.name}'")

            except TimeoutError:
                logger.warning(f"Handler '{hook_handler.name}' timed out after {timeout}s")
            except Exception as e:
                logger.error(f"Error in hook handler '{hook_handler.name}' for event '{event}': {e}")
                # Continue with other handlers

        logger.debug(f"Collected {len(responses)} responses for event '{event}'")
        return responses

    def list_handlers(self, event: str | None = None) -> dict[str, list[str]]:
        """
        List registered handlers.

        Args:
            event: Optional event to filter by

        Returns:
            Dict of event names to handler names
        """
        if event:
            handlers = self._handlers.get(event, [])
            return {event: [h.name for h in handlers if h.name is not None]}
        return {evt: [h.name for h in handlers if h.name is not None] for evt, handlers in self._handlers.items()}
