"""Tests for retry utility exports from amplifier_core."""


class TestRetryExports:
    """Verify retry utilities are importable from amplifier_core."""

    def test_import_from_top_level(self) -> None:
        import amplifier_core

        assert hasattr(amplifier_core, "RetryConfig")
        assert hasattr(amplifier_core, "retry_with_backoff")
        assert hasattr(amplifier_core, "classify_error_message")

    def test_import_from_utils(self) -> None:
        from amplifier_core.utils import (
            RetryConfig,
            classify_error_message,
            retry_with_backoff,
        )

        assert RetryConfig is not None
        assert retry_with_backoff is not None
        assert classify_error_message is not None

    def test_import_from_utils_retry(self) -> None:
        from amplifier_core.utils.retry import (
            RetryConfig,
            classify_error_message,
            retry_with_backoff,
        )

        assert RetryConfig is not None
        assert retry_with_backoff is not None
        assert classify_error_message is not None
