"""
Structural validation tests for Amplifier modules.

Provides exportable test base classes that modules inherit to run standard
structural validation. Tests use the same fixtures as behavioral tests.

Usage:
    # In module's tests/test_structural.py (or alongside behavioral tests)
    from amplifier_core.validation.structural import ToolStructuralTests

    class TestMyToolStructural(ToolStructuralTests):
        '''Inherits all standard tool structural tests.'''
        pass

    # Running tests in module directory picks up the inherited tests
    # pytest tests/ -v

Available base classes:
    - ProviderStructuralTests: For provider modules
    - ToolStructuralTests: For tool modules
    - HookStructuralTests: For hook modules
    - OrchestratorStructuralTests: For orchestrator modules
    - ContextStructuralTests: For context manager modules

Philosophy:
    - Single source of truth: Test definitions live in amplifier-core only
    - Automatic updates: Update core → all modules get new tests
    - Module self-contained: Each module works standalone with pytest
    - Consistent pattern: Mirrors behavioral test inheritance pattern
    - No duplication: Modules just inherit, no copy-paste
"""

import inspect
from typing import Any

from ..base import ValidationCheck
from .test_context import ContextStructuralTests
from .test_hook import HookStructuralTests
from .test_orchestrator import OrchestratorStructuralTests
from .test_provider import ProviderStructuralTests
from .test_tool import ToolStructuralTests

__all__ = [
    "ProviderStructuralTests",
    "ToolStructuralTests",
    "HookStructuralTests",
    "OrchestratorStructuralTests",
    "ContextStructuralTests",
    "check_on_session_ready",
]


def check_on_session_ready(module: Any) -> ValidationCheck | None:
    """Check whether a module's on_session_ready() function, if present, is valid.

    Validates:
    1. Presence: returns None when on_session_ready is absent (no check needed).
    2. Async: returns a failing ValidationCheck when on_session_ready exists but
       is not async (must be ``async def``).
    3. Arity (B5): returns a failing ValidationCheck when on_session_ready exists,
       is async, but accepts no positional arguments — the coordinator argument
       is required.

    Args:
        module: The imported module object to inspect.

    Returns:
        None if no issue found, or a ValidationCheck with passed=False describing
        the first problem encountered.
    """
    fn = getattr(module, "on_session_ready", None)
    if fn is None:
        return None
    if not inspect.iscoroutinefunction(fn):
        return ValidationCheck(
            name="on_session_ready_async",
            passed=False,
            message=(
                "on_session_ready() must be async: found sync function. "
                "Use 'async def on_session_ready(coordinator) -> None:'"
            ),
            severity="error",
        )
    # B5 fix: validate arity — must accept at least one positional arg (coordinator)
    try:
        sig = inspect.signature(fn)
        positional_params = [
            p
            for p in sig.parameters.values()
            if p.kind
            in (
                inspect.Parameter.POSITIONAL_OR_KEYWORD,
                inspect.Parameter.POSITIONAL_ONLY,
            )
            and p.default is inspect.Parameter.empty
        ]
        if len(positional_params) < 1:
            return ValidationCheck(
                name="on_session_ready_async",
                passed=False,
                message=(
                    "on_session_ready() must accept a coordinator argument: "
                    "async def on_session_ready(coordinator) -> None"
                ),
                severity="error",
            )
    except (ValueError, TypeError):
        pass  # Can't inspect — let it pass; runtime will catch it
    return None
