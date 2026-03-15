"""Tests for run_orchestrator in _session_exec.py.

The run_orchestrator function is a thin Python boundary call.
Rust validates mount-point presence before calling this function.
Python-side, we just retrieve and call.
"""

from unittest.mock import AsyncMock, Mock

import pytest

from amplifier_core._session_exec import run_orchestrator


class MockCoordinator:
    """Mock coordinator for testing run_orchestrator."""

    def __init__(self, mount_points=None):
        self._mount_points = mount_points or {}
        self.hooks = Mock()

    def get(self, key):
        return self._mount_points.get(key)


@pytest.mark.asyncio
async def test_run_orchestrator_does_not_raise_for_missing_providers():
    """run_orchestrator must not raise RuntimeError for missing providers.

    Rust validates mount points before calling this function.
    The Python side uses `or {}` to default to empty dict.
    """
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="response")
    mock_context = Mock()

    coordinator = MockCoordinator(
        {
            "orchestrator": mock_orchestrator,
            "context": mock_context,
            # No "providers" mounted
        }
    )

    # Must NOT raise RuntimeError("No providers mounted")
    result = await run_orchestrator(coordinator, "hello")
    assert result == "response"


@pytest.mark.asyncio
async def test_run_orchestrator_passes_empty_providers_when_none_mounted():
    """When no providers are mounted, passes {} as providers to orchestrator."""
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="response")
    mock_context = Mock()

    coordinator = MockCoordinator(
        {
            "orchestrator": mock_orchestrator,
            "context": mock_context,
            # No "providers" mounted
        }
    )

    await run_orchestrator(coordinator, "hello")

    # orchestrator.execute should be called with empty providers
    mock_orchestrator.execute.assert_called_once()
    call_kwargs = mock_orchestrator.execute.call_args[1]
    assert call_kwargs["providers"] == {}


@pytest.mark.asyncio
async def test_run_orchestrator_calls_orchestrator_execute_with_all_kwargs():
    """run_orchestrator calls orchestrator.execute with correct kwargs."""
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="test response")
    mock_context = Mock()
    mock_providers = {"mock": Mock()}
    mock_tools = {"tool1": Mock()}
    mock_hooks = Mock()

    coordinator = MockCoordinator(
        {
            "orchestrator": mock_orchestrator,
            "context": mock_context,
            "providers": mock_providers,
            "tools": mock_tools,
        }
    )
    coordinator.hooks = mock_hooks

    result = await run_orchestrator(coordinator, "test prompt")

    assert result == "test response"
    mock_orchestrator.execute.assert_called_once_with(
        prompt="test prompt",
        context=mock_context,
        providers=mock_providers,
        tools=mock_tools,
        hooks=mock_hooks,
        coordinator=coordinator,
    )


@pytest.mark.asyncio
async def test_run_orchestrator_passes_empty_tools_when_none_mounted():
    """When no tools are mounted, passes {} as tools to orchestrator."""
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="response")
    mock_context = Mock()
    mock_providers = {"mock": Mock()}

    coordinator = MockCoordinator(
        {
            "orchestrator": mock_orchestrator,
            "context": mock_context,
            "providers": mock_providers,
            # No "tools" mounted
        }
    )

    await run_orchestrator(coordinator, "hello")

    call_kwargs = mock_orchestrator.execute.call_args[1]
    assert call_kwargs["tools"] == {}
