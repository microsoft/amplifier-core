"""CP-SM metadata passthrough, exercised against the class that actually runs.

`tests/test_session_metadata.py` covers the same contract against the
*pure-Python* `amplifier_core.session.AmplifierSession`.  Production code does
``from amplifier_core import AmplifierSession``, which is ``RustSession``
(`python/amplifier_core/__init__.py`) -- so those tests were green on a code
path no runtime consumer executes, and the Rust emit shipped without the
metadata merge.

This module is the parallel suite against `RustSession`.  Keep the two in sync:
a payload contract that is only asserted on one side of the switchover is a
contract that can regress silently.

CP-SM: Kernel reads config.session.metadata and includes it as optional
'metadata' key in event payloads. Pure passthrough - no interpretation or
validation.
"""

from __future__ import annotations

from unittest.mock import AsyncMock, Mock, patch

import pytest
from amplifier_core._engine import RustSession
from amplifier_core.events import SESSION_FORK, SESSION_RESUME, SESSION_START

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _config(metadata=None):
    """Minimal mount plan, optionally carrying session.metadata."""
    session: dict = {
        "orchestrator": "loop-basic",
        "context": "context-simple",
    }
    if metadata is not None:
        session["metadata"] = metadata
    return {"session": session, "providers": [], "hooks": [], "tools": []}


async def _make_session(config: dict, **kwargs) -> RustSession:
    """Create a RustSession whose module loading is stubbed out."""
    session = RustSession(config=config, **kwargs)
    with patch("amplifier_core._session_init.initialize_session", AsyncMock()):
        await session.initialize()
    return session


def _mount_stubs(session: RustSession) -> None:
    """Mount the minimum required for execute() to reach the orchestrator."""
    orchestrator = AsyncMock()
    orchestrator.execute = AsyncMock(return_value="ok")
    session.coordinator.mount_points["orchestrator"] = orchestrator
    session.coordinator.mount_points["context"] = AsyncMock()
    session.coordinator.mount_points["providers"] = {"mock": AsyncMock()}


def _capture(session: RustSession, event: str) -> list[dict]:
    """Register a handler that records payloads for `event`."""
    captured: list[dict] = []

    async def _handler(_event: str, data: dict):
        captured.append(dict(data))
        return None

    session.coordinator.hooks.register(event, _handler, name=f"capture-{event}")
    return captured


# ---------------------------------------------------------------------------
# session:start
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_start_includes_metadata_when_configured():
    """session:start carries config.session.metadata verbatim."""
    metadata = {"agent_name": "test-agent", "run_id": "abc123"}
    session = await _make_session(_config(metadata))
    starts = _capture(session, SESSION_START)
    _mount_stubs(session)

    await session.execute("hello")

    assert len(starts) == 1, f"Expected 1 session:start, got {len(starts)}"
    assert "metadata" in starts[0], (
        "Expected 'metadata' key in session:start payload. "
        f"Got keys: {sorted(starts[0])}"
    )
    assert starts[0]["metadata"] == metadata


@pytest.mark.asyncio
async def test_session_start_passes_nested_metadata_through_untouched():
    """Passthrough is verbatim -- nested objects are not flattened or rewritten."""
    metadata = {
        "invocation": {
            "schema": 1,
            "mode": "single",
            "stdin_isatty": False,
            "launched_by_session_id": None,
        }
    }
    session = await _make_session(_config(metadata))
    starts = _capture(session, SESSION_START)
    _mount_stubs(session)

    await session.execute("hello")

    assert starts[0]["metadata"] == metadata


@pytest.mark.asyncio
async def test_session_start_excludes_metadata_when_not_configured():
    """No metadata configured => payload is exactly what it was before CP-SM."""
    session = await _make_session(_config())
    starts = _capture(session, SESSION_START)
    _mount_stubs(session)

    await session.execute("hello")

    assert len(starts) == 1
    assert "metadata" not in starts[0], (
        "Expected no 'metadata' key when unconfigured. "
        f"Got keys: {sorted(starts[0])}"
    )


@pytest.mark.asyncio
async def test_session_start_excludes_empty_metadata():
    """An empty dict is falsy for the Python kernel -- the Rust kernel must agree."""
    session = await _make_session(_config({}))
    starts = _capture(session, SESSION_START)
    _mount_stubs(session)

    await session.execute("hello")

    assert "metadata" not in starts[0], (
        "Empty metadata must behave exactly like absent metadata "
        "(matches the pure-Python kernel's `if session_metadata:` guard)."
    )


@pytest.mark.asyncio
async def test_metadata_does_not_displace_existing_payload_fields():
    """Additive only: session_id / parent_id keep their meaning and values."""
    parent_id = "parent-session-id-123"
    session = await _make_session(
        _config({"tag": "some-tag"}),
        session_id="child-session-id-456",
        parent_id=parent_id,
    )
    starts = _capture(session, SESSION_START)
    _mount_stubs(session)

    await session.execute("hello")

    payload = starts[0]
    assert payload["session_id"] == "child-session-id-456"
    assert payload["parent_id"] == parent_id
    assert payload["metadata"] == {"tag": "some-tag"}


# ---------------------------------------------------------------------------
# session:resume
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_resume_includes_metadata_when_configured():
    """session:resume carries metadata too -- same emit site, same contract."""
    metadata = {"agent_name": "resumed-agent"}
    session = await _make_session(
        _config(metadata), session_id="resumed-id", is_resumed=True
    )
    resumes = _capture(session, SESSION_RESUME)
    _mount_stubs(session)

    await session.execute("hello")

    assert len(resumes) == 1
    assert resumes[0]["metadata"] == metadata


@pytest.mark.asyncio
async def test_session_resume_excludes_metadata_when_not_configured():
    session = await _make_session(
        _config(), session_id="resumed-id-2", is_resumed=True
    )
    resumes = _capture(session, SESSION_RESUME)
    _mount_stubs(session)

    await session.execute("hello")

    assert len(resumes) == 1
    assert "metadata" not in resumes[0]


# ---------------------------------------------------------------------------
# session:fork
#
# fork is emitted by the shared Python helper (`_session_init.py`), which both
# session classes delegate to -- so it already honored CP-SM.  Pinned here so
# the three emit paths stay in agreement.
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_fork_includes_metadata_when_configured():
    metadata = {"agent_name": "child-agent", "depth": 1}
    parent_id = "parent-session-id-789"
    session = RustSession(config=_config(metadata), parent_id=parent_id)

    loader = Mock()
    loader.load = AsyncMock(return_value=AsyncMock(return_value=None))
    loader.get_on_session_ready_queue = Mock(return_value=[])
    loader.clear_on_session_ready_queue = Mock(return_value=None)
    session.coordinator.loader = loader

    forks = _capture(session, SESSION_FORK)

    await session.initialize()

    assert len(forks) == 1, f"Expected 1 session:fork, got {len(forks)}"
    assert forks[0]["metadata"] == metadata
    assert forks[0]["parent"] == parent_id
