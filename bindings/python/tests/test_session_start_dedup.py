"""Tests for Bug 2 — duplicate session:start when session.raw=True.

Root cause:
  When session.raw=True, the Rust kernel emits session:start ONCE (the
  base event), then immediately calls the Python helper
  emit_raw_field_if_configured() which emits session:start a SECOND TIME
  with the raw config payload. Two session:start events per session.

Fix:
  emit_raw_field_if_configured() must NOT re-emit the base session event.
  Instead it emits a dedicated "session:config" event that carries the raw
  config. Consumers that need the raw mount plan should subscribe to
  "session:config" for the raw payload and "session:start" for the base event.

Implementation note:
  emit_raw_field_if_configured() is a Python helper called from the Rust
  bridge's execute() path. Changing it is a pure-Python fix with no Rust
  binary changes required.
"""

from __future__ import annotations

import pytest
from unittest.mock import AsyncMock, patch

from amplifier_core._engine import RustSession


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


async def _make_raw_session(session_id: str = "test-raw-session") -> RustSession:
    """Create a fully-initialized RustSession with session.raw=True."""
    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
            "raw": True,
        },
        "providers": [],
        "hooks": [],
        "tools": [],
    }
    session = RustSession(config=config, session_id=session_id)
    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()
    return session


def _mount_stub_orchestrator(session: RustSession) -> None:
    """Mount a stub orchestrator so execute() can complete."""
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="ok")
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = AsyncMock()
    session.coordinator.mount_points["providers"] = {"mock": AsyncMock()}


# ---------------------------------------------------------------------------
# Bug 2 tests — session:start deduplication
# ---------------------------------------------------------------------------


class TestSessionStartNotDuplicated:
    """session:start must be emitted exactly once per execute() call.

    With the bug: when session.raw=True, two session:start events are emitted
    per execute() call — one from the Rust kernel and one from
    emit_raw_field_if_configured().

    After the fix: exactly one session:start (from the Rust kernel) and
    a separate session:config event carrying the raw config.
    """

    @pytest.mark.asyncio
    async def test_session_start_emitted_exactly_once_for_raw_mode(self):
        """session:start must be emitted exactly once when session.raw=True.

        With the bug: two session:start events per execute().
        After the fix: one session:start.
        """
        session = await _make_raw_session()

        session_starts: list[dict] = []

        async def _count_start(event: str, data: dict):
            session_starts.append(data)
            return None

        session.coordinator.hooks.register(
            "session:start", _count_start, name="test-start-counter"
        )
        _mount_stub_orchestrator(session)

        await session.execute("Hi")

        assert len(session_starts) == 1, (
            f"Expected exactly 1 session:start event, got {len(session_starts)}. "
            f"Duplicate session:start is the Bug 2 symptom — "
            f"emit_raw_field_if_configured() must not re-emit session:start."
        )

    @pytest.mark.asyncio
    async def test_raw_config_captured_in_session_config_event(self):
        """When session.raw=True, the raw config must appear in session:config.

        After the fix, raw config is NOT in a second session:start.
        Instead it is emitted as a dedicated session:config event.
        """
        session = await _make_raw_session()

        config_events: list[dict] = []

        async def _collect_config(event: str, data: dict):
            config_events.append(data)
            return None

        session.coordinator.hooks.register(
            "session:config", _collect_config, name="test-config-collector"
        )
        _mount_stub_orchestrator(session)

        await session.execute("Hi")

        assert len(config_events) == 1, (
            f"Expected exactly 1 session:config event (raw config dump), "
            f"got {len(config_events)}."
        )
        config_data = config_events[0]
        assert "raw" in config_data, (
            f"session:config event must contain 'raw' field with redacted config. "
            f"Got keys: {list(config_data.keys())}"
        )

    @pytest.mark.asyncio
    async def test_session_start_still_emitted_when_raw_true(self):
        """session:start must still be emitted (guard against over-correction)."""
        session = await _make_raw_session()

        started: list[dict] = []

        async def _track_start(event: str, data: dict):
            started.append(data)
            return None

        session.coordinator.hooks.register(
            "session:start", _track_start, name="test-start-guard"
        )
        _mount_stub_orchestrator(session)

        await session.execute("Hi")

        assert len(started) >= 1, (
            "session:start was NOT emitted — the fix must preserve the base emit."
        )

    @pytest.mark.asyncio
    async def test_non_raw_session_emits_single_session_start(self):
        """Non-raw sessions must also emit session:start exactly once."""
        config = {
            "session": {
                "orchestrator": "loop-basic",
                "context": "context-simple",
                # raw is NOT set
            },
            "providers": [],
            "hooks": [],
            "tools": [],
        }
        session = RustSession(config=config)
        mock_init = AsyncMock()
        with patch("amplifier_core._session_init.initialize_session", mock_init):
            await session.initialize()

        starts: list[dict] = []

        async def _track(event: str, data: dict):
            starts.append(data)
            return None

        session.coordinator.hooks.register(
            "session:start", _track, name="test-non-raw-start"
        )
        _mount_stub_orchestrator(session)

        await session.execute("Hi")

        assert len(starts) == 1, (
            f"Non-raw session must emit session:start exactly once, "
            f"got {len(starts)}."
        )


