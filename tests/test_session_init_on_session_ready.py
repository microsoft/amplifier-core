"""Tests verifying Phase 6 on_session_ready() dispatch in initialize_session().

Phase 6 should call loader.get_on_session_ready_queue() after all modules are
loaded and invoke each on_session_ready callback with the coordinator.
"""

from unittest.mock import AsyncMock
from unittest.mock import MagicMock

import pytest

from amplifier_core._session_init import initialize_session


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

_MINIMAL_CONFIG = {
    "session": {
        "orchestrator": "loop-basic",
        "context": "context-simple",
    },
    "providers": [],
    "tools": [],
    "hooks": [],
}


def _make_mocks(on_session_ready_queue):
    """Build a mock loader + coordinator with a configurable on_session_ready_queue.

    Args:
        on_session_ready_queue: List of (module_id, on_session_ready_fn) tuples
            returned by loader.get_on_session_ready_queue().

    Returns:
        (mock_loader, mock_coordinator) tuple.
    """
    # mount function returned by loader.load() — no cleanup
    mock_mount_fn = AsyncMock(return_value=None)

    mock_loader = MagicMock()
    mock_loader.load = AsyncMock(return_value=mock_mount_fn)
    # Sync mock — B4 fix removes coroutine guard; this must be a plain MagicMock
    mock_loader.get_on_session_ready_queue = MagicMock(
        return_value=on_session_ready_queue
    )
    # Provide a real list so _session_init can append/clear without error
    mock_loader._on_session_ready_queue = []

    mock_coordinator = MagicMock()
    mock_coordinator.loader = mock_loader
    # register_cleanup is synchronous
    mock_coordinator.register_cleanup = MagicMock()
    # hooks.emit is async
    mock_coordinator.hooks = MagicMock()
    mock_coordinator.hooks.emit = AsyncMock()

    return mock_loader, mock_coordinator


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_on_session_ready_called_after_all_phases():
    """get_on_session_ready_queue is called once and on_session_ready callbacks are invoked."""
    on_session_ready_cb = AsyncMock()
    queue = [("test-module", on_session_ready_cb)]

    mock_loader, mock_coordinator = _make_mocks(queue)

    await initialize_session(
        _MINIMAL_CONFIG,
        mock_coordinator,
        session_id="test-session",
        parent_id=None,
    )

    # Phase 6 must call get_on_session_ready_queue exactly once
    mock_loader.get_on_session_ready_queue.assert_called_once()
    # The on_session_ready callback must be invoked with the coordinator
    on_session_ready_cb.assert_called_once_with(mock_coordinator)


@pytest.mark.asyncio
async def test_on_session_ready_failure_is_nonfatal():
    """A failing on_session_ready does not prevent other on_session_ready calls."""
    results = []

    async def failing_on_session_ready(coordinator):
        raise RuntimeError("on_session_ready failed intentionally")

    async def succeeding_on_session_ready(coordinator):
        results.append("success")

    queue = [
        ("mod-a", failing_on_session_ready),
        ("mod-b", succeeding_on_session_ready),
    ]

    mock_loader, mock_coordinator = _make_mocks(queue)

    # initialize_session must not raise even though the first on_session_ready fails
    await initialize_session(
        _MINIMAL_CONFIG,
        mock_coordinator,
        session_id="test-session",
        parent_id=None,
    )

    # The second callback must still have run
    assert "success" in results


@pytest.mark.asyncio
async def test_on_session_ready_failure_emits_event():
    """A failing on_session_ready() emits module:on_session_ready_failed event."""
    from amplifier_core.events import MODULE_ON_SESSION_READY_FAILED

    emitted_events = []

    async def failing_on_sr(coordinator):
        raise RuntimeError("intentional failure")

    mock_loader, mock_coordinator = _make_mocks([("failing-module", failing_on_sr)])

    async def tracking_emit(event, payload):
        emitted_events.append((event, payload))

    mock_coordinator.hooks.emit = AsyncMock(side_effect=tracking_emit)

    await initialize_session(
        _MINIMAL_CONFIG,
        mock_coordinator,
        session_id="test-session",
        parent_id=None,
    )

    # Check the failure event was emitted
    failure_events = [(e, p) for e, p in emitted_events if e == MODULE_ON_SESSION_READY_FAILED]
    assert len(failure_events) == 1
    assert failure_events[0][1]["module_id"] == "failing-module"
    assert "intentional failure" in failure_events[0][1]["error"]


@pytest.mark.asyncio
async def test_on_session_ready_runs_before_session_fork():
    """on_session_ready completes before session:fork is emitted."""
    from amplifier_core.events import SESSION_FORK

    event_order = []

    async def tracking_on_session_ready(coordinator):
        event_order.append("on_session_ready")

    queue = [("track-mod", tracking_on_session_ready)]

    mock_loader, mock_coordinator = _make_mocks(queue)

    # Wrap the mock emit so we can observe when SESSION_FORK fires
    original_emit = mock_coordinator.hooks.emit

    async def tracking_emit(event, payload):
        if event == SESSION_FORK:
            event_order.append("session:fork")
        return await original_emit(event, payload)

    mock_coordinator.hooks.emit = tracking_emit

    # Pass parent_id to trigger the session:fork path
    await initialize_session(
        _MINIMAL_CONFIG,
        mock_coordinator,
        session_id="test-session",
        parent_id="parent-session",
    )

    assert "on_session_ready" in event_order, "on_session_ready not called"
    assert event_order.index("on_session_ready") < event_order.index("session:fork"), (
        "on_session_ready must run before session:fork is emitted"
    )
