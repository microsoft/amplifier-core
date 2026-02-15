"""
Behavioral validation tests for Amplifier modules.

Provides exportable test base classes that modules inherit to run standard
contract validation. Tests use fixtures provided by the amplifier-core pytest plugin.

Usage:
    # In module's tests/test_behavioral.py
    from amplifier_core.validation.behavioral import ProviderBehaviorTests

    class TestMyProviderBehavior(ProviderBehaviorTests):
        '''Inherits all standard provider behavioral tests.'''
        pass

    # Running tests in module directory picks up the inherited tests
    # pytest tests/test_behavioral.py -v

Available base classes:
    - ProviderBehaviorTests: For provider modules
    - ToolBehaviorTests: For tool modules
    - HookBehaviorTests: For hook modules
    - OrchestratorBehaviorTests: For orchestrator modules
    - ContextBehaviorTests: For context manager modules

Philosophy:
    - Single source of truth: Test definitions live in amplifier-core only
    - Automatic updates: Update core → all modules get new tests
    - Module self-contained: Each module works standalone with pytest
    - Extensible: Modules can add custom tests by adding methods
    - No duplication: Modules just inherit, no copy-paste
"""

from .test_context import ContextBehaviorTests
from .test_hook import HookBehaviorTests
from .test_orchestrator import OrchestratorBehaviorTests
from .test_provider import ProviderBehaviorTests
from .test_tool import ToolBehaviorTests

__all__ = [
    "ProviderBehaviorTests",
    "ToolBehaviorTests",
    "HookBehaviorTests",
    "OrchestratorBehaviorTests",
    "ContextBehaviorTests",
]
