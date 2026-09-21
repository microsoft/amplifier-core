"""
Tests for thin hooks.py alias that re-exports Rust-backed HookRegistry.
"""

import amplifier_core.hooks as hooks_module
from amplifier_core.hooks import ContextInjection, HookRegistry, HookResult
from amplifier_core.models import ContextInjection as ModelContextInjection


def test_import_hook_registry():
    """HookRegistry from hooks.py is the Rust-backed RustHookRegistry."""
    assert HookRegistry.__name__ == "RustHookRegistry"


def test_import_hook_result():
    """HookResult is importable from amplifier_core.hooks."""
    result = HookResult(action="continue")
    assert result.action == "continue"


def test_hook_registry_has_class_constants():
    """RustHookRegistry exposes the expected lifecycle event constants."""
    assert HookRegistry.SESSION_START == "session:start"
    assert HookRegistry.SESSION_END == "session:end"
    assert HookRegistry.PROMPT_SUBMIT == "prompt:submit"
    assert HookRegistry.TOOL_PRE == "tool:pre"
    assert HookRegistry.TOOL_POST == "tool:post"
    assert HookRegistry.CONTEXT_PRE_COMPACT == "context:pre_compact"
    assert HookRegistry.ORCHESTRATOR_COMPLETE == "orchestrator:complete"
    assert HookRegistry.USER_NOTIFICATION == "user:notification"


def test_hook_registry_instantiation():
    """HookRegistry can be instantiated and list_handlers returns a dict."""
    registry = HookRegistry()
    result = registry.list_handlers()
    assert isinstance(result, dict)


def test_all_exports():
    """__all__ exposes every public hook result type."""
    all_exports = getattr(hooks_module, "__all__", None)
    assert all_exports is not None, "hooks module should define __all__"
    assert set(all_exports) == {"HookRegistry", "ContextInjection", "HookResult"}
    assert ContextInjection is ModelContextInjection
