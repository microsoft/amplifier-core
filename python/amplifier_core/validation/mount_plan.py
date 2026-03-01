"""
Mount Plan validator.

Validates mount plan structure BEFORE module loading begins.
Catches configuration errors early with clear, actionable error messages.

This is distinct from module validators (ProviderValidator, etc.) which validate
that Python modules implement correct protocols. MountPlanValidator validates
that the mount plan dict itself is well-formed.

Example usage:
    from amplifier_core.validation import MountPlanValidator

    validator = MountPlanValidator()
    result = validator.validate(mount_plan)

    if not result.passed:
        print(result.format_errors())
        sys.exit(1)

    # Safe to proceed with session creation
    session = AmplifierSession.create(mount_plan)
"""

from dataclasses import dataclass
from dataclasses import field
from typing import Any

from .base import ValidationCheck


@dataclass
class MountPlanValidationResult:
    """Complete validation result for a mount plan."""

    checks: list[ValidationCheck] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        """True if no error-severity checks failed."""
        return all(c.passed for c in self.checks if c.severity == "error")

    @property
    def errors(self) -> list[ValidationCheck]:
        """All failed error-severity checks."""
        return [c for c in self.checks if not c.passed and c.severity == "error"]

    @property
    def warnings(self) -> list[ValidationCheck]:
        """All failed warning-severity checks."""
        return [c for c in self.checks if not c.passed and c.severity == "warning"]

    def add(self, check: ValidationCheck) -> None:
        """Add a check to the result."""
        self.checks.append(check)

    def summary(self) -> str:
        """Return a human-readable summary."""
        passed_count = sum(1 for c in self.checks if c.passed)
        status = "PASSED" if self.passed else "FAILED"
        return f"{status}: {passed_count}/{len(self.checks)} checks passed ({len(self.errors)} errors, {len(self.warnings)} warnings)"

    def format_errors(self) -> str:
        """Human-readable error summary for display."""
        if not self.errors:
            return "No errors"

        lines = ["Mount Plan Validation Failed:", ""]
        for i, error in enumerate(self.errors, 1):
            lines.append(f"  {i}. [{error.name}] {error.message}")
        lines.append("")
        lines.append(f"Total: {len(self.errors)} error(s)")
        return "\n".join(lines)


