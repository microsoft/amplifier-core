"""
Tests for module validation framework.

Tests the validation system that checks if modules correctly implement
their required protocols (Provider, Tool, HookHandler, Orchestrator, ContextManager).
"""

import pytest
from amplifier_core.validation import ContextValidator
from amplifier_core.validation import HookValidator
from amplifier_core.validation import MountPlanValidationResult
from amplifier_core.validation import MountPlanValidator
from amplifier_core.validation import OrchestratorValidator
from amplifier_core.validation import ProviderValidator
from amplifier_core.validation import ToolValidator
from amplifier_core.validation import ValidationCheck
from amplifier_core.validation import ValidationResult


class TestValidationCheck:
    """Tests for ValidationCheck dataclass."""

    def test_create_passing_check(self):
        check = ValidationCheck(
            name="test_check",
            passed=True,
            message="Test passed",
            severity="info",
        )
        assert check.name == "test_check"
        assert check.passed is True
        assert check.message == "Test passed"
        assert check.severity == "info"

    def test_create_failing_check(self):
        check = ValidationCheck(
            name="test_check",
            passed=False,
            message="Test failed",
            severity="error",
        )
        assert check.passed is False
        assert check.severity == "error"

    def test_severity_levels(self):
        from typing import Literal

        severities: list[Literal["error", "warning", "info"]] = ["error", "warning", "info"]
        for severity in severities:
            check = ValidationCheck(
                name="test",
                passed=True,
                message="msg",
                severity=severity,
            )
            assert check.severity == severity


