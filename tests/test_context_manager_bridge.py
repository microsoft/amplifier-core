"""Tests for PyContextManagerBridge via RustCoordinator.process_hook_result.

Verifies that when a Python context manager is mounted at 'context' and
process_hook_result is called with an inject_context HookResult, the bridge:
  - Calls add_message on the Python context manager (sync or async)
  - Skips add_message for ephemeral injections (but still counts tokens)
"""

import pytest


@pytest.mark.asyncio
async def test_inject_context_calls_add_message():
    """inject_context HookResult calls add_message on the mounted context manager (async)."""
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    class FakeContext:
        def __init__(self):
            self.messages = []

        async def add_message(self, message):
            self.messages.append(message)

    coord = RustCoordinator()
    ctx = FakeContext()
    await coord.mount("context", ctx)

    result = HookResult(
        action="inject_context",
        context_injection="Linter found error on line 42",
        context_injection_role="system",
    )

    await coord.process_hook_result(result, event="tool:post_exec", hook_name="linter")

    assert len(ctx.messages) == 1
    msg = ctx.messages[0]
    assert msg["role"] == "system"
    assert "Linter found error on line 42" in msg["content"]
    meta = msg["metadata"]
    assert meta["source"] == "hook"
    assert "hook_name" in meta
    assert "event" in meta
    assert "timestamp" in meta


@pytest.mark.asyncio
async def test_inject_context_sync_add_message():
    """inject_context HookResult calls add_message when add_message is sync (not async)."""
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    class FakeContext:
        def __init__(self):
            self.messages = []

        def add_message(self, message):
            # Sync, not async
            self.messages.append(message)

    coord = RustCoordinator()
    ctx = FakeContext()
    await coord.mount("context", ctx)

    result = HookResult(
        action="inject_context",
        context_injection="Linter found error on line 42",
        context_injection_role="system",
    )

    await coord.process_hook_result(result, event="tool:post_exec", hook_name="linter")

    assert len(ctx.messages) == 1


@pytest.mark.asyncio
async def test_inject_context_ephemeral_skips_add_message():
    """Ephemeral inject_context does NOT call add_message but still counts tokens."""
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    from amplifier_core.models import HookResult

    class FakeContext:
        def __init__(self):
            self.messages = []

        async def add_message(self, message):
            self.messages.append(message)

    coord = RustCoordinator()
    ctx = FakeContext()
    await coord.mount("context", ctx)

    result = HookResult(
        action="inject_context",
        context_injection="Linter found error on line 42",
        context_injection_role="system",
        ephemeral=True,
    )

    await coord.process_hook_result(result, event="tool:post_exec", hook_name="linter")

    # Ephemeral: add_message must NOT be called
    assert len(ctx.messages) == 0

    # Token counting still happens even for ephemeral injections
    assert coord._current_turn_injections > 0
