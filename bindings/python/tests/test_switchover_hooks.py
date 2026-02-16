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
    registry.on("tool:pre", "test-handler", my_handler, 50)
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
    registry.register("tool:pre", "my-hook", lambda e, d: None, 0)
    registry.register("tool:post", "other-hook", lambda e, d: None, 0)

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
