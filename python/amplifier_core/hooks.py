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
    # See events.py for the canonical list; these are convenience constants
    # for commonly-hooked events.
    SESSION_START = "session:start"
    SESSION_END = "session:end"
    PROMPT_SUBMIT = "prompt:submit"
    TOOL_PRE = "tool:pre"
    TOOL_POST = "tool:post"
    CONTEXT_PRE_COMPACT = "context:pre_compact"
    ORCHESTRATOR_COMPLETE = "orchestrator:complete"
    USER_NOTIFICATION = "user:notification"

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
        hook_handler = HookHandler(
            handler=handler, priority=priority, name=name or handler.__name__
        )

        self._handlers[event].append(hook_handler)
        self._handlers[event].sort()  # Keep sorted by priority

        logger.debug(
            f"Registered hook '{hook_handler.name}' for event '{event}' with priority {priority}"
        )

        def unregister():
            """Remove this handler from the registry."""
            if hook_handler in self._handlers[event]:
                self._handlers[event].remove(hook_handler)
                logger.debug(
                    f"Unregistered hook '{hook_handler.name}' from event '{event}'"
                )

        return unregister

    # Alias for backwards compatibility
    on = register

    def set_default_fields(self, **defaults):
        """
        Set default fields that will be merged with events emitted via emit().

        Note: These defaults only apply to emit(), not emit_and_collect().

        Args:
            **defaults: Key-value pairs to include in emit() events
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
        # Collect ALL inject_context results to merge them
        inject_context_results: list[HookResult] = []

        for hook_handler in handlers:
            try:
                # Call handler with event and current data
                result = await hook_handler.handler(event, current_data)

                if not isinstance(result, HookResult):
                    logger.warning(
                        f"Handler '{hook_handler.name}' returned invalid result type"
                    )
                    continue

                if result.action == "deny":
                    logger.info(
                        f"Event '{event}' denied by handler '{hook_handler.name}': {result.reason}"
                    )
                    return result

                if result.action == "modify" and result.data is not None:
                    current_data = result.data
                    logger.debug(f"Handler '{hook_handler.name}' modified event data")

                # Collect inject_context actions for merging
                if result.action == "inject_context" and result.context_injection:
                    inject_context_results.append(result)
                    logger.debug(
                        f"Handler '{hook_handler.name}' returned inject_context"
                    )

                # Preserve ask_user (only first one, can't merge approvals)
                if result.action == "ask_user" and special_result is None:
                    special_result = result
                    logger.debug(f"Handler '{hook_handler.name}' returned ask_user")

            except asyncio.CancelledError:
                # CancelledError is a BaseException (Python 3.9+). Log and continue
                # so all handlers observe the event (important for cleanup events
                # like session:end that flow through emit).
                logger.error(
                    f"CancelledError in hook handler '{hook_handler.name}' "
                    f"for event '{event}'"
                )
            except Exception as e:
                logger.error(
                    f"Error in hook handler '{hook_handler.name}' for event '{event}': {e}"
                )
                # Continue with other handlers even if one fails

        # If multiple inject_context results, merge them.
        # Note: ask_user takes precedence over inject_context (security blocking
        # actions must not be silently overwritten by information-flow actions).
        # Action precedence: deny > ask_user > inject_context > modify > continue
        if inject_context_results:
            merged_inject = self._merge_inject_context_results(inject_context_results)
            if special_result is None:
                special_result = merged_inject
                logger.debug(
                    f"Merged {len(inject_context_results)} inject_context results"
                )
            else:
                # ask_user already captured - don't overwrite it
                logger.debug(
                    f"Skipped {len(inject_context_results)} inject_context results "
                    f"due to higher-priority {special_result.action} action"
                )

        # Return special action if any hook requested it, otherwise continue
        if special_result:
            return special_result

        # Return final result with potentially modified data
        return HookResult(action="continue", data=current_data)

    def _merge_inject_context_results(self, results: list[HookResult]) -> HookResult:
        """
        Merge multiple inject_context results into a single result.

        When multiple hooks return inject_context on the same event, combine their
        injections into a single message to avoid losing any hook's contribution.

        Args:
            results: List of HookResult with action="inject_context"

        Returns:
            Single HookResult with combined injections
        """
        if not results:
            return HookResult(action="continue")

        if len(results) == 1:
            return results[0]

        # Combine all injections
        combined_content = "\n\n".join(
            result.context_injection for result in results if result.context_injection
        )

        # Use settings from first result (role, ephemeral, suppress_output)
        first = results[0]

        return HookResult(
            action="inject_context",
            context_injection=combined_content,
            context_injection_role=first.context_injection_role,
            ephemeral=first.ephemeral,
            suppress_output=first.suppress_output,
        )

    async def emit_and_collect(
        self, event: str, data: dict[str, Any], timeout: float = 1.0
    ) -> list[Any]:
        """
        Emit event and collect data from all handler responses.

        Unlike emit() which processes action semantics (deny short-circuits,
        modify chains data, ask_user/inject_context return special results),
        this method simply collects result.data from all handlers for aggregation.

        Use for decision events where multiple hooks propose candidates and you
        need to aggregate/reduce their contributions (e.g., tool resolution,
        agent selection).

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

        logger.debug(
            f"Collecting responses for event '{event}' from {len(handlers)} handlers"
        )

        responses = []
        for hook_handler in handlers:
            try:
                # Call handler with timeout
                result = await asyncio.wait_for(
                    hook_handler.handler(event, data), timeout=timeout
                )

                if not isinstance(result, HookResult):
                    logger.warning(
                        f"Handler '{hook_handler.name}' returned invalid result type"
                    )
                    continue

                # Collect response data if present
                if result.data is not None:
                    responses.append(result.data)
                    logger.debug(
                        f"Collected response from handler '{hook_handler.name}'"
                    )

            except TimeoutError:
                logger.warning(
                    f"Handler '{hook_handler.name}' timed out after {timeout}s"
                )
            except asyncio.CancelledError:
                # CancelledError is a BaseException (Python 3.9+). Log and continue
                # so all handlers get a chance to respond.
                logger.error(
                    f"CancelledError in hook handler '{hook_handler.name}' "
                    f"for event '{event}'"
                )
            except Exception as e:
                logger.error(
                    f"Error in hook handler '{hook_handler.name}' for event '{event}': {e}"
                )
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
        return {
            evt: [h.name for h in handlers if h.name is not None]
            for evt, handlers in self._handlers.items()
        }
