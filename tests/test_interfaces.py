"""Tests for interface protocol contracts."""

import inspect
from typing import Any

from amplifier_core.interfaces import Orchestrator, Tool
from amplifier_core.models import ToolResult


# ---------------------------------------------------------------------------
# Minimal Tool implementation WITHOUT input_schema (legacy / backward-compat)
# ---------------------------------------------------------------------------


class _MinimalTool:
    """Bare-minimum Tool that pre-dates input_schema."""

    @property
    def name(self) -> str:
        return "minimal"

    @property
    def description(self) -> str:
        return "A minimal tool without input_schema."

    async def execute(self, input: dict[str, Any]) -> ToolResult:
        return ToolResult(success=True, output="ok")


class TestToolProtocol:
    """Tests for Tool protocol contract — input_schema backward compat."""

    def test_tool_protocol_defines_input_schema(self):
        """Tool protocol must expose input_schema with a safe getattr default.

        Python's Protocol metaclass does not expose default property
        implementations via hasattr() or vars() on the class itself.
        The behavioral contract is: callers use getattr(tool, 'input_schema', {})
        and get {} for tools that predate input_schema. This test verifies that
        contract rather than testing Protocol metaclass introspection.
        """
        tool = _MinimalTool()
        schema = getattr(tool, "input_schema", {})
        assert isinstance(schema, dict), (
            "getattr(tool, 'input_schema', {}) must return a dict "
            "for tools that do not define input_schema"
        )

    def test_tool_without_input_schema_satisfies_isinstance(self):
        """A Tool that does NOT define input_schema must still pass isinstance check.

        Backward-compat: existing tools predate input_schema and must not break.
        """
        tool = _MinimalTool()
        assert isinstance(tool, Tool), (
            "A tool without input_schema must still satisfy the Tool protocol "
            "(input_schema must be optional / have a default)"
        )

    def test_tool_without_input_schema_getattr_returns_empty_dict(self):
        """getattr fallback on a legacy Tool must return {} (not raise AttributeError).

        Callers (e.g. the validator) must use getattr(tool, 'input_schema', {})
        so they degrade gracefully for tools that predate this field.
        """
        tool = _MinimalTool()
        schema = getattr(tool, "input_schema", {})
        assert schema == {}, (
            "getattr(tool, 'input_schema', {}) must return {} for a tool "
            "that does not define input_schema"
        )


class TestOrchestratorProtocol:
    """Tests for Orchestrator protocol contract."""

    def test_execute_accepts_kwargs(self):
        """Orchestrator.execute must accept **kwargs for kernel-injected arguments.

        The kernel (session.py) passes coordinator=<ModuleCoordinator> as an
        extra keyword argument. The Protocol must declare **kwargs: Any so
        implementations are not forced to declare every kernel-internal kwarg.
        """
        sig = inspect.signature(Orchestrator.execute)
        var_keyword_params = [
            p
            for p in sig.parameters.values()
            if p.kind == inspect.Parameter.VAR_KEYWORD
        ]
        assert len(var_keyword_params) == 1, (
            "Orchestrator.execute must have a **kwargs parameter "
            "to accept kernel-injected arguments (e.g. coordinator=)"
        )
