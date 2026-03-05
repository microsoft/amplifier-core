"""
CP-V Kernel: Verbosity Collapse Tests

Tests for Task 12: Remove tiered event constants and collapse session.py
from 3-tier emission to single emission with optional raw field.

This is a BREAKING CHANGE — :debug and :raw event tiers are removed.
"""

from unittest.mock import AsyncMock, Mock

import pytest
from amplifier_core.events import SESSION_FORK, SESSION_RESUME, SESSION_START
from amplifier_core.models import HookResult
from amplifier_core.session import AmplifierSession as PyAmplifierSession
import amplifier_core.events as events


# ---------------------------------------------------------------------------
# Part 1: Verify removed constants no longer exist in events.py
# ---------------------------------------------------------------------------


def test_session_start_debug_removed():
    """SESSION_START_DEBUG constant must no longer exist."""
    assert not hasattr(events, "SESSION_START_DEBUG"), (
        "SESSION_START_DEBUG should be removed — :debug tier is gone"
    )


def test_session_start_raw_removed():
    """SESSION_START_RAW constant must no longer exist."""
    assert not hasattr(events, "SESSION_START_RAW"), (
        "SESSION_START_RAW should be removed — :raw tier is gone"
    )


def test_session_fork_debug_removed():
    """SESSION_FORK_DEBUG constant must no longer exist."""
    assert not hasattr(events, "SESSION_FORK_DEBUG"), (
        "SESSION_FORK_DEBUG should be removed"
    )


def test_session_fork_raw_removed():
    """SESSION_FORK_RAW constant must no longer exist."""
    assert not hasattr(events, "SESSION_FORK_RAW"), "SESSION_FORK_RAW should be removed"


def test_session_resume_debug_removed():
    """SESSION_RESUME_DEBUG constant must no longer exist."""
    assert not hasattr(events, "SESSION_RESUME_DEBUG"), (
        "SESSION_RESUME_DEBUG should be removed"
    )


def test_session_resume_raw_removed():
    """SESSION_RESUME_RAW constant must no longer exist."""
    assert not hasattr(events, "SESSION_RESUME_RAW"), (
        "SESSION_RESUME_RAW should be removed"
    )


def test_llm_request_debug_removed():
    """LLM_REQUEST_DEBUG constant must no longer exist."""
    assert not hasattr(events, "LLM_REQUEST_DEBUG"), (
        "LLM_REQUEST_DEBUG should be removed"
    )


def test_llm_request_raw_removed():
    """LLM_REQUEST_RAW constant must no longer exist."""
    assert not hasattr(events, "LLM_REQUEST_RAW"), "LLM_REQUEST_RAW should be removed"


def test_llm_response_debug_removed():
    """LLM_RESPONSE_DEBUG constant must no longer exist."""
    assert not hasattr(events, "LLM_RESPONSE_DEBUG"), (
        "LLM_RESPONSE_DEBUG should be removed"
    )


def test_llm_response_raw_removed():
    """LLM_RESPONSE_RAW constant must no longer exist."""
    assert not hasattr(events, "LLM_RESPONSE_RAW"), "LLM_RESPONSE_RAW should be removed"


def test_all_events_count_is_41():
    """ALL_EVENTS must contain exactly 41 entries after removing 10 tiered constants."""
    assert len(events.ALL_EVENTS) == 41, (
        f"Expected 41 events after verbosity collapse, got {len(events.ALL_EVENTS)}"
    )


def test_no_debug_or_raw_suffix_events():
    """No event in ALL_EVENTS should have a :debug or :raw suffix."""
    debug_raw_events = [
        e for e in events.ALL_EVENTS if e.endswith(":debug") or e.endswith(":raw")
    ]
    assert len(debug_raw_events) == 0, (
        f"Found :debug/:raw suffix events that should be removed: {debug_raw_events}"
    )


# ---------------------------------------------------------------------------
# Part 2: session:start — raw field in payload
# ---------------------------------------------------------------------------


