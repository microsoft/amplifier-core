"""Tests for ToolResult auto-populate output from error message."""

from amplifier_core.models import ToolResult


class TestToolResultAutoPopulate:
    """Tests for ToolResult.model_post_init auto-populating output from error."""

    def test_toolresult_autopopulates_output_from_error_message(self) -> None:
        """When success=False and output is None, output is auto-populated from error message."""
        result = ToolResult(success=False, error={"message": "something broke"})
        assert result.output == "something broke"

    def test_toolresult_no_autopopulate_when_output_set(self) -> None:
        """When output is explicitly set, it is NOT overwritten by error message."""
        result = ToolResult(
            success=False, output="explicit", error={"message": "ignored"}
        )
        assert result.output == "explicit"

    def test_toolresult_no_autopopulate_on_success(self) -> None:
        """When success=True, output is not auto-populated even if error has a message."""
        result = ToolResult(success=True, error={"message": "irrelevant"})
        assert result.output is None

    def test_toolresult_no_autopopulate_without_message_key(self) -> None:
        """When error dict has no 'message' key, output stays None."""
        result = ToolResult(success=False, error={"detail": "no message key"})
        assert result.output is None
