"""Tests for Rust-driven session lifecycle.

Task 8 - initialize() in Rust:
1. Sets the initialized flag to True after successful init
2. Is idempotent (second call is a no-op)
3. Delegates module loading to the Python helper
4. Propagates errors from module loading (initialized stays False)

Task 9 - execute() in Rust:
5. execute() requires initialization (raises error if not initialized)
6. execute() calls the orchestrator via the Python helper
7. execute() returns the orchestrator's result string

Task 11 - Full session lifecycle integration:
8. Full lifecycle: create → initialize → execute → cleanup through Rust
"""

import pytest
from unittest.mock import AsyncMock, patch

from amplifier_core._engine import RustSession


@pytest.mark.asyncio
async def test_initialize_sets_initialized_flag():
    """After successful initialize(), session.initialized should be True."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert session.initialized is False

    # Mock the Python init helper so we don't need real modules installed
    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()

    assert session.initialized is True


@pytest.mark.asyncio
async def test_initialize_is_idempotent():
    """Calling initialize() twice only runs module loading once."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)

    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()
        await session.initialize()  # Second call should be a no-op

    mock_init.assert_called_once()


@pytest.mark.asyncio
async def test_initialize_delegates_to_python_helper():
    """Rust initialize() passes config, coordinator, session_id, parent_id to Python."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(
        config=config, session_id="test-rust-init", parent_id="parent-42"
    )

    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()

    mock_init.assert_called_once()
    args = mock_init.call_args[0]
    # args[0] = config dict, args[1] = coordinator, args[2] = session_id, args[3] = parent_id
    assert args[2] == "test-rust-init"
    assert args[3] == "parent-42"


@pytest.mark.asyncio
async def test_initialize_error_keeps_initialized_false():
    """If module loading fails, initialized stays False."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)

    mock_init = AsyncMock(side_effect=RuntimeError("Module not found"))
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        with pytest.raises(Exception):
            await session.initialize()

    assert session.initialized is False


# ---------------------------------------------------------------------------
# Task 9: execute() in Rust
# ---------------------------------------------------------------------------


async def _make_initialized_session(config=None, **kwargs):
    """Helper: create a RustSession and initialize it with mocked loader."""
    if config is None:
        config = {
            "session": {"orchestrator": "loop-basic", "context": "context-simple"}
        }
    session = RustSession(config=config, **kwargs)
    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()
    return session


@pytest.mark.asyncio
async def test_execute_requires_initialization():
    """Calling execute() on an un-initialized session must raise an error."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert session.initialized is False

    with pytest.raises(Exception, match="[Nn]ot initialized"):
        await session.execute("hello")


@pytest.mark.asyncio
async def test_execute_calls_orchestrator():
    """After initialize(), execute() should invoke the orchestrator's execute()."""
    session = await _make_initialized_session()

    # Plant a mock orchestrator that returns a string
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="ok")
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator

    # Also need context and providers mounted (execute checks for them)
    session.coordinator.mount_points["context"] = AsyncMock()
    session.coordinator.mount_points["providers"] = {"mock": AsyncMock()}

    await session.execute("hello")

    # The orchestrator's execute() should have been called
    mock_orchestrator.execute.assert_called_once()


@pytest.mark.asyncio
async def test_execute_returns_result():
    """execute() must return the string produced by the orchestrator."""
    session = await _make_initialized_session()

    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="Hello!")
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = AsyncMock()
    session.coordinator.mount_points["providers"] = {"mock": AsyncMock()}

    result = await session.execute("hi")

    assert result == "Hello!"


# ---------------------------------------------------------------------------
# Task 10: cleanup() in Rust
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_cleanup_calls_cleanup_functions():
    """cleanup() should call all registered cleanup functions."""
    session = await _make_initialized_session()

    # Register a cleanup function on the coordinator
    called = []

    def cleanup_fn():
        called.append("cleaned")

    session.coordinator.register_cleanup(cleanup_fn)

    await session.cleanup()

    assert called == ["cleaned"]


@pytest.mark.asyncio
async def test_cleanup_handles_errors_gracefully():
    """cleanup() should not crash when a cleanup function raises."""
    session = await _make_initialized_session()

    # Register a good cleanup function first, then a bad one.
    # Cleanup runs in reverse order: bad runs first, then good should still run.
    called = []

    def good_cleanup():
        called.append("good")

    def bad_cleanup():
        raise RuntimeError("cleanup failed!")

    session.coordinator.register_cleanup(good_cleanup)
    session.coordinator.register_cleanup(bad_cleanup)

    # Should not raise — errors are logged but don't crash
    await session.cleanup()

    # The good cleanup should still have been called despite bad_cleanup raising
    assert "good" in called


