"""
Cancellation primitives for cooperative session cancellation.

The kernel provides the MECHANISM (token with state).
The app layer provides the POLICY (when to cancel).
"""

import asyncio
import logging
from dataclasses import dataclass, field
from enum import Enum
from typing import TYPE_CHECKING, Awaitable, Callable, Set

if TYPE_CHECKING:
    pass  # Future: may need coordinator reference


class CancellationState(Enum):
    """Cancellation state machine states."""

    NONE = "none"  # Running normally
    GRACEFUL = "graceful"  # Waiting for current tools to complete
    IMMEDIATE = "immediate"  # Stop now, synthesize results


@dataclass(eq=False)
class CancellationToken:
    """
    Cancellation token for cooperative cancellation.

    Lives in ModuleCoordinator. Orchestrators and tools check this
    to determine if they should stop.

    Design: Kernel provides mechanism (token), app provides policy
    (when to trigger cancellation).

    State Machine:
        NONE -> GRACEFUL (1st Ctrl+C)
        GRACEFUL -> IMMEDIATE (2nd Ctrl+C or timeout)
        Both -> session.status = "cancelled"

    Example:
        # In orchestrator loop
        if coordinator.cancellation.is_cancelled:
            return self._handle_cancellation(context)

        # Check for graceful (wait for tools) vs immediate (stop now)
        if coordinator.cancellation.is_graceful:
            # Let current tools complete
            pass
        elif coordinator.cancellation.is_immediate:
            # Synthesize cancelled results for pending tools
            pass
    """

    _state: CancellationState = field(default=CancellationState.NONE)
    _running_tools: Set[str] = field(default_factory=set)  # tool_call_ids
    _running_tool_names: dict[str, str] = field(
        default_factory=dict
    )  # tool_call_id -> tool_name
    _child_tokens: Set["CancellationToken"] = field(default_factory=set)
    _on_cancel_callbacks: list[Callable[[], Awaitable[None]]] = field(
        default_factory=list
    )

    @property
    def state(self) -> CancellationState:
        """Current cancellation state."""
        return self._state

    @property
    def is_cancelled(self) -> bool:
        """True if any cancellation requested (graceful or immediate)."""
        return self._state != CancellationState.NONE

    @property
    def is_graceful(self) -> bool:
        """True if graceful cancellation (wait for tools)."""
        return self._state == CancellationState.GRACEFUL

    @property
    def is_immediate(self) -> bool:
        """True if immediate cancellation (stop now)."""
        return self._state == CancellationState.IMMEDIATE

    @property
    def running_tools(self) -> Set[str]:
        """Currently running tool call IDs."""
        return self._running_tools.copy()

    @property
    def running_tool_names(self) -> list[str]:
        """Names of currently running tools (for display)."""
        return list(self._running_tool_names.values())

    def request_graceful(self) -> bool:
        """
        Request graceful cancellation. Waits for current tools to complete.

        Returns:
            True if state changed, False if already cancelled
        """
        if self._state == CancellationState.NONE:
            self._state = CancellationState.GRACEFUL
            self._propagate_to_children()
            return True
        return False

    def request_immediate(self) -> bool:
        """
        Request immediate cancellation. Stops as soon as possible.

        Returns:
            True if state changed
        """
        if self._state != CancellationState.IMMEDIATE:
            self._state = CancellationState.IMMEDIATE
            self._propagate_to_children()
            return True
        return False

    def reset(self) -> None:
        """Reset cancellation state. Called when starting a new turn."""
        self._state = CancellationState.NONE
        self._running_tools.clear()
        self._running_tool_names.clear()
        # Note: Don't clear child tokens or callbacks - those are session-level

    def register_tool_start(self, tool_call_id: str, tool_name: str) -> None:
        """Register a tool as starting execution."""
        self._running_tools.add(tool_call_id)
        self._running_tool_names[tool_call_id] = tool_name

    def register_tool_complete(self, tool_call_id: str) -> None:
        """Register a tool as completed."""
        self._running_tools.discard(tool_call_id)
        self._running_tool_names.pop(tool_call_id, None)

    def register_child(self, child_token: "CancellationToken") -> None:
        """Register a child session's token for propagation."""
        self._child_tokens.add(child_token)
        # Propagate current state to new child
        if self._state == CancellationState.GRACEFUL:
            child_token.request_graceful()
        elif self._state == CancellationState.IMMEDIATE:
            child_token.request_immediate()

    def unregister_child(self, child_token: "CancellationToken") -> None:
        """Unregister a child session's token."""
        self._child_tokens.discard(child_token)

    def _propagate_to_children(self) -> None:
        """Propagate cancellation state to all children."""
        for child in self._child_tokens:
            if self._state == CancellationState.GRACEFUL:
                child.request_graceful()
            elif self._state == CancellationState.IMMEDIATE:
                child.request_immediate()

    def on_cancel(self, callback: Callable[[], Awaitable[None]]) -> None:
        """Register callback to be called on cancellation."""
        self._on_cancel_callbacks.append(callback)

    async def trigger_callbacks(self) -> None:
        """Trigger all registered cancellation callbacks."""
        _logger = logging.getLogger(__name__)
        first_fatal = None
        for callback in self._on_cancel_callbacks:
            try:
                await callback()
            except asyncio.CancelledError:
                # CancelledError is a BaseException (Python 3.9+). Log and continue
                # so all cancellation callbacks run.
                _logger.warning("CancelledError in cancellation callback")
            except Exception:
                pass  # Don't let callback errors prevent cancellation
            except BaseException as e:
                # Track fatal exceptions (KeyboardInterrupt, SystemExit) for re-raise
                # after all callbacks complete.
                _logger.warning(f"Fatal exception in cancellation callback: {e}")
                if first_fatal is None:
                    first_fatal = e
        if first_fatal is not None:
            raise first_fatal
