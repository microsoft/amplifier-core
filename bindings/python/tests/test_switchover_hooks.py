"""Tests for expanded RustHookRegistry API matching Python HookRegistry."""

import pytest
from amplifier_core._engine import RustHookRegistry


def test_set_default_fields():
    """set_default_fields accepts keyword arguments and stores them."""
    registry = RustHookRegistry()
    # Python HookRegistry.set_default_fields takes **kwargs
    registry.set_default_fields(session_id="test-123", parent_id=None)
    # If it doesn't raise, the method exists and accepts kwargs


def test_on_is_alias_for_register():
    """on(event, name, handler, priority) is an alias for register()."""
    registry = RustHookRegistry()

    def my_handler(event, data):
        return None

    # Python HookRegistry has: on = register
    registry.on("tool:pre", my_handler, 50, name="test-handler")
    # If it doesn't raise, the method exists and accepts the same args


def test_list_handlers_empty():
    """list_handlers() returns an empty dict when no handlers registered."""
    registry = RustHookRegistry()
    result = registry.list_handlers()
    assert isinstance(result, dict)
    assert len(result) == 0


def test_list_handlers_with_event_filter():
    """list_handlers(event) returns only handlers for that event."""
    registry = RustHookRegistry()
    registry.register("tool:pre", lambda e, d: None, 0, name="my-hook")
    registry.register("tool:post", lambda e, d: None, 0, name="other-hook")

    result = registry.list_handlers("tool:pre")
    assert "tool:pre" in result
    assert "my-hook" in result["tool:pre"]
    assert "tool:post" not in result


@pytest.mark.asyncio
async def test_emit_and_collect_empty():
    """emit_and_collect returns empty list when no handlers registered."""
    registry = RustHookRegistry()
    result = await registry.emit_and_collect("test:event", {"key": "value"})
    assert isinstance(result, list)
    assert len(result) == 0


@pytest.mark.asyncio
async def test_emit_and_collect_with_timeout():
    """emit_and_collect accepts an optional timeout parameter."""
    registry = RustHookRegistry()
    result = await registry.emit_and_collect("test:event", {}, timeout=2.0)
    assert isinstance(result, list)


def test_event_constants_on_class():
    """RustHookRegistry has class-level event name constants matching Python."""
    assert RustHookRegistry.SESSION_START == "session:start"
    assert RustHookRegistry.SESSION_END == "session:end"
    assert RustHookRegistry.PROMPT_SUBMIT == "prompt:submit"
    assert RustHookRegistry.TOOL_PRE == "tool:pre"
    assert RustHookRegistry.TOOL_POST == "tool:post"
    assert RustHookRegistry.CONTEXT_PRE_COMPACT == "context:pre_compact"
    assert RustHookRegistry.ORCHESTRATOR_COMPLETE == "orchestrator:complete"
    assert RustHookRegistry.USER_NOTIFICATION == "user:notification"


# ---------------------------------------------------------------------------
# Task 3: PyHookHandlerBridge async handler tests (into_future fix)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_emit_with_sync_handler():
    """Sync Python handler returns a dict that becomes a HookResult."""
    registry = RustHookRegistry()

    def sync_handler(event, data):
        return {"action": "continue", "data": {"handled": True}}

    registry.register("test:event", sync_handler, 0, name="sync-hook")
    result = await registry.emit("test:event", {"key": "value"})
    # Should get a valid HookResult back
    assert result is not None
    assert result.action == "continue"


@pytest.mark.asyncio
async def test_emit_with_async_handler():
    """Async Python handler (coroutine) is properly awaited via into_future.

    This is the KEY test for Task 3. The old run_coroutine_threadsafe
    implementation DEADLOCKS here because we're already inside an asyncio
    event loop (pytest-asyncio). The new into_future implementation correctly
    converts the Python coroutine to a Rust Future and awaits it outside the GIL.
    """
    import asyncio

    registry = RustHookRegistry()

    async def async_handler(event, data):
        # Simulate async work — this would deadlock with run_coroutine_threadsafe
        await asyncio.sleep(0.01)
        return {"action": "continue", "data": {"async_handled": True, "event": event}}

    registry.register("test:event", async_handler, 0, name="async-hook")
    result = await registry.emit("test:event", {"key": "value"})
    assert result is not None
    assert result.action == "continue"


@pytest.mark.asyncio
async def test_emit_with_async_handler_returning_none():
    """Async handler returning None produces a default continue HookResult."""
    registry = RustHookRegistry()

    async def noop_handler(event, data):
        return None

    registry.register("test:event", noop_handler, 0, name="noop-hook")
    result = await registry.emit("test:event", {})
    assert result is not None
    # Default HookResult should have action "continue"
    assert result.action == "continue"


@pytest.mark.asyncio
async def test_async_handler_uses_callers_event_loop():
    """Async handler coroutine runs on the caller's event loop via into_future.

    This is the DISCRIMINATING test for the into_future fix (Task 3).

    With the OLD run_coroutine_threadsafe / asyncio.run() fallback:
      - The coroutine runs on a NEW event loop created on the tokio thread
      - asyncio.get_running_loop() inside the handler returns a DIFFERENT loop

    With the NEW into_future() approach:
      - The coroutine is driven by the original event loop (from task locals)
      - asyncio.get_running_loop() inside the handler returns the SAME loop
    """
    import asyncio

    caller_loop = asyncio.get_running_loop()
    handler_loop_holder = {}

    async def loop_detecting_handler(event, data):
        handler_loop_holder["loop"] = asyncio.get_running_loop()
        return {"action": "continue"}

    registry = RustHookRegistry()
    registry.register("test:event", loop_detecting_handler, 0, name="loop-detect")
    await registry.emit("test:event", {"key": "value"})

    # With into_future, the handler coroutine runs on the SAME event loop
    # as the caller (the one that pytest-asyncio set up).
    # With the old asyncio.run() fallback, it would be a different loop.
    assert "loop" in handler_loop_holder, "Handler coroutine was never awaited"
    assert handler_loop_holder["loop"] is caller_loop, (
        "Handler ran on a different event loop — "
        "this means asyncio.run() was used instead of into_future()"
    )
