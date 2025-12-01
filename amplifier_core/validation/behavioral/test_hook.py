"""
Exportable behavioral test base class for hook modules.

Modules inherit from HookBehaviorTests to run standard contract validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.behavioral import HookBehaviorTests

    class TestMyHookBehavior(HookBehaviorTests):
        pass  # Inherits all standard tests
"""

import pytest

from amplifier_core import HookResult


class HookBehaviorTests:
    """Authoritative behavioral tests for hook modules.

    Modules inherit this class to run standard contract validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_mount_succeeds(self, hook_cleanup, coordinator):
        """mount() must succeed and optionally return cleanup."""
        # If we got here, mount succeeded
        # hook_cleanup is the cleanup function returned by mount()
        assert hook_cleanup is None or callable(hook_cleanup)

    @pytest.mark.asyncio
    async def test_handler_returns_hook_result(self, coordinator):
        """Handler must return HookResult."""
        # Emit a test event - if hooks are registered, they should handle it
        result = await coordinator.hooks.emit("test:event", {"data": "test"})

        # emit() returns None if no handlers, or the combined result
        assert result is None or isinstance(result, HookResult)

    @pytest.mark.asyncio
    async def test_hook_result_has_valid_action(self, coordinator):
        """HookResult must have valid action field."""
        result = await coordinator.hooks.emit("test:event", {"data": "test"})

        if result is not None:
            valid_actions = {"continue", "deny", "modify", "inject_context", "ask_user"}
            assert result.action in valid_actions, f"Invalid action: {result.action}"

    @pytest.mark.asyncio
    async def test_cleanup_is_callable_if_present(self, hook_cleanup):
        """If cleanup returned, it must be callable."""
        if hook_cleanup is not None:
            assert callable(hook_cleanup), "Cleanup must be callable"

    @pytest.mark.asyncio
    async def test_cleanup_does_not_raise(self, hook_cleanup):
        """Cleanup function must not raise exceptions."""
        if hook_cleanup is not None:
            try:
                hook_cleanup()
            except Exception as e:
                pytest.fail(f"Cleanup raised exception: {e}")

    @pytest.mark.asyncio
    async def test_handler_does_not_crash_on_malformed_data(self, coordinator):
        """Handler errors must not crash kernel."""
        try:
            result = await coordinator.hooks.emit("test:event", None)  # type: ignore[arg-type]
            assert result is None or isinstance(result, HookResult)
        except Exception as e:
            assert not isinstance(e, AttributeError | TypeError), f"Hook handler crashed: {e}"

    @pytest.mark.asyncio
    async def test_handler_does_not_crash_on_empty_data(self, coordinator):
        """Handler errors must not crash kernel on empty data."""
        try:
            result = await coordinator.hooks.emit("test:event", {})
            assert result is None or isinstance(result, HookResult)
        except Exception as e:
            assert not isinstance(e, AttributeError | TypeError), f"Hook handler crashed: {e}"
