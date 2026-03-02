"""Tests for proto-based validation functions.

Validates that tool results and hook results conform to the structural
constraints defined by the proto schema.
"""

from amplifier_core.validation.proto_schema import (
    validate_hook_result,
    validate_tool_result,
)


class TestToolResultValidation:
    """Tests for validate_tool_result()."""

    def test_valid_tool_result(self):
        result = {"success": True, "output": "hello"}
        errors = validate_tool_result(result)
        assert errors == []

    def test_missing_success_field(self):
        result = {"output": "hello"}
        errors = validate_tool_result(result)
        assert len(errors) > 0
        assert any("success" in e for e in errors)

    def test_invalid_success_type(self):
        result = {"success": "yes", "output": "hello"}
        errors = validate_tool_result(result)
        assert len(errors) > 0


class TestHookResultValidation:
    """Tests for validate_hook_result()."""

    def test_valid_hook_result_continue(self):
        result = {"action": "continue"}
        errors = validate_hook_result(result)
        assert errors == []

    def test_valid_hook_result_deny(self):
        result = {"action": "deny", "reason": "not allowed"}
        errors = validate_hook_result(result)
        assert errors == []

    def test_invalid_action(self):
        result = {"action": "invalid_action"}
        errors = validate_hook_result(result)
        assert len(errors) > 0
        assert any("action" in e for e in errors)

    def test_missing_action_defaults_to_continue(self):
        result = {}
        errors = validate_hook_result(result)
        assert errors == []  # action defaults to "continue" per proto default

    def test_valid_hook_result_inject_context(self):
        result = {
            "action": "inject_context",
            "context_injection": "You are helpful.",
            "context_injection_role": "system",
        }
        errors = validate_hook_result(result)
        assert errors == []
