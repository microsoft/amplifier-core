"""Tests for interface protocol contracts."""

import inspect

from amplifier_core.interfaces import Orchestrator


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
