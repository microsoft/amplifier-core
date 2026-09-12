"""Tests for ToolResult auto-populate output from error message."""

import pytest

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


class TestToolResultContent:
    """ToolResult rich content is normalized by the Rust ingress boundary."""

    def test_content_normalizes_and_preserves_legacy_dump_shape(self) -> None:
        legacy = ToolResult(output="plain")
        assert legacy.model_dump() == {
            "success": True,
            "output": "plain",
            "error": None,
        }

        result = ToolResult(
            content=[
                {"type": "text", "text": "details", "visibility": "user", "extra": True},
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": "AA==",
                        "extra": True,
                    },
                    "extra": True,
                },
            ]
        )
        assert result.model_dump() == {
            "success": True,
            "output": None,
            "error": None,
            "content": [
                {"type": "text", "text": "details"},
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": "AA==",
                    },
                },
            ],
        }
        assert ToolResult(content=[]).content is None

    @pytest.mark.parametrize(
        "content, expected",
        [
            ([{"type": "thinking", "thinking": "no"}], "invalid tool result content"),
            (
                [{"type": "image", "source": {"type": "url", "media_type": "image/png", "data": "AA=="}}],
                "invalid tool result image source",
            ),
            (
                [{"type": "image", "source": {"type": "base64", "media_type": "text/plain", "data": "AA=="}}],
                "invalid tool result image media type",
            ),
            (
                [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "not-base64"}}],
                "invalid tool result image data",
            ),
        ],
    )
    def test_content_rejects_malformed_input_without_echoing_it(self, content, expected) -> None:
        secret = "not-base64"
        with pytest.raises(ValueError) as exc_info:
            ToolResult(content=content)
        assert expected in str(exc_info.value)
        assert secret not in str(exc_info.value)

    def test_safe_hook_presentation_omits_image_data(self) -> None:
        result = ToolResult(
            output={"status": "ok"},
            content=[
                {"type": "text", "text": "details"},
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": "AA==",
                    },
                },
            ],
        )
        assert result.safe_hook_presentation() == {
            "success": True,
            "output": {"status": "ok"},
            "error": None,
            "content": [
                {"type": "text", "text": "details"},
                {
                    "type": "text",
                    "text": "[Image omitted from tool event; original retained for model request if unmodified.]",
                },
            ],
        }
