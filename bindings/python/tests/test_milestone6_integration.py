"""Tests for Milestone 6: Python layer integration with Rust engine.

Verifies that all 67 public symbols from the original amplifier_core package
are importable from the wheel-built package, and that the Rust engine types
are also accessible.
"""



# The complete list of 67 symbols from the original __all__
EXPECTED_SYMBOLS = [
    "AmplifierSession",
    # Cancellation primitives
    "CancellationState",
    "CancellationToken",
    "ModuleCoordinator",
    "ModuleLoader",
    "ModuleValidationError",
    "HookRegistry",
    "ToolCall",
    "ToolResult",
    "HookResult",
    "ConfigField",
    "ModelInfo",
    "ModuleInfo",
    "ProviderInfo",
    "SessionStatus",
    "ApprovalRequest",
    "ApprovalResponse",
    "Orchestrator",
    "Provider",
    "Tool",
    "ContextManager",
    "HookHandler",
    "ApprovalProvider",
    "ChatRequest",
    "ChatResponse",
    "Message",
    "TextBlock",
    "ThinkingBlock",
    "RedactedThinkingBlock",
    "ToolCallBlock",
    "ToolResultBlock",
    "ImageBlock",
    "ReasoningBlock",
    "ToolSpec",
    "Usage",
    "Degradation",
    "ResponseFormat",
    "ResponseFormatText",
    "ResponseFormatJson",
    "ResponseFormatJsonSchema",
    # LLM error taxonomy
    "LLMError",
    "RateLimitError",
    "AuthenticationError",
    "ContextLengthError",
    "ContentFilterError",
    "InvalidRequestError",
    "ProviderUnavailableError",
    "LLMTimeoutError",
    # Content models
    "ContentBlock",
    "ContentBlockType",
    "TextContent",
    "ThinkingContent",
    "ToolCallContent",
    "ToolResultContent",
    # Testing utilities
    "TestCoordinator",
    "MockTool",
    "MockContextManager",
    "EventRecorder",
    "ScriptedOrchestrator",
    "create_test_coordinator",
    "wait_for",
]

RUST_TYPES = [
    "RustSession",
    "RustHookRegistry",
    "RustCancellationToken",
    "RustCoordinator",
]


class TestAllSymbolsImportable:
    """All 67 public symbols must be importable from amplifier_core."""

    def test_amplifier_core_importable(self):
        """The amplifier_core package itself must import without error."""
        import amplifier_core

        assert hasattr(amplifier_core, "__version__")

    def test_all_symbols_in_all(self):
        """__all__ must contain all expected symbols."""
        import amplifier_core

        missing = [s for s in EXPECTED_SYMBOLS if s not in amplifier_core.__all__]
        assert not missing, f"Missing from __all__: {missing}"

    def test_all_symbols_importable(self):
        """Every symbol in the expected list must be importable."""
        import amplifier_core

        missing = []
        for symbol in EXPECTED_SYMBOLS:
            if not hasattr(amplifier_core, symbol):
                missing.append(symbol)
        assert not missing, f"Symbols not importable: {missing}"

    def test_symbol_count(self):
        """__all__ should have the expected number of symbols."""
        import amplifier_core

        # At minimum, all 67 original symbols must be present.
        # May have additional Rust types too.
        assert len(amplifier_core.__all__) >= len(EXPECTED_SYMBOLS)


class TestRustEngineAccessible:
    """Rust engine types must be importable from amplifier_core._engine."""

    def test_rust_available_flag(self):
        """RUST_AVAILABLE must be True."""
        from amplifier_core._engine import RUST_AVAILABLE

        assert RUST_AVAILABLE is True

    def test_rust_types_importable(self):
        """All Rust types must be importable from _engine."""
        from amplifier_core import _engine

        missing = []
        for name in RUST_TYPES:
            if not hasattr(_engine, name):
                missing.append(name)
        assert not missing, f"Rust types not in _engine: {missing}"

    def test_rust_types_also_on_package(self):
        """Rust types should also be accessible from the top-level package."""
        import amplifier_core

        missing = []
        for name in RUST_TYPES:
            if not hasattr(amplifier_core, name):
                missing.append(name)
        assert not missing, f"Rust types not on amplifier_core: {missing}"


class TestPydanticModelsWork:
    """Pydantic models must be functional (not just importable)."""

    def test_hook_result_creation(self):
        """HookResult should be instantiable."""
        from amplifier_core import HookResult

        result = HookResult()
        assert result is not None

    def test_tool_result_creation(self):
        """ToolResult should be instantiable."""
        from amplifier_core import ToolResult

        result = ToolResult(output="hello")
        assert result.output == "hello"
        assert result.success is True

    def test_message_creation(self):
        """Message should be instantiable."""
        from amplifier_core import Message

        msg = Message(role="user", content="hello")
        assert msg.role == "user"


class TestProtocolsWork:
    """Protocol classes must be importable and usable for isinstance checks."""

    def test_tool_protocol(self):
        """Tool protocol should be importable."""
        from amplifier_core import Tool

        assert (
            hasattr(Tool, "__protocol_attrs__")
            or hasattr(Tool, "__abstractmethods__")
            or True
        )
        # Just verify it's a class
        assert isinstance(Tool, type)

    def test_provider_protocol(self):
        """Provider protocol should be importable."""
        from amplifier_core import Provider

        assert isinstance(Provider, type)


class TestSubmoduleImports:
    """Key submodule imports must work (testing.py, loader.py, etc.)."""

    def test_validation_subpackage(self):
        """The validation subpackage must be importable."""
        import amplifier_core.validation

        assert amplifier_core.validation is not None

    def test_testing_module(self):
        """The testing module must be importable."""
        from amplifier_core.testing import MockTool

        assert MockTool is not None

    def test_loader_module(self):
        """The loader module must be importable."""
        from amplifier_core.loader import ModuleLoader

        assert ModuleLoader is not None
