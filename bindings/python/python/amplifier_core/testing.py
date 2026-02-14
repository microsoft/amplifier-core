"""
Testing utilities for Amplifier core.
Provides test fixtures and helpers for module testing.
"""

import asyncio
from collections.abc import Callable
from typing import Any
from unittest.mock import AsyncMock

from amplifier_core import HookResult
from amplifier_core import ModuleCoordinator
from amplifier_core import ToolResult


class TestCoordinator(ModuleCoordinator):
    """Test coordinator with additional debugging capabilities."""

    def __init__(self):
        # Create mock approval/display systems to suppress warnings during testing/validation
        mock_approval = AsyncMock(return_value={"approved": True})
        mock_display = AsyncMock()

        # Create a mock session for testing with minimal valid config
        # Pass mock systems to avoid warnings during session creation
        from amplifier_core.session import AmplifierSession

        minimal_config = {
            "session": {
                "orchestrator": "test-orchestrator",
                "context": "test-context",
            }
        }
        mock_session = AmplifierSession(
            config=minimal_config,
            session_id="test-session",
            approval_system=mock_approval,
            display_system=mock_display,
        )

        # Use the session's coordinator (which already has the mock systems)
        # Don't call super().__init__ - just copy what we need from the session's coordinator
        coord = mock_session.coordinator
        self._session = mock_session
        self.mount_points = coord.mount_points
        self._cleanup_functions = coord._cleanup_functions
        self._capabilities = coord._capabilities
        self.channels = coord.channels
        self.hooks = coord.hooks
        self.approval_system = coord.approval_system
        self.display_system = coord.display_system
        self._current_turn_injections = 0
        self.mount_history = []
        self.unmount_history = []

    async def mount(self, mount_point: str, module: Any, name: str | None = None):
        """Track mount operations."""
        self.mount_history.append({"mount_point": mount_point, "module": module, "name": name})
        await super().mount(mount_point, module, name)

    async def unmount(self, mount_point: str, name: str | None = None):
        """Track unmount operations."""
        self.unmount_history.append({"mount_point": mount_point, "name": name})
        await super().unmount(mount_point, name)


class MockTool:
    """Mock tool for testing."""

    def __init__(self, name: str = "mock_tool", output: Any = "Success"):
        self.name = name
        self.description = f"Mock tool: {name}"
        self.output = output
        self.input_schema = {"type": "object", "properties": {}}  # Minimal schema
        self.execute = AsyncMock(side_effect=self._execute)
        self.call_count = 0

    async def _execute(self, input: dict) -> ToolResult:
        self.call_count += 1
        return ToolResult(success=True, output=self.output)


class MockContextManager:
    """Mock context manager for testing."""

    def __init__(self, messages: list[dict] | None = None):
        self.messages = messages or []
        self.add_message = AsyncMock(side_effect=self._add_message)
        self.get_messages = AsyncMock(return_value=self.messages)
        self.get_messages_for_request = AsyncMock(side_effect=self._get_messages_for_request)
        self.clear = AsyncMock()
        # Internal compaction methods (not called by orchestrators)
        self._should_compact = AsyncMock(return_value=False)
        self._compact_internal = AsyncMock()

    async def _add_message(self, message: dict):
        self.messages.append(message)

    async def _get_messages_for_request(
        self, token_budget: int | None = None, provider: Any | None = None
    ) -> list[dict]:
        """Get messages ready for LLM request (handles compaction internally)."""
        return self.messages.copy()


class EventRecorder:
    """Records lifecycle events for testing.

    Implements the HookRegistry interface for emit() to allow use
    as a mock hooks object in orchestrator tests.
    """

    def __init__(self):
        self.events: list[tuple] = []

    async def emit(self, event: str, data: dict) -> HookResult:
        """Emit (record) an event - compatible with HookRegistry.emit()."""
        self.events.append((event, data.copy()))
        return HookResult(action="continue")

    async def record(self, event: str, data: dict) -> HookResult:
        """Record an event (convenience alias for emit)."""
        return await self.emit(event, data)

    def clear(self):
        """Clear recorded events."""
        self.events.clear()

    def get_events(self, event_type: str | None = None) -> list[tuple]:
        """Get recorded events, optionally filtered by type."""
        if event_type:
            return [e for e in self.events if e[0] == event_type]
        return self.events.copy()


class ScriptedOrchestrator:
    """Orchestrator that returns scripted responses for testing."""

    def __init__(self, responses: list[str]):
        self.responses = responses
        self.call_count = 0

    async def execute(self, prompt: str, context, providers, tools, hooks) -> str:
        if self.call_count < len(self.responses):
            response = self.responses[self.call_count]
        else:
            response = "DONE"

        self.call_count += 1

        # Emit lifecycle events for testing
        await hooks.emit("session:start", {"prompt": prompt})
        await context.add_message({"role": "user", "content": prompt})
        await context.add_message({"role": "assistant", "content": response})
        await hooks.emit("session:end", {"response": response})

        return response


def create_test_coordinator() -> TestCoordinator:
    """Create a test coordinator with basic setup."""
    coordinator = TestCoordinator()

    # Add mock tools
    coordinator.mount_points["tools"]["echo"] = MockTool("echo", "Echo response")
    coordinator.mount_points["tools"]["fail"] = MockTool("fail", None)

    # Add mock context
    coordinator.mount_points["context"] = MockContextManager()

    return coordinator


async def wait_for(condition: Callable[[], bool], timeout: float = 1.0) -> bool:
    """
    Wait for a condition to become true.

    Args:
        condition: Function that returns True when condition is met
        timeout: Maximum time to wait in seconds

    Returns:
        True if condition was met, False if timeout
    """
    start = asyncio.get_event_loop().time()

    while asyncio.get_event_loop().time() - start < timeout:
        if condition():
            return True
        await asyncio.sleep(0.01)

    return False
