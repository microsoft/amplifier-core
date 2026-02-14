"""Tests for amplifier_core.utils.retry module."""

from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from amplifier_core.llm_errors import (
    AccessDeniedError,
    AuthenticationError,
    ContentFilterError,
    ContextLengthError,
    InvalidRequestError,
    LLMError,
    NotFoundError,
    ProviderUnavailableError,
    RateLimitError,
)
from amplifier_core.utils.retry import (
    RetryConfig,
    classify_error_message,
    retry_with_backoff,
)


class TestRetryConfig:
    """Tests for RetryConfig defaults and construction."""

    def test_defaults(self) -> None:
        config = RetryConfig()
        assert config.max_retries == 3
        assert config.min_delay == 1.0
        assert config.max_delay == 60.0
        assert config.jitter == 0.2
        assert config.backoff_multiplier == 2.0
        assert config.honor_retry_after is True

    def test_custom_values(self) -> None:
        config = RetryConfig(
            max_retries=5,
            min_delay=0.5,
            max_delay=30.0,
            jitter=0.1,
            backoff_multiplier=3.0,
            honor_retry_after=False,
        )
        assert config.max_retries == 5
        assert config.min_delay == 0.5
        assert config.max_delay == 30.0
        assert config.jitter == 0.1
        assert config.backoff_multiplier == 3.0
        assert config.honor_retry_after is False

    def test_zero_retries(self) -> None:
        config = RetryConfig(max_retries=0)
        assert config.max_retries == 0


class TestRetryWithBackoff:
    """Tests for retry_with_backoff() async function."""

    @pytest.mark.asyncio
    async def test_succeeds_first_try(self) -> None:
        """No retry needed when operation succeeds."""
        operation = AsyncMock(return_value="success")
        result = await retry_with_backoff(operation)
        assert result == "success"
        assert operation.call_count == 1

    @pytest.mark.asyncio
    async def test_retries_on_retryable_error(self) -> None:
        """Retries on retryable LLMError, succeeds on attempt 2."""
        operation = AsyncMock(
            side_effect=[
                ProviderUnavailableError("down", retryable=True),
                "success",
            ]
        )
        config = RetryConfig(max_retries=3, min_delay=0.01, max_delay=0.1)
        result = await retry_with_backoff(operation, config)
        assert result == "success"
        assert operation.call_count == 2

    @pytest.mark.asyncio
    async def test_respects_max_retries(self) -> None:
        """Gives up after max_retries attempts."""
        error = ProviderUnavailableError("still down", retryable=True)
        operation = AsyncMock(side_effect=error)
        config = RetryConfig(max_retries=2, min_delay=0.01, max_delay=0.1)
        with pytest.raises(ProviderUnavailableError, match="still down"):
            await retry_with_backoff(operation, config)
        # 1 initial + 2 retries = 3 total calls
        assert operation.call_count == 3

    @pytest.mark.asyncio
    async def test_does_not_retry_non_retryable(self) -> None:
        """Non-retryable errors raise immediately without retry."""
        error = AuthenticationError("bad key")
        operation = AsyncMock(side_effect=error)
        config = RetryConfig(max_retries=3, min_delay=0.01)
        with pytest.raises(AuthenticationError, match="bad key"):
            await retry_with_backoff(operation, config)
        assert operation.call_count == 1

    @pytest.mark.asyncio
    async def test_does_not_retry_non_llm_error(self) -> None:
        """Non-LLMError exceptions pass through immediately."""
        operation = AsyncMock(side_effect=ValueError("not an LLM error"))
        config = RetryConfig(max_retries=3, min_delay=0.01)
        with pytest.raises(ValueError, match="not an LLM error"):
            await retry_with_backoff(operation, config)
        assert operation.call_count == 1

    @pytest.mark.asyncio
    async def test_respects_retry_after(self) -> None:
        """Uses RateLimitError.retry_after when available."""
        error = RateLimitError("too fast", retry_after=0.05, retryable=True)
        operation = AsyncMock(side_effect=[error, "ok"])
        config = RetryConfig(max_retries=3, min_delay=0.01, max_delay=1.0)
        result = await retry_with_backoff(operation, config)
        assert result == "ok"

    @pytest.mark.asyncio
    async def test_backoff_increases(self) -> None:
        """Delay increases exponentially between retries."""
        delays: list[float] = []

        async def on_retry(attempt: int, delay: float, error: LLMError) -> None:
            delays.append(delay)

        error = ProviderUnavailableError("down", retryable=True)
        operation = AsyncMock(side_effect=[error, error, error, "success"])
        config = RetryConfig(max_retries=3, min_delay=0.01, max_delay=10.0, jitter=0.0)
        result = await retry_with_backoff(operation, config, on_retry=on_retry)
        assert result == "success"
        # With jitter=0: delays should be 0.01, 0.02, 0.04
        assert len(delays) == 3
        assert delays[0] == pytest.approx(0.01)
        assert delays[1] == pytest.approx(0.02)
        assert delays[2] == pytest.approx(0.04)

    @pytest.mark.asyncio
    async def test_jitter_applied(self) -> None:
        """Delays vary when jitter > 0."""
        delays: list[float] = []

        async def on_retry(attempt: int, delay: float, error: LLMError) -> None:
            delays.append(delay)

        error = ProviderUnavailableError("down", retryable=True)
        operation = AsyncMock(side_effect=[error, error, "success"])
        config = RetryConfig(max_retries=3, min_delay=0.01, jitter=0.2)
        await retry_with_backoff(operation, config, on_retry=on_retry)
        # First delay should be around 0.01 +/- 20%
        assert 0.007 <= delays[0] <= 0.013

    @pytest.mark.asyncio
    async def test_on_retry_callback_called(self) -> None:
        """on_retry callback receives attempt number, delay, and error."""
        callback_args: list[tuple[int, float, LLMError]] = []

        async def on_retry(attempt: int, delay: float, error: LLMError) -> None:
            callback_args.append((attempt, delay, error))

        error = ProviderUnavailableError("down", retryable=True)
        operation = AsyncMock(side_effect=[error, "success"])
        config = RetryConfig(max_retries=3, min_delay=0.01)
        await retry_with_backoff(operation, config, on_retry=on_retry)
        assert len(callback_args) == 1
        attempt, delay, err = callback_args[0]
        assert attempt == 1
        assert delay > 0
        assert err is error

    @pytest.mark.asyncio
    async def test_raises_final_error_after_exhaustion(self) -> None:
        """After all retries exhausted, raises the last error."""
        errors = [
            ProviderUnavailableError("fail 1", retryable=True),
            ProviderUnavailableError("fail 2", retryable=True),
            ProviderUnavailableError("fail 3", retryable=True),
        ]
        operation = AsyncMock(side_effect=errors)
        config = RetryConfig(max_retries=2, min_delay=0.01)
        with pytest.raises(ProviderUnavailableError, match="fail 3"):
            await retry_with_backoff(operation, config)

    @pytest.mark.asyncio
    async def test_zero_max_retries_no_retry(self) -> None:
        """With max_retries=0, the operation is called once and errors propagate."""
        error = ProviderUnavailableError("down", retryable=True)
        operation = AsyncMock(side_effect=error)
        config = RetryConfig(max_retries=0, min_delay=0.01)
        with pytest.raises(ProviderUnavailableError):
            await retry_with_backoff(operation, config)
        assert operation.call_count == 1

    @pytest.mark.asyncio
    async def test_delay_capped_at_max_delay(self) -> None:
        """Delay never exceeds max_delay."""
        delays: list[float] = []

        async def on_retry(attempt: int, delay: float, error: LLMError) -> None:
            delays.append(delay)

        error = ProviderUnavailableError("down", retryable=True)
        operation = AsyncMock(
            side_effect=[error, error, error, error, error, "success"]
        )
        config = RetryConfig(max_retries=5, min_delay=0.01, max_delay=0.025, jitter=0.0)
        await retry_with_backoff(operation, config, on_retry=on_retry)
        # 0.01, 0.02, 0.025(capped), 0.025(capped), 0.025(capped)
        for d in delays:
            assert d <= 0.025

    @pytest.mark.asyncio
    async def test_default_config_when_none(self) -> None:
        """Uses default RetryConfig when config=None."""
        error = ProviderUnavailableError("down", retryable=True)
        operation = AsyncMock(side_effect=[error, "ok"])
        # Passing config=None should use defaults (works, doesn't crash)
        result = await retry_with_backoff(operation, None)
        assert result == "ok"


