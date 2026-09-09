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

import importlib.util
import inspect
import sys
from dataclasses import dataclass
from dataclasses import field
from pathlib import Path
from types import ModuleType
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


def import_module_from_path(module_path: str | Path) -> ModuleType:
    """Import a Python source path without duplicating its canonical module.

    When runtime loading already imported the module from the same source,
    validation must inspect that object.  A same-named module from another
    source is deliberately not reused: path validation must validate the
    requested file rather than whichever package happens to be in
    ``sys.modules``.
    """
    path = Path(module_path)
    source_path = path / "__init__.py" if path.is_dir() else path
    module_name = path.name if path.is_dir() else path.stem

    existing = sys.modules.get(module_name)
    if existing is not None:
        existing_file = getattr(existing, "__file__", None)
        if existing_file is not None and Path(existing_file).resolve() == source_path.resolve():
            return existing

    spec = importlib.util.spec_from_file_location(module_name, source_path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Could not load spec for {path}")

    module = importlib.util.module_from_spec(spec)
    module_prefix = f"{module_name}."
    previous_modules = {
        name: value
        for name, value in sys.modules.items()
        if name == module_name or name.startswith(module_prefix)
    }
    for name in previous_modules:
        del sys.modules[name]
    sys.modules[module_name] = module
    try:
        spec.loader.exec_module(module)
    finally:
        for name in list(sys.modules):
            if name == module_name or name.startswith(module_prefix):
                del sys.modules[name]
        sys.modules.update(previous_modules)
    return module


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
