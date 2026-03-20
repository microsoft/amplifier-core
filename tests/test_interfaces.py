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
        """Tool protocol must expose input_schema as a property.

        This is the RED test: before input_schema is added to the Protocol,
        Tool will not have this attribute.
        """
        assert hasattr(Tool, "input_schema"), (
            "Tool protocol must define input_schema property "
            "(PR #22 — add input_schema with empty-dict default)"
        )

    def test_tool_without_input_schema_satisfies_structural_check(self):
        """A Tool that does NOT define input_schema must pass structural conformance.

        Structural conformance is checked with hasattr(), not isinstance().
        The Protocol classes are NOT @runtime_checkable — isinstance() against
        them would raise TypeError on Python 3.11+. The validation framework
        (and the kernel runtime) uses hasattr()-based duck typing instead,
        which works identically on Python 3.11, 3.12 and 3.13.

        Backward-compat: existing tools predate input_schema and must not break.
        The core required members are name, description, and execute;
        input_schema is intentionally optional with a {} default.
        """
        tool = _MinimalTool()
        # Core required members must be present
        assert hasattr(tool, "name"), "Tool must have 'name'"
        assert hasattr(tool, "description"), "Tool must have 'description'"
        assert hasattr(tool, "execute"), "Tool must have 'execute'"
        assert callable(getattr(tool, "execute")), "Tool.execute must be callable"
        # input_schema is optional — getattr with fallback must work
        schema = getattr(tool, "input_schema", {})
        assert isinstance(schema, dict), "Tool.input_schema fallback must return a dict"

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

    def test_tool_is_not_runtime_checkable(self):
        """Tool protocol must NOT be @runtime_checkable.

        The @runtime_checkable decorator plus @property members causes silent
        breakage on Python 3.11: the Tool.__protocol_attrs__ override (added to
        work around the input_schema backward-compat issue) leaks as a spurious
        required member in Python 3.11's _get_protocol_attrs(), making
        isinstance(tool, Tool) return False for every tool — even fully
        conformant ones.

        The correct approach is structural duck typing with hasattr(), which the
        kernel runtime and validation framework already use. Keeping the Protocol
        class without @runtime_checkable gives us importable contract
        documentation, IDE hover support, and pyright type checking without
        any runtime isinstance() hazards.
        """
        # Tool must not be flagged as a runtime-checkable protocol
        assert not getattr(Tool, "_is_runtime_protocol", False), (
            "Tool must not be @runtime_checkable — use hasattr() structural "
            "checks instead of isinstance() to avoid Python 3.11 breakage"
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

    def test_orchestrator_is_not_runtime_checkable(self):
        """Orchestrator protocol must NOT be @runtime_checkable.

        Same reasoning as Tool: use hasattr()-based structural checks.
        """
        assert not getattr(Orchestrator, "_is_runtime_protocol", False), (
            "Orchestrator must not be @runtime_checkable — use hasattr() "
            "structural checks instead of isinstance()"
        )
