"""
Tests for session metadata passthrough on session:start, session:fork, session:resume.

CP-SM: Kernel reads config.session.metadata and includes it as optional 'metadata'
key in event payloads. Pure passthrough - no interpretation or validation.
"""

from unittest.mock import AsyncMock, Mock

import pytest
from amplifier_core.events import SESSION_FORK, SESSION_RESUME, SESSION_START
from amplifier_core.models import HookResult
from amplifier_core.session import AmplifierSession as PyAmplifierSession


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _minimal_config_with_metadata(metadata):
    """Config that includes session.metadata."""
    return {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
            "metadata": metadata,
        },
        "providers": [],
        "tools": [],
    }


def _minimal_config_no_metadata():
    """Config without session.metadata."""
    return {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        },
        "providers": [],
        "tools": [],
    }


def _setup_mock_loader(session):
    """Replace the session loader with a mock that succeeds silently."""
    mock_mount = AsyncMock(return_value=None)
    session.loader.load = AsyncMock(return_value=mock_mount)


def _make_capture_handler(events_list):
    """Return an async hook handler that appends (event, data) to events_list."""

    async def handler(event, data):
        events_list.append((event, dict(data)))
        return HookResult(action="continue")

    return handler


# ---------------------------------------------------------------------------
# session:start metadata tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_start_includes_metadata_when_configured():
    """session:start payload includes 'metadata' when config.session.metadata is set."""
    metadata = {"agent_name": "test-agent", "run_id": "abc123"}
    config = _minimal_config_with_metadata(metadata)

    session = PyAmplifierSession(config)
    session._initialized = True

    # Mount minimal mocks
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="ok")
    mock_context = Mock()
    mock_context.add_message = AsyncMock()
    mock_context.get_messages = AsyncMock(return_value=[])
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = mock_context
    session.coordinator.mount_points["providers"] = {"mock": Mock()}

    emitted = []
    session.coordinator.hooks.on(
        SESSION_START, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    start_events = [d for e, d in emitted if e == SESSION_START]
    assert len(start_events) == 1, f"Expected 1 SESSION_START, got {len(start_events)}"
    payload = start_events[0]
    assert "metadata" in payload, "Expected 'metadata' key in session:start payload"
    assert payload["metadata"] == metadata


@pytest.mark.asyncio
async def test_session_start_excludes_metadata_when_not_configured():
    """session:start payload does NOT include 'metadata' when config.session.metadata is absent."""
    config = _minimal_config_no_metadata()

    session = PyAmplifierSession(config)
    session._initialized = True

    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="ok")
    mock_context = Mock()
    mock_context.add_message = AsyncMock()
    mock_context.get_messages = AsyncMock(return_value=[])
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = mock_context
    session.coordinator.mount_points["providers"] = {"mock": Mock()}

    emitted = []
    session.coordinator.hooks.on(
        SESSION_START, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    start_events = [d for e, d in emitted if e == SESSION_START]
    assert len(start_events) == 1
    payload = start_events[0]
    assert "metadata" not in payload, (
        "Expected no 'metadata' key in session:start payload when not configured"
    )


# ---------------------------------------------------------------------------
# session:resume metadata tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_resume_includes_metadata_when_configured():
    """session:resume payload includes 'metadata' when config.session.metadata is set."""
    metadata = {"agent_name": "resumed-agent"}
    config = _minimal_config_with_metadata(metadata)

    session = PyAmplifierSession(config, is_resumed=True)
    session._initialized = True

    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="ok")
    mock_context = Mock()
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = mock_context
    session.coordinator.mount_points["providers"] = {"mock": Mock()}

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
    assert "metadata" in payload, "Expected 'metadata' key in session:resume payload"
    assert payload["metadata"] == metadata


@pytest.mark.asyncio
async def test_session_resume_excludes_metadata_when_not_configured():
    """session:resume payload does NOT include 'metadata' when not set in config."""
    config = _minimal_config_no_metadata()

    session = PyAmplifierSession(config, is_resumed=True)
    session._initialized = True

    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="ok")
    mock_context = Mock()
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = mock_context
    session.coordinator.mount_points["providers"] = {"mock": Mock()}

    emitted = []
    session.coordinator.hooks.on(
        SESSION_RESUME, _make_capture_handler(emitted), name="test-capture"
    )

    await session.execute("hello")

    resume_events = [d for e, d in emitted if e == SESSION_RESUME]
    assert len(resume_events) == 1
    payload = resume_events[0]
    assert "metadata" not in payload, (
        "Expected no 'metadata' key in session:resume payload when not configured"
    )


# ---------------------------------------------------------------------------
# session:fork metadata tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_fork_includes_metadata_when_configured():
    """session:fork payload includes 'metadata' when config.session.metadata is set."""
    metadata = {"agent_name": "child-agent", "depth": 1}
    config = _minimal_config_with_metadata(metadata)

    parent_id = "parent-session-id-123"
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
    assert "metadata" in payload, "Expected 'metadata' key in session:fork payload"
    assert payload["metadata"] == metadata


@pytest.mark.asyncio
async def test_session_fork_excludes_metadata_when_not_configured():
    """session:fork payload does NOT include 'metadata' when config.session.metadata is absent."""
    config = _minimal_config_no_metadata()

    parent_id = "parent-session-id-456"
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
    assert "metadata" not in payload, (
        "Expected no 'metadata' key in session:fork payload when not configured"
    )


@pytest.mark.asyncio
async def test_session_fork_still_has_parent_and_session_id():
    """session:fork payload still contains parent and session_id regardless of metadata."""
    metadata = {"tag": "some-tag"}
    config = _minimal_config_with_metadata(metadata)

    parent_id = "parent-xyz"
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
    assert payload["parent"] == parent_id
    assert "session_id" in payload