@pytest.mark.asyncio
async def test_cleanup_emits_session_end_event():
    """cleanup() should emit a session:end event."""
    session = await _make_initialized_session()

    # Track emitted events via the hooks
    emitted_events = []

    async def track_event(event, data):
        emitted_events.append(event)
        return None

    session.coordinator.hooks.register("session:end", track_event, name="test-tracker")

    await session.cleanup()

    assert "session:end" in emitted_events


@pytest.mark.asyncio
async def test_cleanup_resets_initialized_flag():
    """After cleanup(), session.initialized should be False."""
    session = await _make_initialized_session()
    assert session.initialized is True

    await session.cleanup()

    assert session.initialized is False


# ---------------------------------------------------------------------------
# Task 11: Full session lifecycle integration test
# ---------------------------------------------------------------------------


class MockOrchestrator:
    """Mock orchestrator that records calls and returns a predictable response."""

    def __init__(self):
        self.called_with = None
        self.call_count = 0

    async def execute(
        self,
        prompt,
        context=None,
        providers=None,
        tools=None,
        hooks=None,
        coordinator=None,
    ):
        self.called_with = prompt
        self.call_count += 1
        return f"Response to: {prompt}"


@pytest.mark.asyncio
async def test_full_lifecycle_through_rust():
    """Full lifecycle: create → initialize → execute → cleanup, all driven by Rust.

    Proves:
    - RustSession drives the lifecycle (not Python AmplifierSession)
    - initialize() sets the initialized flag
    - execute() calls the Python orchestrator via PyO3 and returns its result
    - Events are emitted with timestamp fields (session:start, session:end)
    - cleanup() calls cleanup functions, emits session:end, resets initialized
    """
    # --- Setup ---
    config = {"session": {"orchestrator": "mock", "context": "mock"}}
    session = RustSession(config=config, session_id="lifecycle-test-001")

    # Track ALL emitted events and their data
    captured_events = []

    async def capture_event(event, data):
        captured_events.append({"event": event, "data": dict(data)})
        return None  # Python HookRegistry tolerates None returns

    # Track cleanup function calls
    cleanup_called = []

    def on_cleanup():
        cleanup_called.append("cleaned")

    # --- Phase 1: Create & verify initial state ---
    assert session.initialized is False

    # --- Phase 2: Initialize ---
    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()

    assert session.initialized is True

    # --- Phase 3: Mount mock modules & register hooks ---
    mock_orch = MockOrchestrator()
    session.coordinator.mount_points["orchestrator"] = mock_orch
    session.coordinator.mount_points["context"] = AsyncMock()
    session.coordinator.mount_points["providers"] = {"mock-provider": AsyncMock()}

    # Register hook handlers AFTER initialize so hooks object exists
    session.coordinator.hooks.register(
        "session:start", capture_event, name="test-start-tracker"
    )
    session.coordinator.hooks.register(
        "session:end", capture_event, name="test-end-tracker"
    )

    # Register a cleanup function
    session.coordinator.register_cleanup(on_cleanup)

    # --- Phase 4: Execute ---
    was_initialized_before_execute = session.initialized
    result = await session.execute("Hello!")

    # --- Phase 5: Cleanup ---
    was_initialized_before_cleanup = session.initialized
    await session.cleanup()

    # --- Assertions ---

    # 1. The result matches what the mock orchestrator returns
    assert result == "Response to: Hello!"

    # 2. The mock orchestrator's execute() was called with the right prompt
    assert mock_orch.called_with == "Hello!"
    assert mock_orch.call_count == 1

    # 3. Session was initialized before execute and cleanup
    assert was_initialized_before_execute is True
    assert was_initialized_before_cleanup is True

    # 4. After cleanup, initialized is False
    assert session.initialized is False

    # 5. Cleanup function was called
    assert cleanup_called == ["cleaned"]

    # 6. Events were emitted — check for session:start and session:end
    event_names = [e["event"] for e in captured_events]
    assert "session:start" in event_names, (
        f"Expected session:start event, got: {event_names}"
    )
    assert "session:end" in event_names, (
        f"Expected session:end event, got: {event_names}"
    )

    # 7. All emitted events have timestamp fields with valid ISO format strings
    #    (timestamps are stamped by HookRegistry.emit as infrastructure-owned fields)
    from datetime import datetime

    for entry in captured_events:
        assert "timestamp" in entry["data"], (
            f"Event '{entry['event']}' missing timestamp field. "
            f"Data keys: {list(entry['data'].keys())}"
        )
        # Verify the timestamp is a parseable ISO format string
        ts = entry["data"]["timestamp"]
        assert isinstance(ts, str), f"Timestamp should be a string, got {type(ts)}"
        datetime.fromisoformat(ts)  # Raises ValueError if not valid ISO format

    # 8. The session:start event contains our session_id
    start_events = [e for e in captured_events if e["event"] == "session:start"]
    assert len(start_events) == 1
    assert start_events[0]["data"]["session_id"] == "lifecycle-test-001"

    # 9. The session:end event contains our session_id
    end_events = [e for e in captured_events if e["event"] == "session:end"]
    assert len(end_events) == 1
    assert end_events[0]["data"]["session_id"] == "lifecycle-test-001"


