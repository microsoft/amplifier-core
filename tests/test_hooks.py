"""
Tests for hook registry functionality.
"""

import pytest
from amplifier_core.hooks import HookRegistry
from amplifier_core.models import HookResult


@pytest.mark.asyncio
async def test_register_method():
    """Test register() method works."""
    registry = HookRegistry()

    async def handler(event, data):
        data["handled"] = True
        return HookResult(action="continue")

    unregister = registry.register("test:event", handler, name="test-handler")

    # Verify handler was registered
    handlers = registry.list_handlers("test:event")
    assert "test:event" in handlers
    assert "test-handler" in handlers["test:event"]

    # Test handler is called
    result = await registry.emit("test:event", {})
    assert result.action == "continue"
    assert result.data is not None
    assert result.data["handled"] is True

    # Test unregister
    unregister()
    handlers = registry.list_handlers("test:event")
    assert "test:event" not in handlers or len(handlers["test:event"]) == 0


@pytest.mark.asyncio
async def test_on_alias():
    """Test on() alias works identically to register()."""
    registry = HookRegistry()

    async def handler(event, data):
        data["handled_via_on"] = True
        return HookResult(action="continue")

    unregister = registry.on("test:event", handler, name="on-handler")

    # Verify handler was registered
    handlers = registry.list_handlers("test:event")
    assert "test:event" in handlers
    assert "on-handler" in handlers["test:event"]

    # Test handler is called
    result = await registry.emit("test:event", {})
    assert result.action == "continue"
    assert result.data is not None
    assert result.data["handled_via_on"] is True

    # Test unregister
    unregister()
    handlers = registry.list_handlers("test:event")
    assert "test:event" not in handlers or len(handlers["test:event"]) == 0


@pytest.mark.asyncio
async def test_register_and_on_are_equivalent():
    """Test both methods produce same result and can be used together."""
    registry = HookRegistry()

    async def handler1(event, data):
        data.setdefault("calls", []).append("handler1")
        return HookResult(action="continue")

    async def handler2(event, data):
        data.setdefault("calls", []).append("handler2")
        return HookResult(action="continue")

    # Register one with each method
    registry.register("test:event", handler1, name="handler1")
    registry.on("test:event", handler2, name="handler2")

    # Both should be registered
    handlers = registry.list_handlers("test:event")
    assert len(handlers["test:event"]) == 2
    assert "handler1" in handlers["test:event"]
    assert "handler2" in handlers["test:event"]

    # Both should be called
    result = await registry.emit("test:event", {})
    assert result.action == "continue"
    assert result.data is not None
    assert "handler1" in result.data["calls"]
    assert "handler2" in result.data["calls"]


@pytest.mark.asyncio
async def test_hook_priority():
    """Test hooks execute in priority order."""
    registry = HookRegistry()

    async def low_priority(event, data):
        data.setdefault("order", []).append("low")
        return HookResult(action="continue")

    async def high_priority(event, data):
        data.setdefault("order", []).append("high")
        return HookResult(action="continue")

    # Register with different priorities (lower number = higher priority)
    registry.register("test:event", low_priority, priority=10, name="low")
    registry.register("test:event", high_priority, priority=5, name="high")

    result = await registry.emit("test:event", {})
    assert result.data is not None
    assert result.data["order"] == ["high", "low"]


@pytest.mark.asyncio
async def test_hook_data_modification():
    """Test hooks can modify data."""
    registry = HookRegistry()

    async def modifier(event, data):
        data["modified"] = True
        return HookResult(action="modify", data=data)

    registry.register("test:event", modifier)

    result = await registry.emit("test:event", {"original": True})
    assert result.data is not None
    assert result.data["original"] is True
    assert result.data["modified"] is True


@pytest.mark.asyncio
async def test_hook_deny():
    """Test hook can deny an event."""
    registry = HookRegistry()

    async def deny_handler(event, data):
        return HookResult(action="deny", reason="Test denial")

    async def never_called(event, data):
        data["should_not_be_here"] = True
        return HookResult(action="continue")

    registry.register("test:event", deny_handler, priority=5)
    registry.register("test:event", never_called, priority=10)

    result = await registry.emit("test:event", {})
    assert result.action == "deny"
    assert result.reason == "Test denial"
    # When denied, result.data is None (handler returned no data)
    assert result.data is None


@pytest.mark.asyncio
async def test_hook_error_handling():
    """Test that hook errors don't crash the system."""
    registry = HookRegistry()

    async def failing_handler(event, data):
        raise RuntimeError("Test error")

    async def working_handler(event, data):
        data["still_works"] = True
        return HookResult(action="continue")

    registry.register("test:event", failing_handler, priority=5)
    registry.register("test:event", working_handler, priority=10)

    # Should not raise, should continue to next handler
    result = await registry.emit("test:event", {})
    assert result.action == "continue"
    assert result.data is not None
    assert result.data["still_works"] is True


@pytest.mark.asyncio
async def test_no_handlers():
    """Test emit with no registered handlers."""
    registry = HookRegistry()

    result = await registry.emit("test:event", {"data": "value"})
    assert result.action == "continue"
    assert result.data is not None
    assert result.data["data"] == "value"


@pytest.mark.asyncio
async def test_default_fields():
    """Test set_default_fields merges defaults with event data."""
    registry = HookRegistry()

    # Set default fields (e.g., session_id for traceability)
    registry.set_default_fields(session_id="test-session-123", env="test")

    captured_data = []

    async def capture_handler(event: str, data: dict):
        captured_data.append(data.copy())
        return HookResult(action="continue")

    registry.register("test:event", capture_handler)

    # Test 1: Defaults are merged with explicit data
    await registry.emit("test:event", {"custom": "value"})
    assert captured_data[0]["session_id"] == "test-session-123"
    assert captured_data[0]["env"] == "test"
    assert captured_data[0]["custom"] == "value"

    # Test 2: Explicit event data overrides defaults
    await registry.emit("test:event", {"session_id": "override-456"})
    assert captured_data[1]["session_id"] == "override-456"
    assert captured_data[1]["env"] == "test"  # Non-overridden default persists

    # Test 3: Empty data still gets defaults
    await registry.emit("test:event", {})
    assert captured_data[2]["session_id"] == "test-session-123"
    assert captured_data[2]["env"] == "test"
