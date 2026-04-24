"""Tests for ModuleLoader on_session_ready lifecycle hook detection.

Verifies that _load_filesystem() detects on_session_ready() and attaches it
to the returned mount function as ``__on_session_ready__`` (B1 attachment
pattern), rejects sync implementations with a warning, and that
get_on_session_ready_queue() returns a defensive copy.

Also verifies that on_session_ready is NOT enqueued when mount() fails (B1
guard: enqueue only after successful mount).
"""

import inspect
import logging
import types
from unittest.mock import AsyncMock
from unittest.mock import MagicMock
from unittest.mock import patch

import pytest

from amplifier_core.loader import ModuleLoader


def _make_module(
    *,
    has_mount: bool = True,
    has_on_session_ready: bool = False,
    on_session_ready_async: bool = True,
) -> types.ModuleType:
    """Create a fake Python module with configurable lifecycle functions."""
    mod = types.ModuleType("amplifier_module_fake")

    if has_mount:

        async def mount(coordinator, config):
            return None

        mod.mount = mount  # type: ignore[attr-defined]

    if has_on_session_ready:
        if on_session_ready_async:

            async def on_session_ready(coordinator):
                pass

            mod.on_session_ready = on_session_ready  # type: ignore[attr-defined]
        else:

            def on_session_ready_sync(coordinator):
                pass

            mod.on_session_ready = on_session_ready_sync  # type: ignore[attr-defined]

    return mod


# ---------------------------------------------------------------------------
# Tests — queue (get_on_session_ready_queue / _on_session_ready_queue)
# ---------------------------------------------------------------------------


def test_queue_empty_initially():
    """New loader has an empty on_session_ready queue."""
    loader = ModuleLoader()
    assert loader.get_on_session_ready_queue() == []


def test_get_on_session_ready_queue_returns_defensive_copy():
    """Mutating the returned list does not affect the internal queue."""
    loader = ModuleLoader()

    async def on_session_ready(coordinator):
        pass

    # Directly append to the internal queue (simulating post-mount state)
    loader._on_session_ready_queue.append(("mod-a", on_session_ready))

    copy = loader.get_on_session_ready_queue()
    copy.clear()  # Mutate the returned copy

    assert len(loader.get_on_session_ready_queue()) == 1, "Internal queue must be unaffected"


def test_clear_on_session_ready_queue():
    """clear_on_session_ready_queue() drains the internal queue."""
    loader = ModuleLoader()

    async def on_session_ready(coordinator):
        pass

    loader._on_session_ready_queue.append(("mod-a", on_session_ready))
    assert len(loader._on_session_ready_queue) == 1

    loader.clear_on_session_ready_queue()
    assert loader._on_session_ready_queue == []


# ---------------------------------------------------------------------------
# Tests — B1 attachment pattern via _load_filesystem()
# ---------------------------------------------------------------------------


def test_async_on_session_ready_is_attached():
    """_load_filesystem attaches __on_session_ready__ to the returned mount fn."""
    loader = ModuleLoader()
    fake_mod = _make_module(
        has_mount=True, has_on_session_ready=True, on_session_ready_async=True
    )

    with patch("importlib.import_module", return_value=fake_mod):
        result = loader._load_filesystem("fake")

    assert result is not None, "mount function should have been returned"
    on_sr = getattr(result, "__on_session_ready__", None)
    assert on_sr is not None, "mount fn must have __on_session_ready__ attribute"
    assert on_sr[0] == "fake"
    assert inspect.iscoroutinefunction(on_sr[1])


def test_async_on_session_ready_not_in_queue_until_session_init():
    """_load_filesystem does NOT directly populate _on_session_ready_queue (B1)."""
    loader = ModuleLoader()
    fake_mod = _make_module(
        has_mount=True, has_on_session_ready=True, on_session_ready_async=True
    )

    with patch("importlib.import_module", return_value=fake_mod):
        loader._load_filesystem("fake")

    # After _load_filesystem, queue must be empty — session_init populates it
    assert loader.get_on_session_ready_queue() == [], (
        "_load_filesystem must not populate the queue (B1 fix: session_init does it)"
    )


