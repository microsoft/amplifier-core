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
        # Create a mock session for testing
        from amplifier_core.session import AmplifierSession

        mock_session = AmplifierSession(config={}, session_id="test-session")
        super().__init__(session=mock_session)
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
        self.should_compact = AsyncMock(return_value=False)
        self.compact = AsyncMock()
        self.clear = AsyncMock()

    async def _add_message(self, message: dict):
        self.messages.append(message)


class EventRecorder:
    """Records lifecycle events for testing."""

    def __init__(self):
        self.events: list[tuple] = []

    async def record(self, event: str, data: dict) -> HookResult:
        """Record an event."""
        self.events.append((event, data.copy()))
        return HookResult(action="continue")

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
