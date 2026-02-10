"""Tests for asyncio.CancelledError resilience across kernel exception-handling sites.

Python 3.9+ made CancelledError a BaseException subclass, so bare `except Exception`
misses it. These tests verify each fixed site handles CancelledError correctly.
"""

import asyncio
from unittest.mock import AsyncMock, MagicMock

import pytest

from amplifier_core.cancellation import CancellationToken
from amplifier_core.coordinator import ModuleCoordinator
from amplifier_core.hooks import HookRegistry
from amplifier_core.models import HookResult
from amplifier_core.session import AmplifierSession


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def coordinator():
    """Create a minimal coordinator for testing."""

    class MockSession:
        session_id = "test-session"

    mock_session = MockSession()
    return ModuleCoordinator(session=mock_session)  # type: ignore[arg-type]


# ---------------------------------------------------------------------------
# 1. collect_contributions — CancelledError breaks, Exception continues
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_collect_contributions_breaks_on_cancelled_error(coordinator):
    """CancelledError in a contributor stops collection and returns partial results."""
    called = []

    coordinator.register_contributor("ch", "mod1", lambda: "result-1")

    async def cancelling_contributor():
        called.append("mod2")
        raise asyncio.CancelledError()

    coordinator.register_contributor("ch", "mod2", cancelling_contributor)

    def mod3_contributor():
        called.append("mod3")
        return "result-3"

    coordinator.register_contributor("ch", "mod3", mod3_contributor)

    contributions = await coordinator.collect_contributions("ch")

    # Only mod1 returned before cancellation; mod3 never ran
    assert contributions == ["result-1"]
    assert "mod2" in called
    assert "mod3" not in called


@pytest.mark.asyncio
async def test_collect_contributions_exception_continues(coordinator):
    """A regular Exception in a contributor doesn't stop collection."""
    coordinator.register_contributor("ch", "mod1", lambda: "result-1")

    def failing_contributor():
        raise RuntimeError("boom")

    coordinator.register_contributor("ch", "mod2", failing_contributor)
    coordinator.register_contributor("ch", "mod3", lambda: "result-3")

    contributions = await coordinator.collect_contributions("ch")

    assert "result-1" in contributions
    assert "result-3" in contributions
    assert len(contributions) == 2


# ---------------------------------------------------------------------------
# 2. coordinator.cleanup — survives CancelledError, re-raises fatal exceptions
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_cleanup_continues_after_cancelled_error(coordinator):
    """All cleanup functions run even if one raises CancelledError.

    CancelledError is a BaseException (not Exception), so it is tracked as
    fatal and re-raised after all cleanup functions complete.
    """
    called = []

    async def cleanup1():
        called.append(1)
        raise asyncio.CancelledError()

    def cleanup2():
        called.append(2)

    def cleanup3():
        called.append(3)

    coordinator.register_cleanup(cleanup1)
    coordinator.register_cleanup(cleanup2)
    coordinator.register_cleanup(cleanup3)

    # cleanup runs in reverse order: 3, 2, 1
    # CancelledError is re-raised after all cleanup completes
    with pytest.raises(asyncio.CancelledError):
        await coordinator.cleanup()

    assert sorted(called) == [1, 2, 3]


@pytest.mark.asyncio
async def test_cleanup_reraises_keyboard_interrupt_after_completing(coordinator):
    """KeyboardInterrupt is re-raised after all cleanup functions have run."""
    called = []

    def cleanup1():
        called.append(1)
        raise KeyboardInterrupt()

    def cleanup2():
        called.append(2)

    coordinator.register_cleanup(cleanup1)
    coordinator.register_cleanup(cleanup2)

    # Reverse order: cleanup2 runs first, then cleanup1 raises
    with pytest.raises(KeyboardInterrupt):
        await coordinator.cleanup()

    assert sorted(called) == [1, 2]


@pytest.mark.asyncio
async def test_cleanup_reraises_system_exit_after_completing(coordinator):
    """SystemExit is re-raised after all cleanup functions have run."""
    called = []

    def cleanup1():
        called.append(1)
        raise SystemExit(1)

    def cleanup2():
        called.append(2)

    coordinator.register_cleanup(cleanup1)
    coordinator.register_cleanup(cleanup2)

    with pytest.raises(SystemExit):
        await coordinator.cleanup()

    assert sorted(called) == [1, 2]


