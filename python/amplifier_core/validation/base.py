"""
Base types for module validation.

Provides ValidationCheck and ValidationResult dataclasses used by all validators,
plus structural-check helper functions that operate on imported module objects
without depending on the test-class hierarchy in ``validation.structural``.

NOTE: ``check_on_session_ready`` lives here (not in ``validation.structural``)
so the per-type validators can import it without pulling in the pytest-dependent
test base classes at module-load time. See incident #5 in
``context/release-mandate.md`` for the v1.4.0 regression that motivated this.
"""

import inspect
from dataclasses import dataclass
from dataclasses import field
from typing import Any
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

    Note:
        This function lives in ``validation.base`` (not ``validation.structural``)
        so that the per-type validators can import it without triggering the
        pytest-dependent test base classes in ``validation.structural``. See
        incident #5 in ``context/release-mandate.md`` for the v1.4.0 regression
        that motivated this placement.
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
