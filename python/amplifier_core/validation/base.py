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

import _imp
import importlib
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
    """Import a Python source path through Python's normal import machinery.

    Validation must use the canonical module object that runtime loading will
    mount.  If another source already owns the same package name, fail closed
    rather than replacing entries in ``sys.modules`` while another importer can
    observe them.
    """
    path = Path(module_path)
    source_path = path / "__init__.py" if path.is_dir() else path
    module_name = path.name if path.is_dir() else path.stem

    import_root = path.parent if path.is_dir() else source_path.parent
    parent = str(import_root)
    _imp.acquire_lock()
    try:
        existing = sys.modules.get(module_name)
        if existing is not None:
            existing_file = getattr(existing, "__file__", None)
            if (
                existing_file is not None
                and Path(existing_file).resolve() == source_path.resolve()
            ):
                return existing
            raise ImportError(
                f"Refusing to import '{module_name}' from {source_path}: "
                f"it is already loaded from {existing_file}"
            )

        package_dir = source_path.parent.resolve()
        for cached_name, cached_module in sys.modules.items():
            if not cached_name.startswith(f"{module_name}."):
                continue
            cached_file = getattr(cached_module, "__file__", None)
            if cached_file is None or not Path(cached_file).resolve().is_relative_to(
                package_dir
            ):
                raise ImportError(
                    f"Refusing to import '{module_name}' from {source_path}: "
                    f"cached submodule '{cached_name}' is from {cached_file}"
                )

        try:
            original_path_index = sys.path.index(parent)
        except ValueError:
            original_path_index = None
        next_path = (
            sys.path[original_path_index + 1]
            if original_path_index is not None
            and original_path_index + 1 < len(sys.path)
            else None
        )
        if original_path_index is None:
            sys.path.insert(0, parent)
        elif original_path_index != 0:
            sys.path.pop(original_path_index)
            sys.path.insert(0, parent)
        try:
            module = importlib.import_module(module_name)
        finally:
            current_path_index = next(
                (index for index, value in enumerate(sys.path) if value is parent),
                None,
            )
            if original_path_index is None:
                if current_path_index is not None:
                    sys.path.pop(current_path_index)
            elif original_path_index != 0 and current_path_index is not None:
                sys.path.pop(current_path_index)
                next_path_index = next(
                    (index for index, value in enumerate(sys.path) if value is next_path),
                    None,
                )
                if next_path_index is None:
                    sys.path.append(parent)
                else:
                    sys.path.insert(next_path_index, parent)
    finally:
        _imp.release_lock()

    imported_file = getattr(module, "__file__", None)
    if imported_file is None or Path(imported_file).resolve() != source_path.resolve():
        raise ImportError(
            f"Refusing to validate '{module_name}' from {source_path}: "
            f"Python imported {imported_file}"
        )
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
