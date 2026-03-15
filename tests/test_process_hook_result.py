"""Comprehensive tests for process_hook_result() action branches.

Covers every branch of process_hook_result on RustCoordinator:
- inject_context: size limit enforcement, budget soft-warning
- ask_user: approved, denied, no approval system, timeout with deny default
- user_message: with display system, without display system (log fallback)
- continue: result returned unchanged
"""

import types

import pytest


def _make_coordinator(*, injection_size_limit=None, injection_budget_per_turn=None):
    """Create a RustCoordinator with optional session config.

    Uses types.SimpleNamespace as a fake session object with configurable
    injection_size_limit and injection_budget_per_turn.
    """
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    config = {"session": {"orchestrator": "loop-basic"}}
    if injection_size_limit is not None:
        config["session"]["injection_size_limit"] = injection_size_limit
    if injection_budget_per_turn is not None:
        config["session"]["injection_budget_per_turn"] = injection_budget_per_turn

    fake_session = types.SimpleNamespace(
        session_id="test-session",
        parent_id=None,
        config=config,
    )
    return RustCoordinator(fake_session)


# ---------------------------------------------------------------------------
# inject_context tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_inject_context_size_limit_exceeded():
    """inject_context raises ValueError when content exceeds the size limit."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator(injection_size_limit=10)

    result = HookResult(
        action="inject_context",
        context_injection="x" * 20,
    )

    with pytest.raises(ValueError, match="exceeds 10 bytes"):
        await coord.process_hook_result(
            result, event="tool:post_exec", hook_name="test"
        )


@pytest.mark.asyncio
async def test_inject_context_budget_exceeded_logs_warning():
    """Budget exceeded is a soft warning: no exception raised, message still added.

    'x'*40 = 10 tokens (40 // 4), which exceeds budget of 1.
    The injection should proceed and the message should be added to context.
    """
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator(injection_budget_per_turn=1)

    class FakeContext:
        def __init__(self):
            self.messages = []

        async def add_message(self, message):
            self.messages.append(message)

    ctx = FakeContext()
    await coord.mount("context", ctx)

    result = HookResult(
        action="inject_context",
        context_injection="x" * 40,  # 10 tokens > budget of 1
    )

    # Should NOT raise (soft warning only)
    await coord.process_hook_result(result, event="tool:post_exec", hook_name="test")

    # Message should still be added despite budget exceeded
    assert len(ctx.messages) == 1


# ---------------------------------------------------------------------------
# ask_user tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_ask_user_approved():
    """When FakeApproval returns 'Allow once', processed result action is 'continue'."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator()

    class FakeApproval:
        async def request_approval(self, prompt, options, timeout, default):
            return "Allow once"

    coord.approval_system = FakeApproval()

    result = HookResult(action="ask_user", approval_prompt="Allow this?")
    processed = await coord.process_hook_result(
        result, event="tool:pre_exec", hook_name="test"
    )

    assert processed.action == "continue"


@pytest.mark.asyncio
async def test_ask_user_denied():
    """When FakeApproval returns 'Deny', processed action is 'deny' with 'User denied' in reason."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator()

    class FakeApproval:
        async def request_approval(self, prompt, options, timeout, default):
            return "Deny"

    coord.approval_system = FakeApproval()

    result = HookResult(action="ask_user", approval_prompt="Allow this?")
    processed = await coord.process_hook_result(
        result, event="tool:pre_exec", hook_name="test"
    )

    assert processed.action == "deny"
    assert "User denied" in processed.reason


@pytest.mark.asyncio
async def test_ask_user_no_approval_system():
    """With no approval system, processed action is 'deny' with 'No approval system' in reason."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator()
    # No approval system set (default is None)

    result = HookResult(action="ask_user", approval_prompt="Allow this?")
    processed = await coord.process_hook_result(
        result, event="tool:pre_exec", hook_name="test"
    )

    assert processed.action == "deny"
    assert "No approval system" in processed.reason


@pytest.mark.asyncio
async def test_ask_user_timeout_deny_default():
    """Approval timeout with deny default: action is 'deny', 'timeout' in reason (case-insensitive)."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.approval import ApprovalTimeoutError
    from amplifier_core.models import HookResult

    coord = _make_coordinator()

    class FakeApproval:
        async def request_approval(self, prompt, options, timeout, default):
            raise ApprovalTimeoutError()

    coord.approval_system = FakeApproval()

    result = HookResult(
        action="ask_user",
        approval_prompt="Allow this?",
        approval_default="deny",
    )
    processed = await coord.process_hook_result(
        result, event="tool:pre_exec", hook_name="test"
    )

    assert processed.action == "deny"
    assert "timeout" in processed.reason.lower()


# ---------------------------------------------------------------------------
# user_message tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_user_message_with_display_system():
    """User message is routed to display system with correct message, level, and source."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator()

    class FakeDisplay:
        def __init__(self):
            self.messages = []

        def show_message(self, message, level, source):
            self.messages.append({"msg": message, "level": level, "source": source})

    display = FakeDisplay()
    coord.display_system = display

    result = HookResult(
        action="continue",
        user_message="Found 3 issues",
        user_message_level="warning",
    )
    await coord.process_hook_result(result, event="tool:post_exec", hook_name="checker")

    assert len(display.messages) == 1
    entry = display.messages[0]
    assert entry["msg"] == "Found 3 issues"
    assert entry["level"] == "warning"
    assert "checker" in entry["source"]


@pytest.mark.asyncio
async def test_user_message_no_display_falls_back_to_log():
    """With no display system, user message falls back to log without raising."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator()
    # No display system set (default is None)

    result = HookResult(
        action="continue",
        user_message="Some status message",
        user_message_level="info",
    )

    # Should not raise — falls back to log
    await coord.process_hook_result(result, event="tool:post_exec", hook_name="test")


# ---------------------------------------------------------------------------
# continue tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_continue_returns_result_unchanged():
    """Continue action: the original result is returned with action=='continue'."""
    try:
        from amplifier_core._engine import RustCoordinator  # noqa: F401
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    coord = _make_coordinator()

    result = HookResult(action="continue")
    processed = await coord.process_hook_result(
        result, event="tool:post_exec", hook_name="test"
    )

    assert processed.action == "continue"