# ---------------------------------------------------------------------------
# Task 12: Remove hooks property override — coordinator.hooks is RustHookRegistry
# ---------------------------------------------------------------------------


def test_coordinator_hooks_returns_rust_registry():
    """After removing the override, coordinator.hooks should be the Rust RustHookRegistry,
    not the Python HookRegistry."""
    from amplifier_core import AmplifierSession
    from amplifier_core._engine import RustHookRegistry

    session = AmplifierSession({"session": {"orchestrator": "test", "context": "test"}})
    hooks = session.coordinator.hooks
    assert isinstance(hooks, RustHookRegistry), (
        f"Expected RustHookRegistry, got {type(hooks)}"
    )


# ---------------------------------------------------------------------------
# Cleanup defense-in-depth: skip None and non-callable items in _cleanup_fns
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_cleanup_skips_non_callable_items():
    """PySession.cleanup() must silently skip non-callable items in
    _cleanup_fns — no 'Error during cleanup' log messages."""
    import logging
    import io

    session = await _make_initialized_session()

    # Register a legitimate cleanup via the proper API
    called = []

    def good_cleanup():
        called.append("good")

    session.coordinator.register_cleanup(good_cleanup)

    # Directly append non-callable items to the list — simulates what
    # happens when external code bypasses register_cleanup()
    fns = session.coordinator._cleanup_fns
    fns.append(None)
    fns.append({"name": "not-callable"})
    fns.append(42)

    # Capture log output from the session logger
    log_stream = io.StringIO()
    handler = logging.StreamHandler(log_stream)
    handler.setLevel(logging.DEBUG)
    logger = logging.getLogger("amplifier_core.session")
    logger.addHandler(handler)
    try:
        await session.cleanup()
    finally:
        logger.removeHandler(handler)

    # The good cleanup function must still have been called
    assert "good" in called, "Good cleanup function should have been called"

    # No "Error during cleanup" messages should appear for non-callable items
    log_output = log_stream.getvalue()
    assert "Error during cleanup" not in log_output, (
        f"Non-callable items should be silently skipped, but got: {log_output}"
    )


@pytest.mark.asyncio
async def test_coordinator_cleanup_skips_non_callable_items():
    """PyCoordinator.cleanup() must silently skip non-callable items in
    _cleanup_fns — no 'Error during cleanup' log messages."""
    import logging
    import io

    session = await _make_initialized_session()
    coordinator = session.coordinator

    # Register a legitimate cleanup via the proper API
    called = []

    def good_cleanup():
        called.append("good")

    coordinator.register_cleanup(good_cleanup)

    # Directly append non-callable items to the list
    fns = coordinator._cleanup_fns
    fns.append(None)
    fns.append({"name": "not-callable"})
    fns.append(42)

    # Capture log output from the coordinator logger
    log_stream = io.StringIO()
    handler = logging.StreamHandler(log_stream)
    handler.setLevel(logging.DEBUG)
    logger = logging.getLogger("amplifier_core.coordinator")
    logger.addHandler(handler)
    try:
        await coordinator.cleanup()
    finally:
        logger.removeHandler(handler)

    # The good cleanup function must still have been called
    assert "good" in called, "Good cleanup function should have been called"

    # No "Error during cleanup" messages should appear for non-callable items
    log_output = log_stream.getvalue()
    assert "Error during cleanup" not in log_output, (
        f"Non-callable items should be silently skipped, but got: {log_output}"
    )
