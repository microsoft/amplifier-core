"""
Base types for module validation.

Provides ValidationCheck and ValidationResult dataclasses used by all validators.
"""

from dataclasses import dataclass
from dataclasses import field
from typing import Literal


@dataclass
class ValidationCheck:
    """Single validation check result."""

    name: str
    passed: bool
    message: str
    severity: Literal["error", "warning", "info"]


@dataclass
class ValidationResult:
    """Complete validation result for a module."""

    module_type: str
    module_path: str
    checks: list[ValidationCheck] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        """True if no error-level checks failed (warnings OK)."""
        return all(c.passed for c in self.checks if c.severity == "error")

    @property
    def errors(self) -> list[ValidationCheck]:
        """Return only failed error-level checks."""
        return [c for c in self.checks if c.severity == "error" and not c.passed]

    @property
    def warnings(self) -> list[ValidationCheck]:
        """Return only failed warning-level checks."""
        return [c for c in self.checks if c.severity == "warning" and not c.passed]

    def add(self, check: ValidationCheck) -> None:
        """Add a check to the result."""
        self.checks.append(check)

    def summary(self) -> str:
        """Return a human-readable summary."""
        passed_count = sum(1 for c in self.checks if c.passed)
        status = "PASSED" if self.passed else "FAILED"
        return f"{status}: {passed_count}/{len(self.checks)} checks passed ({len(self.errors)} errors, {len(self.warnings)} warnings)"
