"""Tests verifying the module:load_failed observability event.

Provider, tool, and hook modules that raise during load/mount are caught and
logged as a WARNING (non-fatal -- the session still starts with the other
modules loaded). Before this fix, that WARNING was the *only* trace of the
failure: nothing was observable through the kernel's event surface, so a
hook module had no way to detect that a configured tool/provider/hook never
mounted. These tests pin down the mechanism fix: a `module:load_failed`
event, carrying which module type failed, its module_id, and the error.

Mirrors the mocked style of test_session_init_on_session_ready.py.
"""

from unittest.mock import AsyncMock, MagicMock

import pytest
from amplifier_core._session_init import initialize_session
from amplifier_core.events import MODULE_LOAD_FAILED

# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------


def _make_mocks(failing_module_ids, calls=None):
    """Build a mock loader + coordinator whose loader.load() raises for any
    module_id in ``failing_module_ids``, and otherwise succeeds -- including
    for the required orchestrator/context modules ("loop-basic",
    "context-simple"), which must mount successfully so the provider/tool/
    hook loops under test are actually reached.

    Args:
        failing_module_ids: module_id or iterable of module_ids whose
            load() call should raise.
        calls: optional list to record every module_id load() was called
            with, in order (used to assert non-interference).

    Returns:
        (mock_loader, mock_coordinator) tuple.
    """
    if isinstance(failing_module_ids, str):
        failing_module_ids = {failing_module_ids}
    else:
        failing_module_ids = set(failing_module_ids)

    async def load_side_effect(module_id, config, source_hint=None, coordinator=None):
        if calls is not None:
            calls.append(module_id)
        if module_id in failing_module_ids:
            raise RuntimeError(f"{module_id} mount() deliberately raised")

        async def mount_fn(coordinator):
            return None

        return mount_fn

    mock_loader = MagicMock()
    mock_loader.load = AsyncMock(side_effect=load_side_effect)
    mock_loader.get_on_session_ready_queue = MagicMock(return_value=[])
    mock_loader._on_session_ready_queue = []

    mock_coordinator = MagicMock()
    mock_coordinator.loader = mock_loader
    mock_coordinator.register_cleanup = MagicMock()
    mock_coordinator.get = MagicMock(return_value={})
    mock_coordinator.hooks = MagicMock()
    mock_coordinator.hooks.emit = AsyncMock()

    return mock_loader, mock_coordinator


def _tracking_emit(emitted_events):
    async def _emit(event, payload):
        emitted_events.append((event, payload))

    return _emit


_BASE_CONFIG = {
    "session": {"orchestrator": "loop-basic", "context": "context-simple"},
}


# ---------------------------------------------------------------------------
# Tool load failure -- the exact defect described in the issue
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_tool_load_failure_emits_module_load_failed():
    """A tool whose mount() raises is caught (session continues) AND emits
    module:load_failed. Before the fix, the exception was logged at WARNING
    and nothing else observed it -- this test fails against the old code
    because zero events were ever emitted for a tool load failure."""

    _, mock_coordinator = _make_mocks(failing_module_ids="tool-computer-use")
    emitted_events = []
    mock_coordinator.hooks.emit = AsyncMock(side_effect=_tracking_emit(emitted_events))

    config = {
        **_BASE_CONFIG,
        "providers": [],
        "tools": [{"module": "tool-computer-use"}],
        "hooks": [],
    }

    # Must not raise -- tool failures are non-fatal by design (unlike
    # orchestrator/context). The defect is silence, not the non-fatal-ness.
    await initialize_session(
        config, mock_coordinator, session_id="test-session", parent_id=None
    )

    failures = [(e, p) for e, p in emitted_events if e == MODULE_LOAD_FAILED]
    assert len(failures) == 1, (
        f"Expected exactly one module:load_failed event, got: {emitted_events}"
    )
    payload = failures[0][1]
    assert payload["module_type"] == "tool"
    assert payload["module_id"] == "tool-computer-use"
    assert "tool-computer-use mount() deliberately raised" in payload["error"]


@pytest.mark.asyncio
async def test_tool_load_failure_does_not_abort_remaining_tools():
    """One tool's mount() raising must not prevent the next tool from loading
    -- non-interference must be preserved by this fix, not just the event."""

    calls: list[str] = []
    _, mock_coordinator = _make_mocks(failing_module_ids="tool-broken", calls=calls)

    config = {
        **_BASE_CONFIG,
        "providers": [],
        "tools": [{"module": "tool-broken"}, {"module": "tool-ok"}],
        "hooks": [],
    }

    await initialize_session(
        config, mock_coordinator, session_id="test-session", parent_id=None
    )

    assert calls == ["loop-basic", "context-simple", "tool-broken", "tool-ok"], (
        "tool-ok must still be attempted after tool-broken raised"
    )


# ---------------------------------------------------------------------------
# Provider and hook load failure -- identical shape to tools; same fix applies
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_provider_load_failure_emits_module_load_failed():
    _, mock_coordinator = _make_mocks(failing_module_ids="provider-anthropic")
    emitted_events = []
    mock_coordinator.hooks.emit = AsyncMock(side_effect=_tracking_emit(emitted_events))

    config = {
        **_BASE_CONFIG,
        "providers": [{"module": "provider-anthropic"}],
        "tools": [],
        "hooks": [],
    }

    await initialize_session(
        config, mock_coordinator, session_id="test-session", parent_id=None
    )

    failures = [(e, p) for e, p in emitted_events if e == MODULE_LOAD_FAILED]
    assert len(failures) == 1
    assert failures[0][1]["module_type"] == "provider"
    assert failures[0][1]["module_id"] == "provider-anthropic"


@pytest.mark.asyncio
async def test_hook_load_failure_emits_module_load_failed():
    _, mock_coordinator = _make_mocks(failing_module_ids="hook-logging")
    emitted_events = []
    mock_coordinator.hooks.emit = AsyncMock(side_effect=_tracking_emit(emitted_events))

    config = {
        **_BASE_CONFIG,
        "providers": [],
        "tools": [],
        "hooks": [{"module": "hook-logging"}],
    }

    await initialize_session(
        config, mock_coordinator, session_id="test-session", parent_id=None
    )

    failures = [(e, p) for e, p in emitted_events if e == MODULE_LOAD_FAILED]
    assert len(failures) == 1
    assert failures[0][1]["module_type"] == "hook"
    assert failures[0][1]["module_id"] == "hook-logging"


# ---------------------------------------------------------------------------
# Emission failure must not suppress the original warning / abort the session
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_event_emission_failure_does_not_propagate():
    """If coordinator.hooks.emit() itself raises, initialize_session must
    still complete -- mirrors the on_session_ready precedent at
    _session_init.py's Phase 6 (event emission failure must never suppress
    the original warning or abort session init)."""

    _, mock_coordinator = _make_mocks(failing_module_ids="tool-computer-use")
    mock_coordinator.hooks.emit = AsyncMock(side_effect=RuntimeError("emit is broken"))

    config = {
        **_BASE_CONFIG,
        "providers": [],
        "tools": [{"module": "tool-computer-use"}],
        "hooks": [],
    }

    # Must not raise even though hooks.emit() itself raises.
    await initialize_session(
        config, mock_coordinator, session_id="test-session", parent_id=None
    )