class TestClassifyErrorMessage:
    """Tests for classify_error_message() heuristic classifier."""

    def test_context_length_keywords(self) -> None:
        assert classify_error_message("context length exceeded") is ContextLengthError
        assert classify_error_message("too many tokens for model") is ContextLengthError
        assert classify_error_message("maximum context length") is ContextLengthError

    def test_rate_limit_keywords(self) -> None:
        assert classify_error_message("rate limit exceeded") is RateLimitError
        assert classify_error_message("too many requests") is RateLimitError

    def test_authentication_keywords(self) -> None:
        assert classify_error_message("authentication failed") is AuthenticationError
        assert classify_error_message("invalid api key") is AuthenticationError
        assert classify_error_message("unauthorized access") is AuthenticationError

    def test_not_found_keywords(self) -> None:
        assert classify_error_message("model not found") is NotFoundError
        assert classify_error_message("endpoint not found") is NotFoundError

    def test_content_filter_keywords(self) -> None:
        assert classify_error_message("content filter triggered") is ContentFilterError
        assert classify_error_message("blocked by safety filter") is ContentFilterError

    def test_unknown_message_returns_base(self) -> None:
        assert classify_error_message("something unknown happened") is LLMError

    def test_case_insensitive(self) -> None:
        assert classify_error_message("RATE LIMIT EXCEEDED") is RateLimitError
        assert classify_error_message("Context Length Exceeded") is ContextLengthError

    def test_status_code_overrides_message(self) -> None:
        """Status code takes priority when available."""
        # Message says "rate limit" but status is 404
        assert classify_error_message("rate limit", status_code=404) is NotFoundError
        assert (
            classify_error_message("something", status_code=401) is AuthenticationError
        )
        assert classify_error_message("something", status_code=403) is AccessDeniedError
        assert classify_error_message("something", status_code=429) is RateLimitError
        assert (
            classify_error_message("something", status_code=413) is ContextLengthError
        )

    def test_status_code_5xx(self) -> None:
        assert (
            classify_error_message("error", status_code=500) is ProviderUnavailableError
        )
        assert (
            classify_error_message("error", status_code=502) is ProviderUnavailableError
        )
        assert (
            classify_error_message("error", status_code=503) is ProviderUnavailableError
        )

    def test_status_code_400_falls_through_to_message(self) -> None:
        """400 is ambiguous -- fall through to message classification."""
        assert (
            classify_error_message("context length exceeded", status_code=400)
            is ContextLengthError
        )
        assert (
            classify_error_message("unknown error", status_code=400)
            is InvalidRequestError
        )
