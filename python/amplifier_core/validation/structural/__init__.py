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

# Re-export ``check_on_session_ready`` from ``validation.base`` for backward
# compatibility. The function lives in ``base`` (not here) so that the per-type
# validators can import it without triggering the pytest-dependent test base
# classes below. See incident #5 in ``context/release-mandate.md`` for the
# v1.4.0 regression that motivated this placement.
from ..base import check_on_session_ready
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
