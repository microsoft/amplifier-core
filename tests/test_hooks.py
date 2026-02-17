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


@pytest.mark.asyncio
async def test_ask_user_not_overwritten_by_inject_context():
    """Test that ask_user takes precedence over inject_context.

    This tests the action precedence rule:
    deny > ask_user > inject_context > modify > continue

    When both ask_user and inject_context are returned by different handlers,
    ask_user (a blocking security action) must not be silently overwritten
    by inject_context (a non-blocking information-flow action).
    """
    registry = HookRegistry()

    async def approval_handler(event, data):
        """Handler that requires user approval (higher priority)."""
        return HookResult(
            action="ask_user",
            approval_prompt="Allow this operation?",
            approval_options=["Allow", "Deny"],
            approval_default="deny",
        )

    async def context_handler(event, data):
        """Handler that injects context (lower priority)."""
        return HookResult(
            action="inject_context",
            context_injection="Additional context for the agent",
        )

    # Register approval handler with higher priority (lower number = runs first)
    registry.register("test:event", approval_handler, priority=5, name="approval")
    # Register context handler with lower priority (runs second)
    registry.register("test:event", context_handler, priority=10, name="context")

    result = await registry.emit("test:event", {})

    # ask_user should NOT be overwritten by inject_context
    assert result.action == "ask_user", (
        f"Expected ask_user but got {result.action}. "
        "ask_user must take precedence over inject_context."
    )
    assert result.approval_prompt == "Allow this operation?"
    assert result.approval_default == "deny"


@pytest.mark.asyncio
async def test_inject_context_works_without_ask_user():
    """Test that inject_context works normally when no ask_user is present.

    This ensures the fix for ask_user precedence doesn't break normal
    inject_context functionality.
    """
    registry = HookRegistry()

    async def context_handler_1(event, data):
        return HookResult(
            action="inject_context",
            context_injection="Context from handler 1",
        )

    async def context_handler_2(event, data):
        return HookResult(
            action="inject_context",
            context_injection="Context from handler 2",
        )

    registry.register("test:event", context_handler_1, priority=5, name="ctx1")
    registry.register("test:event", context_handler_2, priority=10, name="ctx2")

    result = await registry.emit("test:event", {})

    # inject_context should be returned and merged
    assert result.action == "inject_context"
    assert "Context from handler 1" in result.context_injection
    assert "Context from handler 2" in result.context_injection


@pytest.mark.asyncio
async def test_ask_user_precedence_regardless_of_handler_order():
    """Test that ask_user takes precedence even when it runs after inject_context.

    The precedence rule should apply regardless of which handler runs first.
    """
    registry = HookRegistry()

    async def context_handler(event, data):
        return HookResult(
            action="inject_context",
            context_injection="Some context",
        )

    async def approval_handler(event, data):
        return HookResult(
            action="ask_user",
            approval_prompt="Approve?",
        )

    # Register context handler FIRST (lower priority number = runs first)
    registry.register("test:event", context_handler, priority=5, name="context")
    # Register approval handler SECOND (runs after context handler)
    registry.register("test:event", approval_handler, priority=10, name="approval")

    result = await registry.emit("test:event", {})

    # Even though inject_context handler ran first, ask_user should win
    assert result.action == "ask_user", (
        f"Expected ask_user but got {result.action}. "
        "ask_user must take precedence over inject_context regardless of order."
    )
    assert result.approval_prompt == "Approve?"


@pytest.mark.asyncio
async def test_deny_takes_precedence_over_ask_user():
    """Test that deny short-circuits before ask_user can be processed.

    This verifies the full action precedence hierarchy:
    deny > ask_user > inject_context > modify > continue

    When deny is returned, it should short-circuit immediately and
    subsequent handlers (including those that would return ask_user)
    should never execute.
    """
    registry = HookRegistry()

    execution_log = []

    async def deny_handler(event, data):
        """Handler that denies the operation (highest priority action)."""
        execution_log.append("deny_handler")
        return HookResult(action="deny", reason="Operation not permitted")

    async def approval_handler(event, data):
        """Handler that would request approval (should never run)."""
        execution_log.append("approval_handler")
        return HookResult(
            action="ask_user",
            approval_prompt="This should never be seen",
        )

    async def context_handler(event, data):
        """Handler that would inject context (should never run)."""
        execution_log.append("context_handler")
        return HookResult(
            action="inject_context",
            context_injection="This should never be injected",
        )

    # Register handlers in priority order
    registry.register("test:event", deny_handler, priority=5, name="deny")
    registry.register("test:event", approval_handler, priority=10, name="approval")
    registry.register("test:event", context_handler, priority=15, name="context")

    result = await registry.emit("test:event", {})

    # deny should short-circuit immediately
    assert result.action == "deny", (
        f"Expected deny but got {result.action}. "
        "deny must short-circuit before other handlers run."
    )
    assert result.reason == "Operation not permitted"

    # Only the deny handler should have executed (short-circuit behavior)
    assert execution_log == ["deny_handler"], (
        f"Expected only deny_handler to run, but got {execution_log}. "
        "deny should short-circuit and prevent subsequent handlers from executing."
    )


# --- Tests for _prepare_event_data() ---


