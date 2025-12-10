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

Mount Plan validation (validates structure before module loading):
    from amplifier_core.validation import MountPlanValidator

    validator = MountPlanValidator()
    result = validator.validate(mount_plan)

    if not result.passed:
        print(result.format_errors())
        sys.exit(1)
"""

from .base import ValidationCheck
from .base import ValidationResult
from .context import ContextValidator
from .hook import HookValidator
from .mount_plan import MountPlanValidationResult
from .mount_plan import MountPlanValidator
from .orchestrator import OrchestratorValidator
from .provider import ProviderValidator
from .tool import ToolValidator

__all__ = [
    "ValidationCheck",
    "ValidationResult",
    "MountPlanValidationResult",
    "MountPlanValidator",
    "ProviderValidator",
    "ToolValidator",
    "HookValidator",
    "OrchestratorValidator",
    "ContextValidator",
]