def test_no_on_session_ready_no_attachment():
    """_load_filesystem does not attach __on_session_ready__ when absent."""
    loader = ModuleLoader()
    fake_mod = _make_module(has_mount=True, has_on_session_ready=False)

    with patch("importlib.import_module", return_value=fake_mod):
        result = loader._load_filesystem("plain")

    assert result is not None
    assert getattr(result, "__on_session_ready__", None) is None


def test_sync_on_session_ready_warns_and_not_attached(caplog):
    """_load_filesystem warns and does NOT attach sync on_session_ready."""
    loader = ModuleLoader()
    fake_mod = _make_module(
        has_mount=True, has_on_session_ready=True, on_session_ready_async=False
    )

    with (
        patch("importlib.import_module", return_value=fake_mod),
        caplog.at_level(logging.WARNING, logger="amplifier_core.loader"),
    ):
        result = loader._load_filesystem("sync-mod")

    assert result is not None
    assert getattr(result, "__on_session_ready__", None) is None
    assert "sync" in caplog.text.lower() or "async" in caplog.text.lower()


# ---------------------------------------------------------------------------
# Tests — B1: on_session_ready does NOT fire when mount() fails
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_on_session_ready_not_enqueued_when_mount_fails():
    """on_session_ready is NOT queued when mount() raises — enqueue only after success."""
    from amplifier_core._session_init import initialize_session

    did_fire = []

    async def on_session_ready(coordinator):
        did_fire.append(True)

    # A mount function that raises, but has on_session_ready attached
    async def failing_mount(coordinator):
        raise RuntimeError("mount failed")

    failing_mount.__on_session_ready__ = ("failing-module", on_session_ready)

    mock_loader = MagicMock(spec=ModuleLoader)
    # Return the failing mount for every call (orchestrator fails → session raises)
    mock_loader.load = AsyncMock(return_value=failing_mount)
    mock_loader._on_session_ready_queue = []
    mock_loader.get_on_session_ready_queue = MagicMock(
        side_effect=lambda: list(mock_loader._on_session_ready_queue)
    )

    mock_coordinator = MagicMock()
    mock_coordinator.loader = mock_loader
    mock_coordinator.register_cleanup = MagicMock()
    mock_coordinator.hooks = MagicMock()
    mock_coordinator.hooks.emit = AsyncMock()

    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [],
        "tools": [],
        "hooks": [],
    }

    # Orchestrator failure is fatal — session init will raise
    with pytest.raises(RuntimeError, match="Cannot initialize without orchestrator"):
        await initialize_session(config, mock_coordinator, "test-session", None)

    # on_session_ready must NOT have fired
    assert did_fire == [], "on_session_ready fired despite mount() failure"


@pytest.mark.asyncio
async def test_on_session_ready_not_enqueued_for_failed_tool_mount():
    """Tool mount() failure skips on_session_ready enqueue for that module."""
    did_fire = []

    async def on_session_ready_for_failing_tool(coordinator):
        did_fire.append("bad-tool")

    async def failing_tool_mount(coordinator):
        raise RuntimeError("tool mount failed")

    failing_tool_mount.__on_session_ready__ = (
        "bad-tool",
        on_session_ready_for_failing_tool,
    )

    async def good_orchestrator_mount(coordinator):
        return None

    async def good_context_mount(coordinator):
        return None

    # Loader returns good mounts for required modules, failing mount for tool
    async def load_side_effect(module_id, config=None, source_hint=None, coordinator=None):
        if module_id == "loop-basic":
            return good_orchestrator_mount
        if module_id == "context-simple":
            return good_context_mount
        return failing_tool_mount

    mock_loader = MagicMock(spec=ModuleLoader)
    mock_loader.load = AsyncMock(side_effect=load_side_effect)
    mock_loader._on_session_ready_queue = []
    mock_loader.get_on_session_ready_queue = MagicMock(
        side_effect=lambda: list(mock_loader._on_session_ready_queue)
    )

    mock_coordinator = MagicMock()
    mock_coordinator.loader = mock_loader
    mock_coordinator.register_cleanup = MagicMock()
    mock_coordinator.hooks = MagicMock()
    mock_coordinator.hooks.emit = AsyncMock()

    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [],
        "tools": [{"module": "bad-tool"}],
        "hooks": [],
    }

    from amplifier_core._session_init import initialize_session

    await initialize_session(config, mock_coordinator, "test-session", None)

    # on_session_ready for bad-tool must NOT have fired
    assert did_fire == []