class TestPrepareEventData:
    """Unit tests for HookRegistry._prepare_event_data()."""

    def test_merges_defaults_with_data(self):
        """Defaults are merged, explicit data wins on collision."""
        registry = HookRegistry()
        registry.set_default_fields(session_id="sess-1", env="test")

        result = registry._prepare_event_data({"custom": "value", "env": "prod"})

        assert result["session_id"] == "sess-1"
        assert result["env"] == "prod"  # explicit wins
        assert result["custom"] == "value"

    def test_sequence_increments_monotonically(self):
        """Each call increments the sequence counter starting at 1."""
        registry = HookRegistry()
        registry.set_default_fields(session_id="sess-1")

        r1 = registry._prepare_event_data({})
        r2 = registry._prepare_event_data({})
        r3 = registry._prepare_event_data({})

        assert r1["sequence"] == 1
        assert r2["sequence"] == 2
        assert r3["sequence"] == 3

    def test_event_id_is_nonempty_string(self):
        """event_id is a non-empty string."""
        registry = HookRegistry()
        registry.set_default_fields(session_id="sess-1")

        result = registry._prepare_event_data({})

        assert isinstance(result["event_id"], str)
        assert len(result["event_id"]) > 0

    def test_event_ids_are_unique_across_calls(self):
        """Each call produces a distinct event_id."""
        registry = HookRegistry()
        registry.set_default_fields(session_id="sess-1")

        ids = [registry._prepare_event_data({})["event_id"] for _ in range(5)]

        assert len(set(ids)) == 5

    def test_missing_session_id_falls_back_to_unknown(self):
        """Without session_id in defaults or data, fallback to 'unknown'."""
        registry = HookRegistry()
        # No set_default_fields called, no session_id in data

        result = registry._prepare_event_data({})

        assert result["event_id"].startswith("unknown:")

    def test_infrastructure_keys_overwrite_caller_values(self):
        """Callers cannot override event_id or sequence."""
        registry = HookRegistry()
        registry.set_default_fields(session_id="sess-1")

        result = registry._prepare_event_data(
            {
                "event_id": "caller-fake-id",
                "sequence": 999,
            }
        )

        assert result["event_id"] != "caller-fake-id"
        assert result["sequence"] == 1  # first call, must be 1

    def test_handles_none_data(self):
        """Passing None as data does not crash."""
        registry = HookRegistry()
        registry.set_default_fields(session_id="sess-1")

        result = registry._prepare_event_data(None)

        assert result["session_id"] == "sess-1"
        assert result["sequence"] == 1
        assert isinstance(result["event_id"], str)


# --- Tests for emit_and_collect() with _prepare_event_data() ---


@pytest.mark.asyncio
async def test_emit_and_collect_injects_session_id():
    """emit_and_collect() now merges defaults (bug fix)."""
    registry = HookRegistry()
    registry.set_default_fields(session_id="sess-1")

    captured = []

    async def handler(event, data):
        captured.append(data.copy())
        return HookResult(
            action="continue", data={"saw_session": data.get("session_id")}
        )

    registry.register("test:event", handler)

    responses = await registry.emit_and_collect("test:event", {"key": "val"})

    assert captured[0]["session_id"] == "sess-1"
    assert captured[0]["key"] == "val"
    assert responses == [{"saw_session": "sess-1"}]


@pytest.mark.asyncio
async def test_emit_and_collect_injects_event_id_and_sequence():
    """emit_and_collect() provides event_id and sequence to handlers."""
    registry = HookRegistry()
    registry.set_default_fields(session_id="sess-1")

    captured = []

    async def handler(event, data):
        captured.append(data.copy())
        return HookResult(action="continue", data="ok")

    registry.register("test:event", handler)

    await registry.emit_and_collect("test:event", {})
    await registry.emit_and_collect("test:event", {})

    assert "event_id" in captured[0]
    assert "event_id" in captured[1]
    assert captured[0]["sequence"] == 1
    assert captured[1]["sequence"] == 2
    assert captured[0]["event_id"] != captured[1]["event_id"]


@pytest.mark.asyncio
async def test_emit_and_emit_and_collect_share_sequence():
    """emit() and emit_and_collect() share the same sequence counter."""
    registry = HookRegistry()
    registry.set_default_fields(session_id="sess-1")

    captured = []

    async def handler(event, data):
        captured.append(data.copy())
        return HookResult(action="continue", data="ok")

    registry.register("test:event", handler)

    await registry.emit("test:event", {})  # sequence 1
    await registry.emit_and_collect("test:event", {})  # sequence 2
    await registry.emit("test:event", {})  # sequence 3

    assert captured[0]["sequence"] == 1
    assert captured[1]["sequence"] == 2
    assert captured[2]["sequence"] == 3


@pytest.mark.asyncio
async def test_emit_injects_event_id_and_sequence():
    """emit() provides event_id and sequence to handlers."""
    registry = HookRegistry()
    registry.set_default_fields(session_id="sess-1")

    captured = []

    async def handler(event, data):
        captured.append(data.copy())
        return HookResult(action="continue")

    registry.register("test:event", handler)

    await registry.emit("test:event", {"key": "val1"})
    await registry.emit("test:event", {"key": "val2"})

    # Both events have event_id and sequence
    assert "event_id" in captured[0]
    assert "event_id" in captured[1]
    assert captured[0]["sequence"] == 1
    assert captured[1]["sequence"] == 2

    # event_ids are distinct
    assert captured[0]["event_id"] != captured[1]["event_id"]

    # session_id is still present
    assert captured[0]["session_id"] == "sess-1"

    # caller data is preserved
    assert captured[0]["key"] == "val1"
