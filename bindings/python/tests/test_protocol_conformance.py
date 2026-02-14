"""Protocol conformance tests — verify backward-compatible imports and interfaces.

These tests ensure:
1. All symbols in __all__ are importable from the top level
2. All submodule import paths work
3. PyO3-exposed classes have the expected interface
"""

import amplifier_core


def test_all_top_level_symbols_importable():
    """Every symbol in __all__ must be importable from the top level."""
    for name in amplifier_core.__all__:
        assert hasattr(amplifier_core, name), f"Missing top-level export: {name}"


def test_top_level_symbol_count():
    """The wheel's __all__ must include the original 61 Python symbols + 4 Rust types."""
    # The wheel __init__.py adds RustSession, RustHookRegistry,
    # RustCancellationToken, RustCoordinator on top of the original 61.
    assert len(amplifier_core.__all__) >= 61, (
        f"Expected at least 61 symbols, got {len(amplifier_core.__all__)}"
    )


def test_submodule_imports_models():
    """Submodule import paths for models must work."""
    from amplifier_core.models import HookResult, ToolResult, ConfigField, ModelInfo

    assert HookResult is not None
    assert ToolResult is not None
    assert ConfigField is not None
    assert ModelInfo is not None


def test_submodule_imports_message_models():
    """Submodule import paths for message models must work."""
    from amplifier_core.message_models import (
        ChatRequest,
        ChatResponse,
        Message,
        TextBlock,
        ToolSpec,
        Usage,
    )

    assert ChatRequest is not None
    assert ChatResponse is not None
    assert Message is not None
    assert TextBlock is not None
    assert ToolSpec is not None
    assert Usage is not None


def test_submodule_imports_hooks():
    """Submodule import path for HookRegistry must work."""
    from amplifier_core.hooks import HookRegistry

    assert HookRegistry is not None


def test_submodule_imports_interfaces():
    """Submodule import paths for Protocol interfaces must work."""
    from amplifier_core.interfaces import (
        ApprovalProvider,
        ContextManager,
        HookHandler,
        Orchestrator,
        Provider,
        Tool,
    )

    assert Orchestrator is not None
    assert Provider is not None
    assert Tool is not None
    assert ContextManager is not None
    assert HookHandler is not None
    assert ApprovalProvider is not None


def test_submodule_imports_session():
    """Submodule import path for AmplifierSession must work."""
    from amplifier_core.session import AmplifierSession

    assert AmplifierSession is not None


def test_submodule_imports_events():
    """Submodule import paths for events must work."""
    from amplifier_core.events import ALL_EVENTS, SESSION_START

    assert SESSION_START == "session:start"
    assert len(ALL_EVENTS) >= 40


def test_submodule_imports_cancellation():
    """Submodule import paths for cancellation must work."""
    from amplifier_core.cancellation import CancellationState, CancellationToken

    assert CancellationState is not None
    assert CancellationToken is not None


def test_submodule_imports_coordinator():
    """Submodule import path for ModuleCoordinator must work."""
    from amplifier_core.coordinator import ModuleCoordinator

    assert ModuleCoordinator is not None


def test_submodule_imports_loader():
    """Submodule import paths for module loader must work."""
    from amplifier_core.loader import ModuleLoader, ModuleValidationError

    assert ModuleLoader is not None
    assert ModuleValidationError is not None


def test_submodule_imports_testing():
    """Submodule import paths for testing utilities must work."""
    from amplifier_core.testing import (
        EventRecorder,
        MockContextManager,
        MockTool,
        ScriptedOrchestrator,
        TestCoordinator,
        create_test_coordinator,
        wait_for,
    )

    assert EventRecorder is not None
    assert MockTool is not None
    assert ScriptedOrchestrator is not None
    assert create_test_coordinator is not None


def test_submodule_imports_llm_errors():
    """Submodule import paths for LLM error types must work."""
    from amplifier_core.llm_errors import (
        AuthenticationError,
        ContentFilterError,
        ContextLengthError,
        InvalidRequestError,
        LLMError,
        LLMTimeoutError,
        ProviderUnavailableError,
        RateLimitError,
    )

    assert LLMError is not None
    assert RateLimitError is not None


def test_submodule_imports_content_models():
    """Submodule import paths for content models must work."""
    from amplifier_core.content_models import (
        ContentBlock,
        ContentBlockType,
        TextContent,
        ThinkingContent,
        ToolCallContent,
        ToolResultContent,
    )

    assert ContentBlock is not None
    assert ContentBlockType is not None


# ---- PyO3 class interface tests ----


def test_rust_session_has_expected_interface():
    """Verify RustSession has the methods we expect."""
    from amplifier_core._engine import RustSession

    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config)

    assert hasattr(session, "session_id")
    assert hasattr(session, "execute")
    assert hasattr(session, "initialize")
    assert hasattr(session, "cleanup")

    # Verify session_id returns a non-empty string
    assert isinstance(session.session_id, str)
    assert len(session.session_id) > 0


def test_rust_cancellation_token_interface():
    """Verify RustCancellationToken has the expected interface and behavior."""
    from amplifier_core._engine import RustCancellationToken

    token = RustCancellationToken()

    assert hasattr(token, "request_cancellation")
    assert hasattr(token, "is_cancelled")
    assert hasattr(token, "state")

    # Verify initial state
    assert token.state == "none"
    assert token.is_cancelled() is False

    # Verify cancellation changes state
    token.request_cancellation()
    assert token.is_cancelled() is True
    assert token.state == "graceful"


def test_rust_hook_registry_interface():
    """Verify RustHookRegistry has the expected interface."""
    from amplifier_core._engine import RustHookRegistry

    registry = RustHookRegistry()

    assert hasattr(registry, "register")
    assert hasattr(registry, "emit")
    assert hasattr(registry, "unregister")


def test_rust_coordinator_interface():
    """Verify RustCoordinator has the expected interface."""
    from amplifier_core._engine import RustCoordinator

    coordinator = RustCoordinator()

    assert hasattr(coordinator, "hooks")
    assert hasattr(coordinator, "cancellation")
    assert hasattr(coordinator, "config")

    # Verify property types
    from amplifier_core._engine import RustCancellationToken, RustHookRegistry

    assert isinstance(coordinator.hooks, RustHookRegistry)
    assert isinstance(coordinator.cancellation, RustCancellationToken)
    assert isinstance(coordinator.config, dict)
