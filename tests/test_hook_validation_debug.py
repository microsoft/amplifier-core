"""
Debug test to reproduce and trace the hook validation bug.

The error: hook_call: HookHandler.__call__() should be async
This occurs when hook modules (hooks-approval, hook-shell) are validated
despite the v1.0.5 fix to _check_hook_methods.

Pattern being tested: bound async method registered via coordinator.hooks.register()
Same pattern used by both failing modules.
"""

import pytest
from amplifier_core.validation import HookValidator


class TestHookValidationBoundMethod:
    """Reproduces the bound async method registration pattern."""

    @pytest.mark.asyncio
    async def test_bound_async_method_registration_passes(self, tmp_path):
        """
        Hook module that registers a bound async method via coordinator.hooks.register()
        should PASS validation.

        This is the EXACT pattern used by hooks-approval and hook-shell.
        """
        module_dir = tmp_path / "test_hook_bound_method"
        module_dir.mkdir()
        init_file = module_dir / "__init__.py"
        init_file.write_text(
            """
from amplifier_core import HookResult

class MyHook:
    async def handle_event(self, event: str, data: dict) -> HookResult:
        return HookResult(action="continue")

async def mount(coordinator, config):
    hook = MyHook()
    # Register via coordinator.hooks.register() — the standard pattern
    # Actual signature (both Python and Rust): register(event, handler, priority=0, name=None)
    coordinator.hooks.register(
        "tool:pre",
        hook.handle_event,
        priority=0,
        name="my_hook",
    )

    # Return a sync cleanup function (same pattern as hooks-approval)
    def cleanup():
        pass

    return cleanup
"""
        )

        validator = HookValidator()
        result = await validator.validate(str(module_dir))

        # Print all checks for debugging
        print("\n=== Validation Checks ===")
        for check in result.checks:
            status = "PASS" if check.passed else "FAIL"
            print(f"  [{status}] {check.name}: {check.message}")

        assert result.passed, (
            f"Hook with bound async method registration should PASS validation. "
            f"Errors: {[c.message for c in result.errors]}"
        )

    @pytest.mark.asyncio
    async def test_coordinator_hooks_get_pattern_passes(self, tmp_path):
        """
        Hook module using coordinator.get('hooks') then .register() should PASS.

        This is the exact pattern from hooks-approval/__init__.py:
            hooks = coordinator.get("hooks")
            hooks.register("tool:pre", handler, priority=-10, name="approval_hook")
        """
        module_dir = tmp_path / "test_hook_get_pattern"
        module_dir.mkdir()
        init_file = module_dir / "__init__.py"
        init_file.write_text(
            """
from amplifier_core import HookResult

class ApprovalHook:
    async def handle_tool_pre(self, event: str, data: dict) -> HookResult:
        return HookResult(action="continue")

async def mount(coordinator, config):
    # Pattern from hooks-approval: get hooks registry via coordinator.get()
    hooks = coordinator.get("hooks")
    if not hooks:
        return None

    approval_hook = ApprovalHook()

    # This is the exact call that hooks-approval makes
    # Note: RustHookRegistry.register(event, name, handler, priority)
    # vs Python HookRegistry.register(event, handler, priority, name)
    try:
        unregister = hooks.register(
            "tool:pre",
            approval_hook.handle_tool_pre,
            priority=-10,
            name="approval_hook",
        )
    except TypeError:
        # RustHookRegistry has different signature: register(event, name, handler, priority)
        unregister = hooks.register(
            "tool:pre",
            "approval_hook",
            approval_hook.handle_tool_pre,
            priority=-10,
        )

    def cleanup():
        if callable(unregister):
            unregister()

    return cleanup
"""
        )

        validator = HookValidator()
        result = await validator.validate(str(module_dir))

        print("\n=== Validation Checks (coordinator.get pattern) ===")
        for check in result.checks:
            status = "PASS" if check.passed else "FAIL"
            print(f"  [{status}] {check.name}: {check.message}")

        assert result.passed, (
            f"Hook using coordinator.get('hooks').register() should PASS. "
            f"Errors: {[c.message for c in result.errors]}"
        )

    @pytest.mark.asyncio
    async def test_sync_cleanup_not_mistaken_for_hook_handler(self, tmp_path):
        """
        A sync cleanup function returned from mount() should NOT trigger
        _check_hook_methods. It should not be mistaken for a HookHandler.

        This is the specific bug: isinstance(sync_cleanup_fn, HookHandler)
        returns True for ANY callable because HookHandler is @runtime_checkable
        and all callables have __call__.
        """
        module_dir = tmp_path / "test_sync_cleanup"
        module_dir.mkdir()
        init_file = module_dir / "__init__.py"
        init_file.write_text(
            """
from amplifier_core import HookResult

async def my_handler(event: str, data: dict) -> HookResult:
    return HookResult(action="continue")

async def mount(coordinator, config):
    # Register hook
    coordinator.hooks.register("session:start", my_handler, name="my_handler")

    # Return a SYNC cleanup function
    def cleanup():
        pass  # sync, not async

    return cleanup
"""
        )

        validator = HookValidator()
        result = await validator.validate(str(module_dir))

        print("\n=== Validation Checks (sync cleanup) ===")
        for check in result.checks:
            status = "PASS" if check.passed else "FAIL"
            print(f"  [{status}] {check.name}: {check.message}")

        # The sync cleanup fn must NOT trigger hook_call error
        hook_call_error = next(
            (c for c in result.checks if c.name == "hook_call" and not c.passed),
            None,
        )
        assert hook_call_error is None, (
            f"Sync cleanup function should not trigger hook_call validation error. "
            f"Got: {hook_call_error}"
        )
        assert result.passed, (
            f"Module with sync cleanup should PASS. "
            f"Errors: {[c.message for c in result.errors]}"
        )