class MountPlanValidator:
    """Validates mount plan structure before module loading.

    Validates:
    - Root structure (is dict, has required sections)
    - Session section (has orchestrator and context)
    - Module spec format (each spec has 'module' field)

    Does NOT validate:
    - Module importability (that's Loader's job)
    - Protocol compliance (that's per-type validators' job)
    - Config values (that's module-specific)
    """

    # Required top-level sections
    REQUIRED_SECTIONS: set[str] = {"session"}
    OPTIONAL_SECTIONS: set[str] = {"providers", "tools", "hooks", "agents"}

    # Required session fields
    REQUIRED_SESSION_FIELDS: set[str] = {"orchestrator", "context"}

    # Required module spec fields
    REQUIRED_MODULE_SPEC_FIELDS: set[str] = {"module"}

    def validate(self, mount_plan: Any) -> MountPlanValidationResult:
        """Validate a mount plan structure.

        Args:
            mount_plan: The mount plan dictionary to validate

        Returns:
            MountPlanValidationResult with all validation checks
        """
        result = MountPlanValidationResult()

        # 1. Validate root structure
        if not self._validate_root_structure(result, mount_plan):
            return result  # Fatal - can't continue

        # 2. Validate session section
        if "session" in mount_plan:
            self._validate_session(result, mount_plan["session"])

        # 3. Validate module lists
        for section in self.OPTIONAL_SECTIONS:
            if section in mount_plan and section != "agents":
                # agents is special - it's a dict of agent configs, not a list of modules
                self._validate_module_list(result, mount_plan[section], section)

        return result

    def _validate_root_structure(self, result: MountPlanValidationResult, mount_plan: Any) -> bool:
        """Check root-level structure. Returns False if fatal error."""
        # Must be a dict
        if not isinstance(mount_plan, dict):
            result.add(
                ValidationCheck(
                    name="root_type",
                    passed=False,
                    message=f"Mount plan must be a dict, got {type(mount_plan).__name__}",
                    severity="error",
                )
            )
            return False

        result.add(
            ValidationCheck(
                name="root_type",
                passed=True,
                message="Mount plan is a dict",
                severity="info",
            )
        )

        # Must have session section
        if "session" not in mount_plan:
            result.add(
                ValidationCheck(
                    name="session_present",
                    passed=False,
                    message="Mount plan missing required 'session' section",
                    severity="error",
                )
            )
        else:
            result.add(
                ValidationCheck(
                    name="session_present",
                    passed=True,
                    message="Session section present",
                    severity="info",
                )
            )

        # Check for unknown sections (warning, not error)
        known = self.REQUIRED_SECTIONS | self.OPTIONAL_SECTIONS
        unknown = set(mount_plan.keys()) - known
        if unknown:
            result.add(
                ValidationCheck(
                    name="unknown_sections",
                    passed=False,  # Flag as warning (but severity=warning so won't fail overall)
                    message=f"Unknown sections will be ignored: {sorted(unknown)}",
                    severity="warning",
                )
            )

        return True

    def _validate_session(self, result: MountPlanValidationResult, session: Any) -> None:
        """Check session section has required fields."""
        # Session must be a dict
        if not isinstance(session, dict):
            result.add(
                ValidationCheck(
                    name="session_type",
                    passed=False,
                    message=f"Session section must be a dict, got {type(session).__name__}",
                    severity="error",
                )
            )
            return

        # Check required session fields
        for field_name in self.REQUIRED_SESSION_FIELDS:
            if field_name not in session:
                result.add(
                    ValidationCheck(
                        name=f"session_{field_name}_present",
                        passed=False,
                        message=f"Session section missing required '{field_name}' field",
                        severity="error",
                    )
                )
            else:
                # Validate the module spec for this field
                self._validate_module_spec(result, session[field_name], f"session.{field_name}")

    def _validate_module_list(
        self,
        result: MountPlanValidationResult,
        modules: Any,
        section_name: str,
    ) -> None:
        """Check each module spec in a list."""
        # Must be a list
        if not isinstance(modules, list):
            result.add(
                ValidationCheck(
                    name=f"{section_name}_type",
                    passed=False,
                    message=f"'{section_name}' section must be a list, got {type(modules).__name__}",
                    severity="error",
                )
            )
            return

        # Empty list is OK (info, not warning)
        if not modules:
            result.add(
                ValidationCheck(
                    name=f"{section_name}_empty",
                    passed=True,
                    message=f"'{section_name}' section is empty",
                    severity="info",
                )
            )
            return

        # Validate each module spec
        for i, spec in enumerate(modules):
            self._validate_module_spec(result, spec, f"{section_name}[{i}]")

    def _validate_module_spec(
        self,
        result: MountPlanValidationResult,
        spec: Any,
        path: str,
    ) -> None:
        """Check individual module spec structure."""
        # Must be a dict
        if not isinstance(spec, dict):
            result.add(
                ValidationCheck(
                    name=f"{path}_type",
                    passed=False,
                    message=f"Module spec at {path} must be a dict, got {type(spec).__name__}",
                    severity="error",
                )
            )
            return

        # Must have 'module' field
        if "module" not in spec:
            result.add(
                ValidationCheck(
                    name=f"{path}_module_required",
                    passed=False,
                    message=(
                        f"Module spec at {path} missing required 'module' field.\n"
                        f"  Got: {spec}\n"
                        f"  Expected: {{'module': 'module-name', 'source': '...', 'config': {{...}}}}"
                    ),
                    severity="error",
                )
            )
        else:
            # Validate module path format
            module_value = spec["module"]
            if not isinstance(module_value, str):
                result.add(
                    ValidationCheck(
                        name=f"{path}_module_type",
                        passed=False,
                        message=f"Module path at {path} must be a string, got {type(module_value).__name__}",
                        severity="error",
                    )
                )
            elif not module_value:
                result.add(
                    ValidationCheck(
                        name=f"{path}_module_empty",
                        passed=False,
                        message=f"Module path at {path} cannot be empty",
                        severity="error",
                    )
                )
            else:
                result.add(
                    ValidationCheck(
                        name=f"{path}_module_valid",
                        passed=True,
                        message=f"Module path '{module_value}' at {path} is valid",
                        severity="info",
                    )
                )

        # Config must be dict if present
        if "config" in spec and not isinstance(spec["config"], dict):
            result.add(
                ValidationCheck(
                    name=f"{path}_config_type",
                    passed=False,
                    message=f"Config at {path} must be a dict, got {type(spec['config']).__name__}",
                    severity="error",
                )
            )

        # Source should be string if present
        if "source" in spec and not isinstance(spec["source"], str):
            result.add(
                ValidationCheck(
                    name=f"{path}_source_type",
                    passed=False,
                    message=f"Source at {path} must be a string, got {type(spec['source']).__name__}",
                    severity="error",
                )
            )