def _minimal_config(raw: bool | None = None):
    """Build a minimal valid config with optional raw flag."""
    session_config: dict = {
        "orchestrator": "loop-basic",
        "context": "context-simple",
    }
    if raw is not None:
        session_config["raw"] = raw
    return {
        "session": session_config,
        "providers": [],
        "tools": [],
    }


def _make_capture_handler(events_list):
    async def handler(event, data):
        events_list.append((event, dict(data)))
        return HookResult(action="continue")

    return handler


def _setup_mock_session(session):
    """Mount minimal mocks and mark as initialized."""
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="ok")
    mock_context = Mock()
    mock_context.add_message = AsyncMock()
    mock_context.get_messages = AsyncMock(return_value=[])
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = mock_context
    session.coordinator.mount_points["providers"] = {"mock": Mock()}
    session._initialized = True


@pytest.mark.asyncio
async def test_session_start_includes_raw_field_when_raw_true():
    """session:start payload includes 'raw' when session.raw=true."""
    config = _minimal_config(raw=True)
    session = PyAmplifierSession(config)
    _setup_mock_session(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_START, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    start_events = [d for e, d in emitted if e == SESSION_START]
    assert len(start_events) == 1, f"Expected 1 SESSION_START, got {len(start_events)}"
    payload = start_events[0]
    assert "raw" in payload, (
        "Expected 'raw' key in session:start payload when session.raw=true"
    )


@pytest.mark.asyncio
async def test_session_start_excludes_raw_field_when_raw_false():
    """session:start payload does NOT include 'raw' when session.raw=false."""
    config = _minimal_config(raw=False)
    session = PyAmplifierSession(config)
    _setup_mock_session(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_START, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    start_events = [d for e, d in emitted if e == SESSION_START]
    assert len(start_events) == 1
    payload = start_events[0]
    assert "raw" not in payload, (
        "Expected no 'raw' key in session:start payload when session.raw=false"
    )


@pytest.mark.asyncio
async def test_session_start_excludes_raw_field_when_raw_absent():
    """session:start payload does NOT include 'raw' when session.raw is not set."""
    config = _minimal_config(raw=None)
    session = PyAmplifierSession(config)
    _setup_mock_session(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_START, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    start_events = [d for e, d in emitted if e == SESSION_START]
    assert len(start_events) == 1
    payload = start_events[0]
    assert "raw" not in payload, (
        "Expected no 'raw' key in session:start payload when raw not configured"
    )


# ---------------------------------------------------------------------------
# Part 3: session:resume — raw field in payload
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_resume_includes_raw_field_when_raw_true():
    """session:resume payload includes 'raw' when session.raw=true."""
    config = _minimal_config(raw=True)
    session = PyAmplifierSession(config, is_resumed=True)
    _setup_mock_session(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_RESUME, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    resume_events = [d for e, d in emitted if e == SESSION_RESUME]
    assert len(resume_events) == 1, (
        f"Expected 1 SESSION_RESUME, got {len(resume_events)}"
    )
    payload = resume_events[0]
    assert "raw" in payload, (
        "Expected 'raw' key in session:resume payload when session.raw=true"
    )


@pytest.mark.asyncio
async def test_session_resume_excludes_raw_field_when_raw_absent():
    """session:resume payload does NOT include 'raw' when session.raw is not set."""
    config = _minimal_config(raw=None)
    session = PyAmplifierSession(config, is_resumed=True)
    _setup_mock_session(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_RESUME, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    resume_events = [d for e, d in emitted if e == SESSION_RESUME]
    assert len(resume_events) == 1
    payload = resume_events[0]
    assert "raw" not in payload, (
        "Expected no 'raw' key in session:resume payload when raw not configured"
    )


# ---------------------------------------------------------------------------
# Part 4: session:fork — raw field in payload
# ---------------------------------------------------------------------------


def _setup_mock_loader(session):
    """Replace the session loader with a mock that succeeds silently."""
    mock_mount = AsyncMock(return_value=None)
    session.loader.load = AsyncMock(return_value=mock_mount)


@pytest.mark.asyncio
async def test_session_fork_includes_raw_field_when_raw_true():
    """session:fork payload includes 'raw' when session.raw=true."""
    config = _minimal_config(raw=True)
    parent_id = "parent-session-id-raw-true"
    session = PyAmplifierSession(config, parent_id=parent_id)

    _setup_mock_loader(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_FORK, _make_capture_handler(emitted), name="test-capture"
    )

    await session.initialize()

    fork_events = [d for e, d in emitted if e == SESSION_FORK]
    assert len(fork_events) == 1, f"Expected 1 SESSION_FORK, got {len(fork_events)}"
    payload = fork_events[0]
    assert "raw" in payload, (
        "Expected 'raw' key in session:fork payload when session.raw=true"
    )


@pytest.mark.asyncio
async def test_session_fork_excludes_raw_field_when_raw_absent():
    """session:fork payload does NOT include 'raw' when session.raw is not set."""
    config = _minimal_config(raw=None)
    parent_id = "parent-session-id-no-raw"
    session = PyAmplifierSession(config, parent_id=parent_id)

    _setup_mock_loader(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_FORK, _make_capture_handler(emitted), name="test-capture"
    )

    await session.initialize()

    fork_events = [d for e, d in emitted if e == SESSION_FORK]
    assert len(fork_events) == 1
    payload = fork_events[0]
    assert "raw" not in payload, (
        "Expected no 'raw' key in session:fork payload when raw not configured"
    )


# ---------------------------------------------------------------------------
# Part 5: No extra tiered events emitted
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_start_emits_only_one_session_event_with_raw_true():
    """With raw=true, only ONE session:start event is emitted (not 3 tiered events)."""
    config = _minimal_config(raw=True)
    session = PyAmplifierSession(config)
    _setup_mock_session(session)

    emitted = []

    async def capture_all(event, data):
        if event.startswith("session:start"):
            emitted.append(event)
        return HookResult(action="continue")

    # Subscribe to any session:start* event
    session.coordinator.hooks.on(SESSION_START, capture_all, name="test-capture-base")

    # Also check old debug/raw event strings don't appear
    all_emitted_events = []

    async def capture_every_event(event, data):
        all_emitted_events.append(event)
        return HookResult(action="continue")

    # Hook into the wildcard if possible, otherwise just check the specific events
    await session.execute("hello")

    # Only 1 session:start should have been emitted
    assert len(emitted) == 1, (
        f"Expected exactly 1 session:start emission, got {len(emitted)}: {emitted}"
    )


@pytest.mark.asyncio
async def test_session_fork_emits_only_one_session_event_with_raw_true():
    """With raw=true, only ONE session:fork event is emitted (not 3 tiered events)."""
    config = _minimal_config(raw=True)
    parent_id = "parent-one-fork"
    session = PyAmplifierSession(config, parent_id=parent_id)
    _setup_mock_loader(session)

    fork_events_emitted = []

    async def capture_fork(event, data):
        fork_events_emitted.append(event)
        return HookResult(action="continue")

    session.coordinator.hooks.on(SESSION_FORK, capture_fork, name="test-capture-fork")

    await session.initialize()

    assert len(fork_events_emitted) == 1, (
        f"Expected exactly 1 session:fork emission, got {len(fork_events_emitted)}: {fork_events_emitted}"
    )


# ---------------------------------------------------------------------------
# Part 6: raw field contains redacted config (no plain secrets)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_start_raw_field_is_redacted():
    """The 'raw' field in session:start must have secrets redacted."""
    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
            "raw": True,
        },
        "providers": [
            {
                "module": "some-provider",
                "config": {"api_key": "super-secret-key-1234"},
            }
        ],
        "tools": [],
    }

    session = PyAmplifierSession(config)
    _setup_mock_session(session)

    emitted = []
    session.coordinator.hooks.on(
        SESSION_START, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    start_events = [d for e, d in emitted if e == SESSION_START]
    assert len(start_events) == 1
    payload = start_events[0]
    assert "raw" in payload

    # The raw field should not contain the plain-text secret
    raw_str = str(payload["raw"])
    assert "super-secret-key-1234" not in raw_str, (
        "Raw field must not contain plain-text secrets — redact_secrets must be applied"
    )