# ---------------------------------------------------------------------------
# 3. session.cleanup — try/finally ensures loader.cleanup always runs
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_cleanup_runs_loader_even_if_coordinator_fails():
    """loader.cleanup() runs even when coordinator.cleanup() raises CancelledError."""
    minimal_config = {
        "session": {
            "orchestrator": "test-orch",
            "context": "test-ctx",
        },
    }
    session = AmplifierSession(config=minimal_config)

    # Mock coordinator.cleanup to raise CancelledError
    session.coordinator.cleanup = AsyncMock(side_effect=asyncio.CancelledError())
    # Mock loader.cleanup to track that it was called
    session.loader.cleanup = MagicMock()

    with pytest.raises(asyncio.CancelledError):
        await session.cleanup()

    session.loader.cleanup.assert_called_once()


# ---------------------------------------------------------------------------
# 4. hooks.emit — CancelledError in a handler doesn't skip remaining handlers
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_emit_continues_after_cancelled_error():
    """All hook handlers run even if one raises CancelledError."""
    registry = HookRegistry()
    called = []

    async def handler1(event, data):
        called.append("h1")
        raise asyncio.CancelledError()

    async def handler2(event, data):
        called.append("h2")
        return HookResult(action="continue")

    async def handler3(event, data):
        called.append("h3")
        return HookResult(action="continue")

    registry.register("test:event", handler1, priority=0, name="h1")
    registry.register("test:event", handler2, priority=1, name="h2")
    registry.register("test:event", handler3, priority=2, name="h3")

    result = await registry.emit("test:event", {"key": "value"})

    assert called == ["h1", "h2", "h3"]
    assert result.action == "continue"


# ---------------------------------------------------------------------------
# 5. hooks.emit_and_collect — same pattern for the collection variant
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_emit_and_collect_continues_after_cancelled_error():
    """emit_and_collect runs all handlers even if one raises CancelledError."""
    registry = HookRegistry()
    called = []

    async def handler1(event, data):
        called.append("h1")
        raise asyncio.CancelledError()

    async def handler2(event, data):
        called.append("h2")
        return HookResult(action="continue", data={"from": "h2"})

    async def handler3(event, data):
        called.append("h3")
        return HookResult(action="continue", data={"from": "h3"})

    registry.register("test:event", handler1, priority=0, name="h1")
    registry.register("test:event", handler2, priority=1, name="h2")
    registry.register("test:event", handler3, priority=2, name="h3")

    responses = await registry.emit_and_collect("test:event", {"key": "value"})

    assert called == ["h1", "h2", "h3"]
    assert len(responses) == 2
    assert {"from": "h2"} in responses
    assert {"from": "h3"} in responses


# ---------------------------------------------------------------------------
# 6. cancellation.trigger_callbacks — survives CancelledError, re-raises fatal
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_trigger_callbacks_continues_after_cancelled_error():
    """All cancellation callbacks run even if one raises CancelledError."""
    token = CancellationToken()
    called = []

    async def cb1():
        called.append(1)
        raise asyncio.CancelledError()

    async def cb2():
        called.append(2)

    async def cb3():
        called.append(3)

    token.on_cancel(cb1)
    token.on_cancel(cb2)
    token.on_cancel(cb3)

    await token.trigger_callbacks()

    assert called == [1, 2, 3]


@pytest.mark.asyncio
async def test_trigger_callbacks_reraises_keyboard_interrupt_after_completing():
    """KeyboardInterrupt is re-raised after all cancellation callbacks run."""
    token = CancellationToken()
    called = []

    async def cb1():
        called.append(1)
        raise KeyboardInterrupt()

    async def cb2():
        called.append(2)

    token.on_cancel(cb1)
    token.on_cancel(cb2)

    with pytest.raises(KeyboardInterrupt):
        await token.trigger_callbacks()

    assert called == [1, 2]
