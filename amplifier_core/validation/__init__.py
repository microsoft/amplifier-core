"""
Module validation framework.

Provides validators for checking module compliance with Amplifier protocols.
Uses dynamic import to validate at runtime via isinstance() with runtime_checkable protocols.

Validators check:
1. Module is importable
2. mount() function exists with correct signature
3. Mounted instance implements required protocol
4. Required methods exist with correct signatures

Example usage:
    from amplifier_core.validation import ToolValidator, ValidationResult

    validator = ToolValidator()
    result = await validator.validate("./my-tool-module")

    if result.passed:
        print(f"Module valid: {result.summary()}")
    else:
        for error in result.errors:
            print(f"Error: {error.message}")
"""

from .base import ValidationCheck
from .base import ValidationResult
from .context import ContextValidator
from .hook import HookValidator
from .orchestrator import OrchestratorValidator
from .provider import ProviderValidator
from .tool import ToolValidator

__all__ = [
    "ValidationCheck",
    "ValidationResult",
    "ProviderValidator",
    "ToolValidator",
    "HookValidator",
    "OrchestratorValidator",
    "ContextValidator",
]