class TestValidationResult:
    """Tests for ValidationResult dataclass."""

    def test_empty_result_passes(self):
        result = ValidationResult(module_type="provider", module_path="/test/path")
        assert result.passed is True
        assert result.errors == []
        assert result.warnings == []

    def test_result_with_passing_checks(self):
        result = ValidationResult(module_type="provider", module_path="/test/path")
        result.add(
            ValidationCheck(
                name="check1",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        result.add(
            ValidationCheck(
                name="check2",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        assert result.passed is True
        assert len(result.checks) == 2

    def test_result_with_error_fails(self):
        result = ValidationResult(module_type="provider", module_path="/test/path")
        result.add(
            ValidationCheck(
                name="check1",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        result.add(
            ValidationCheck(
                name="check2",
                passed=False,
                message="Fail",
                severity="error",
            )
        )
        assert result.passed is False
        assert len(result.errors) == 1
        assert result.errors[0].name == "check2"

    def test_result_with_warning_still_passes(self):
        result = ValidationResult(module_type="provider", module_path="/test/path")
        result.add(
            ValidationCheck(
                name="check1",
                passed=False,
                message="Warning",
                severity="warning",
            )
        )
        assert result.passed is True
        assert len(result.warnings) == 1

    def test_summary_format(self):
        result = ValidationResult(module_type="provider", module_path="/test/path")
        result.add(
            ValidationCheck(
                name="pass",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        result.add(
            ValidationCheck(
                name="fail",
                passed=False,
                message="Fail",
                severity="error",
            )
        )
        summary = result.summary()
        assert "FAILED" in summary
        assert "1/2" in summary
        assert "1 errors" in summary


class TestProviderValidator:
    """Tests for ProviderValidator."""

    @pytest.mark.asyncio
    async def test_validates_nonexistent_module(self):
        validator = ProviderValidator()
        result = await validator.validate("/nonexistent/path")
        assert result.passed is False
        assert any("import" in c.message.lower() for c in result.checks)

    @pytest.mark.asyncio
    async def test_validates_module_without_mount(self, tmp_path):
        # Create a module without mount function
        module_file = tmp_path / "test_provider.py"
        module_file.write_text("x = 1")

        validator = ProviderValidator()
        result = await validator.validate(str(module_file))
        assert result.passed is False
        assert any("mount" in c.message.lower() for c in result.checks if not c.passed)


class TestToolValidator:
    """Tests for ToolValidator."""

    @pytest.mark.asyncio
    async def test_validates_nonexistent_module(self):
        validator = ToolValidator()
        result = await validator.validate("/nonexistent/path")
        assert result.passed is False

    @pytest.mark.asyncio
    async def test_validates_module_without_mount(self, tmp_path):
        module_file = tmp_path / "test_tool.py"
        module_file.write_text("x = 1")

        validator = ToolValidator()
        result = await validator.validate(str(module_file))
        assert result.passed is False
        assert any("mount" in c.message.lower() for c in result.checks if not c.passed)


class TestHookValidator:
    """Tests for HookValidator."""

    @pytest.mark.asyncio
    async def test_validates_nonexistent_module(self):
        validator = HookValidator()
        result = await validator.validate("/nonexistent/path")
        assert result.passed is False

    @pytest.mark.asyncio
    async def test_validates_module_without_mount(self, tmp_path):
        module_file = tmp_path / "test_hook.py"
        module_file.write_text("x = 1")

        validator = HookValidator()
        result = await validator.validate(str(module_file))
        assert result.passed is False
        assert any("mount" in c.message.lower() for c in result.checks if not c.passed)


class TestOrchestratorValidator:
    """Tests for OrchestratorValidator."""

    @pytest.mark.asyncio
    async def test_validates_nonexistent_module(self):
        validator = OrchestratorValidator()
        result = await validator.validate("/nonexistent/path")
        assert result.passed is False

    @pytest.mark.asyncio
    async def test_validates_module_without_mount(self, tmp_path):
        module_file = tmp_path / "test_orchestrator.py"
        module_file.write_text("x = 1")

        validator = OrchestratorValidator()
        result = await validator.validate(str(module_file))
        assert result.passed is False
        assert any("mount" in c.message.lower() for c in result.checks if not c.passed)


class TestContextValidator:
    """Tests for ContextValidator."""

    @pytest.mark.asyncio
    async def test_validates_nonexistent_module(self):
        validator = ContextValidator()
        result = await validator.validate("/nonexistent/path")
        assert result.passed is False

    @pytest.mark.asyncio
    async def test_validates_module_without_mount(self, tmp_path):
        module_file = tmp_path / "test_context.py"
        module_file.write_text("x = 1")

        validator = ContextValidator()
        result = await validator.validate(str(module_file))
        assert result.passed is False
        assert any("mount" in c.message.lower() for c in result.checks if not c.passed)


class TestValidatorMountSignature:
    """Tests that validators properly check mount() signature."""

    @pytest.mark.asyncio
    async def test_sync_mount_fails(self, tmp_path):
        """mount() must be async."""
        module_file = tmp_path / "test_sync_mount.py"
        module_file.write_text(
            """
def mount(coordinator, config):
    return None  # Sync mount - should fail validation
"""
        )

        validator = ProviderValidator()
        result = await validator.validate(str(module_file))
        assert any("async" in c.message.lower() for c in result.checks if not c.passed and c.name == "mount_signature")

    @pytest.mark.asyncio
    async def test_mount_missing_params_fails(self, tmp_path):
        """mount() must have at least 2 parameters."""
        module_file = tmp_path / "test_missing_params.py"
        module_file.write_text(
            """
async def mount(coordinator):
    return None  # Missing config param - should fail validation
"""
        )

        validator = ProviderValidator()
        result = await validator.validate(str(module_file))
        assert any(
            "2 parameters" in c.message.lower() for c in result.checks if not c.passed and c.name == "mount_signature"
        )

    @pytest.mark.asyncio
    async def test_valid_mount_signature_passes(self, tmp_path):
        """Valid async mount(coordinator, config) passes signature check."""
        module_file = tmp_path / "test_valid_mount.py"
        module_file.write_text(
            """
async def mount(coordinator, config):
    return None  # Valid mount signature
"""
        )

        validator = ProviderValidator()
        result = await validator.validate(str(module_file))

        # Should pass importable and mount_exists and mount_signature checks
        signature_check = next((c for c in result.checks if c.name == "mount_signature"), None)
        assert signature_check is not None
        assert signature_check.passed is True


class TestValidatorDirectoryImport:
    """Tests for importing module directories with __init__.py."""

    @pytest.mark.asyncio
    async def test_directory_without_init_fails(self, tmp_path):
        """Directory without __init__.py should fail."""
        module_dir = tmp_path / "test_module"
        module_dir.mkdir()

        validator = ProviderValidator()
        result = await validator.validate(str(module_dir))
        assert result.passed is False
        assert any("__init__.py" in c.message for c in result.checks if not c.passed)

    @pytest.mark.asyncio
    async def test_directory_with_init_imports(self, tmp_path):
        """Directory with __init__.py should import."""
        module_dir = tmp_path / "test_module"
        module_dir.mkdir()
        init_file = module_dir / "__init__.py"
        init_file.write_text(
            """
async def mount(coordinator, config):
    return None  # Valid mount in __init__.py
"""
        )

        validator = ProviderValidator()
        result = await validator.validate(str(module_dir))

        # Should pass importable check
        importable_check = next((c for c in result.checks if c.name == "module_importable"), None)
        assert importable_check is not None
        assert importable_check.passed is True


# =============================================================================
# MountPlanValidator Tests
# =============================================================================


class TestMountPlanValidationResult:
    """Tests for MountPlanValidationResult dataclass."""

    def test_empty_result_passes(self):
        """Empty result (no checks) passes by default."""
        result = MountPlanValidationResult()
        assert result.passed is True
        assert result.errors == []
        assert result.warnings == []

    def test_result_with_passing_checks(self):
        """Result with only passing checks passes."""
        result = MountPlanValidationResult()
        result.add(
            ValidationCheck(
                name="check1",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        result.add(
            ValidationCheck(
                name="check2",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        assert result.passed is True
        assert len(result.checks) == 2

    def test_result_with_error_fails(self):
        """Result with failed error-level check fails."""
        result = MountPlanValidationResult()
        result.add(
            ValidationCheck(
                name="check1",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        result.add(
            ValidationCheck(
                name="check2",
                passed=False,
                message="Fail",
                severity="error",
            )
        )
        assert result.passed is False
        assert len(result.errors) == 1
        assert result.errors[0].name == "check2"

    def test_result_with_warning_still_passes(self):
        """Result with warning but no errors still passes."""
        result = MountPlanValidationResult()
        result.add(
            ValidationCheck(
                name="check1",
                passed=False,
                message="Warning",
                severity="warning",
            )
        )
        assert result.passed is True
        assert len(result.warnings) == 1

    def test_summary_format(self):
        """Summary includes pass/fail status and counts."""
        result = MountPlanValidationResult()
        result.add(
            ValidationCheck(
                name="pass",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        result.add(
            ValidationCheck(
                name="fail",
                passed=False,
                message="Fail",
                severity="error",
            )
        )
        summary = result.summary()
        assert "FAILED" in summary
        assert "1/2" in summary
        assert "1 errors" in summary

    def test_format_errors(self):
        """format_errors returns human-readable error list."""
        result = MountPlanValidationResult()
        result.add(
            ValidationCheck(
                name="missing_session",
                passed=False,
                message="Session section is missing",
                severity="error",
            )
        )
        formatted = result.format_errors()
        assert "Mount Plan Validation Failed" in formatted
        assert "missing_session" in formatted
        assert "Session section is missing" in formatted

    def test_format_errors_no_errors(self):
        """format_errors with no errors returns appropriate message."""
        result = MountPlanValidationResult()
        result.add(
            ValidationCheck(
                name="check1",
                passed=True,
                message="Pass",
                severity="info",
            )
        )
        formatted = result.format_errors()
        assert "No errors" in formatted


class TestMountPlanValidator:
    """Tests for MountPlanValidator validation logic."""

    def test_valid_minimal_mount_plan(self):
        """Minimal valid mount plan passes."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default"},
                "context": {"module": "context-default"},
            }
        }
        result = MountPlanValidator().validate(mount_plan)
        assert result.passed
        assert len(result.errors) == 0

    def test_valid_full_mount_plan(self):
        """Full mount plan with all sections passes."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default"},
                "context": {"module": "context-default"},
            },
            "providers": [{"module": "provider-anthropic", "config": {"model": "sonnet"}}],
            "tools": [{"module": "tool-web-search"}],
            "hooks": [],
        }
        result = MountPlanValidator().validate(mount_plan)
        assert result.passed

    def test_not_a_dict_fails(self):
        """Non-dict mount plan fails."""
        result = MountPlanValidator().validate("not a dict")
        assert not result.passed
        assert any("must be a dict" in e.message for e in result.errors)

    def test_none_fails(self):
        """None mount plan fails."""
        result = MountPlanValidator().validate(None)
        assert not result.passed
        assert any("must be a dict" in e.message for e in result.errors)

    def test_missing_session_fails(self):
        """Missing session section fails."""
        mount_plan = {"providers": []}
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("session" in e.message.lower() for e in result.errors)

    def test_missing_orchestrator_fails(self):
        """Missing orchestrator in session fails."""
        mount_plan = {"session": {"context": {"module": "context-default"}}}
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("orchestrator" in e.message.lower() for e in result.errors)

    def test_missing_context_fails(self):
        """Missing context in session fails."""
        mount_plan = {"session": {"orchestrator": {"module": "orchestrator-default"}}}
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("context" in e.message.lower() for e in result.errors)

    def test_malformed_module_spec_fails(self):
        """Module spec without 'module' field fails."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default"},
                "context": {"module": "context-default"},
            },
            "providers": [
                {"config": {"model": "gpt-4"}}  # Missing 'module'
            ],
        }
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("module" in e.message.lower() for e in result.errors)

    def test_module_spec_not_dict_fails(self):
        """Module spec that is not a dict fails."""
        mount_plan = {
            "session": {
                "orchestrator": "just-a-string",  # Should be dict
                "context": {"module": "context-default"},
            }
        }
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("must be a dict" in e.message for e in result.errors)

    def test_config_not_dict_fails(self):
        """Config that is not a dict fails."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default", "config": "string"},
                "context": {"module": "context-default"},
            }
        }
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("config" in e.message.lower() for e in result.errors)

    def test_source_not_string_fails(self):
        """Source that is not a string fails."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default", "source": 123},
                "context": {"module": "context-default"},
            }
        }
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("source" in e.message.lower() for e in result.errors)

    def test_empty_module_path_fails(self):
        """Empty module path fails."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": ""},
                "context": {"module": "context-default"},
            }
        }
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("empty" in e.message.lower() for e in result.errors)

    def test_module_path_wrong_type_fails(self):
        """Module path that is not a string fails."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": 123},
                "context": {"module": "context-default"},
            }
        }
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("string" in e.message.lower() for e in result.errors)

    def test_providers_not_list_fails(self):
        """Providers section that is not a list fails."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default"},
                "context": {"module": "context-default"},
            },
            "providers": {"module": "provider-anthropic"},  # Should be list
        }
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("must be a list" in e.message for e in result.errors)

    def test_empty_providers_list_ok(self):
        """Empty providers list is OK."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default"},
                "context": {"module": "context-default"},
            },
            "providers": [],
        }
        result = MountPlanValidator().validate(mount_plan)
        assert result.passed

    def test_unknown_sections_warning(self):
        """Unknown sections generate warning but don't fail."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default"},
                "context": {"module": "context-default"},
            },
            "unknown_section": {"foo": "bar"},
        }
        result = MountPlanValidator().validate(mount_plan)
        assert result.passed  # Should still pass
        assert len(result.warnings) >= 1
        assert any("unknown" in w.message.lower() for w in result.warnings)

    def test_session_not_dict_fails(self):
        """Session that is not a dict fails."""
        mount_plan = {"session": "not a dict"}
        result = MountPlanValidator().validate(mount_plan)
        assert not result.passed
        assert any("session" in e.message.lower() and "dict" in e.message.lower() for e in result.errors)

    def test_agents_section_not_validated_as_module_list(self):
        """Agents section is not validated as a module list (it's dict of configs)."""
        mount_plan = {
            "session": {
                "orchestrator": {"module": "orchestrator-default"},
                "context": {"module": "context-default"},
            },
            "agents": {"my-agent": {"providers": [{"module": "provider-mock"}]}},
        }
        result = MountPlanValidator().validate(mount_plan)
        assert result.passed  # Should not fail on agents being a dict

    def test_error_message_is_helpful(self):
        """Error messages explain what's wrong and show expected format."""
        mount_plan = {
            "session": {
                "orchestrator": {"config": {}},  # Missing 'module'
                "context": {"module": "context-default"},
            }
        }
        result = MountPlanValidator().validate(mount_plan)
        error_msg = result.format_errors()

        # Should explain the problem
        assert "missing" in error_msg.lower()
        assert "module" in error_msg.lower()

        # Should show expected format
        assert "expected" in error_msg.lower()