# ---------------------------------------------------------------------------
# Multi-turn regression tests — session:start must fire ONCE per session,
# not once per execute() call (the Rust-port regression fixed in v1.6.0).
#
# Before the fix: every execute() call re-fired session:start, so 3 turns
# yielded 3 events.  After the fix: exactly 1 event across any number of
# execute() calls.
# ---------------------------------------------------------------------------


class TestSessionStartOncePerSession:
    """session:start must be emitted once per SESSION, not once per execute() call.

    This tests the Rust-port regression (introduced 2026-02-14, d2826b4):
    the Rust kernel placed session:start emission inside execute(), causing
    every interactive turn to re-fire the event.  Hook handlers that
    registered for session:start expecting once-per-session semantics
    (e.g. hooks-routing, hook-context-intelligence) were re-running expensive
    work on every turn, contributing 7-13s wall-clock cost per prompt.

    Fix: session:start is guarded by an atomic flag so it fires at most once
    per session lifetime regardless of how many times execute() is called.
    """

    @pytest.mark.asyncio
    async def test_session_start_emitted_once_across_multiple_execute_calls(self):
        """session:start must fire exactly once even when execute() is called 3 times."""
        session = await _make_raw_session("multi-turn-test-1")

        starts: list[dict] = []

        async def _count_start(event: str, data: dict):
            starts.append(data)
            return None

        session.coordinator.hooks.register(
            "session:start", _count_start, name="test-multi-turn-counter"
        )
        _mount_stub_orchestrator(session)

        await session.execute("turn one")
        await session.execute("turn two")
        await session.execute("turn three")

        assert len(starts) == 1, (
            f"session:start must fire exactly once per session regardless of "
            f"how many execute() calls are made. Got {len(starts)} events "
            f"across 3 execute() calls. "
            f"This was the Rust-port regression: session:start was emitted "
            f"inside execute() instead of once at session initialization."
        )

    @pytest.mark.asyncio
    async def test_session_resume_emitted_once_across_multiple_execute_calls(self):
        """session:resume must fire exactly once for resumed sessions with multiple turns."""
        config = {
            "session": {
                "orchestrator": "loop-basic",
                "context": "context-simple",
            },
            "providers": [],
            "hooks": [],
            "tools": [],
        }
        session = RustSession(
            config=config,
            session_id="resumed-multi-turn",
            is_resumed=True,
        )
        mock_init = AsyncMock()
        with patch("amplifier_core._session_init.initialize_session", mock_init):
            await session.initialize()

        resumes: list[dict] = []

        async def _count_resume(event: str, data: dict):
            resumes.append(data)
            return None

        session.coordinator.hooks.register(
            "session:resume", _count_resume, name="test-resume-counter"
        )
        _mount_stub_orchestrator(session)

        await session.execute("turn one")
        await session.execute("turn two")
        await session.execute("turn three")

        assert len(resumes) == 1, (
            f"session:resume must fire exactly once per session regardless of "
            f"how many execute() calls are made. Got {len(resumes)} events "
            f"across 3 execute() calls."
        )

    @pytest.mark.asyncio
    async def test_session_start_not_emitted_on_second_or_third_turn(self):
        """Verify session:start is absent on turns 2 and 3 — only present on turn 1."""
        session = await _make_raw_session("multi-turn-test-2")

        per_turn_counts: list[int] = []
        cumulative: list[dict] = []

        async def _count_start(event: str, data: dict):
            cumulative.append(data)
            return None

        session.coordinator.hooks.register(
            "session:start", _count_start, name="test-per-turn-counter"
        )
        _mount_stub_orchestrator(session)

        await session.execute("turn one")
        per_turn_counts.append(len(cumulative))  # After turn 1: should be 1

        await session.execute("turn two")
        per_turn_counts.append(len(cumulative))  # After turn 2: should still be 1

        await session.execute("turn three")
        per_turn_counts.append(len(cumulative))  # After turn 3: should still be 1

        assert per_turn_counts == [1, 1, 1], (
            f"session:start count after each turn should be [1, 1, 1]. "
            f"Got {per_turn_counts}. "
            f"Any count > 1 means session:start re-fired on a subsequent turn."
        )
