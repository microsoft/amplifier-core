"""Tests for LLM provider error taxonomy."""

import pytest
from amplifier_core.llm_errors import (
    AuthenticationError,
    ContentFilterError,
    ContextLengthError,
    InvalidRequestError,
    LLMError,
    LLMTimeoutError,
    ProviderUnavailableError,
    RateLimitError,
)


class TestLLMErrorBase:
    """Tests for the LLMError base class."""

    def test_basic_creation(self) -> None:
        """LLMError can be created with just a message."""
        err = LLMError("Something went wrong")
        assert str(err) == "Something went wrong"
        assert err.provider is None
        assert err.status_code is None
        assert err.retryable is False

    def test_full_creation(self) -> None:
        """LLMError accepts all keyword arguments."""
        err = LLMError(
            "Provider error",
            provider="anthropic",
            status_code=500,
            retryable=True,
        )
        assert str(err) == "Provider error"
        assert err.provider == "anthropic"
        assert err.status_code == 500
        assert err.retryable is True

    def test_is_exception(self) -> None:
        """LLMError is a proper Exception subclass."""
        err = LLMError("test")
        assert isinstance(err, Exception)

    def test_all_subtypes_are_llm_errors(self) -> None:
        """All error subtypes can be caught as LLMError."""
        errors = [
            RateLimitError("rate limited"),
            AuthenticationError("bad key"),
            ContextLengthError("too long"),
            ContentFilterError("blocked"),
            InvalidRequestError("bad request"),
            ProviderUnavailableError("down"),
            LLMTimeoutError("timed out"),
        ]
        for err in errors:
            assert isinstance(err, LLMError), f"{type(err).__name__} is not an LLMError"
            assert isinstance(err, Exception)


class TestRateLimitError:
    """Tests for RateLimitError."""

    def test_retryable_by_default(self) -> None:
        """RateLimitError is retryable by default."""
        err = RateLimitError("Too many requests")
        assert err.retryable is True
        assert err.retry_after is None

    def test_retry_after(self) -> None:
        """RateLimitError accepts retry_after seconds."""
        err = RateLimitError("Too many requests", retry_after=30.0)
        assert err.retry_after == 30.0
        assert err.retryable is True

    def test_with_provider(self) -> None:
        """RateLimitError accepts provider info."""
        err = RateLimitError(
            "Rate limited",
            retry_after=5.0,
            provider="anthropic",
            status_code=429,
        )
        assert err.provider == "anthropic"
        assert err.status_code == 429
        assert err.retry_after == 5.0


class TestRetryableErrors:
    """Tests for errors that are retryable by default."""

    def test_provider_unavailable_is_retryable(self) -> None:
        """ProviderUnavailableError is retryable by default."""
        err = ProviderUnavailableError("Service down")
        assert err.retryable is True

    def test_timeout_is_retryable(self) -> None:
        """LLMTimeoutError is retryable by default."""
        err = LLMTimeoutError("Request timed out")
        assert err.retryable is True

    def test_rate_limit_retryable_override(self) -> None:
        """RateLimitError retryable default can be overridden."""
        err = RateLimitError("Rate limited", retryable=False)
        assert err.retryable is False

    def test_provider_unavailable_retryable_override(self) -> None:
        """ProviderUnavailableError retryable default can be overridden."""
        err = ProviderUnavailableError("Service down", retryable=False)
        assert err.retryable is False

    def test_timeout_retryable_override(self) -> None:
        """LLMTimeoutError retryable default can be overridden."""
        err = LLMTimeoutError("Timed out", retryable=False)
        assert err.retryable is False

    def test_provider_unavailable_with_status(self) -> None:
        """ProviderUnavailableError accepts provider and status_code."""
        err = ProviderUnavailableError(
            "Internal server error",
            provider="openai",
            status_code=500,
        )
        assert err.provider == "openai"
        assert err.status_code == 500
        assert err.retryable is True


class TestNonRetryableErrors:
    """Tests for errors that are NOT retryable by default."""

    def test_authentication_not_retryable(self) -> None:
        """AuthenticationError is not retryable."""
        err = AuthenticationError("Invalid API key")
        assert err.retryable is False

    def test_context_length_not_retryable(self) -> None:
        """ContextLengthError is not retryable."""
        err = ContextLengthError("Exceeds 200k context window")
        assert err.retryable is False

    def test_content_filter_not_retryable(self) -> None:
        """ContentFilterError is not retryable."""
        err = ContentFilterError("Content blocked by safety filter")
        assert err.retryable is False

    def test_invalid_request_not_retryable(self) -> None:
        """InvalidRequestError is not retryable."""
        err = InvalidRequestError("Malformed request")
        assert err.retryable is False


class TestRepr:
    """Tests for LLMError.__repr__ with structured info."""

    def test_basic_repr(self) -> None:
        """Basic error repr shows class name and message."""
        err = LLMError("Something broke")
        assert repr(err) == "LLMError('Something broke')"

    def test_repr_with_provider(self) -> None:
        """Repr includes provider when set."""
        err = LLMError("fail", provider="anthropic")
        assert "provider='anthropic'" in repr(err)

    def test_repr_with_all_fields(self) -> None:
        """Repr includes all structured fields."""
        err = RateLimitError("Too fast", provider="anthropic", status_code=429)
        r = repr(err)
        assert "RateLimitError(" in r
        assert "provider='anthropic'" in r
        assert "status_code=429" in r
        assert "retryable=True" in r

    def test_repr_omits_none_and_false(self) -> None:
        """Repr omits None fields and retryable=False."""
        err = AuthenticationError("Bad key")
        r = repr(err)
        assert "provider=" not in r
        assert "status_code=" not in r
        assert "retryable=" not in r


class TestExceptionChaining:
    """Tests for exception chaining (raise X from native_error)."""

    def test_chain_preserves_cause(self) -> None:
        """Chained exceptions preserve __cause__."""
        native = ValueError("native SDK error")
        try:
            raise RateLimitError("Rate limited", provider="anthropic") from native
        except RateLimitError as err:
            assert err.__cause__ is native
            assert isinstance(err.__cause__, ValueError)

    def test_catch_as_base_type(self) -> None:
        """Specific errors can be caught as LLMError."""
        with pytest.raises(LLMError):
            raise RateLimitError("Rate limited")

    def test_catch_as_exception(self) -> None:
        """LLM errors can be caught as generic Exception."""
        with pytest.raises(Exception):
            raise AuthenticationError("Bad key")

    def test_catch_specific_type(self) -> None:
        """Specific error types can be caught individually."""
        with pytest.raises(RateLimitError):
            raise RateLimitError("Rate limited")

        with pytest.raises(AuthenticationError):
            raise AuthenticationError("Bad key")

    def test_retryable_filter_pattern(self) -> None:
        """Common pattern: catch LLMError and check retryable."""
        errors: list[LLMError] = [
            RateLimitError("rate limited"),
            AuthenticationError("bad key"),
            ProviderUnavailableError("down"),
            ContextLengthError("too long"),
            LLMTimeoutError("timed out"),
        ]
        retryable = [e for e in errors if e.retryable]
        non_retryable = [e for e in errors if not e.retryable]

        assert len(retryable) == 3
        assert len(non_retryable) == 2
        assert all(
            isinstance(e, (RateLimitError, ProviderUnavailableError, LLMTimeoutError))
            for e in retryable
        )


class TestImportFromCore:
    """Tests that error types are importable from amplifier_core."""

    def test_import_from_top_level(self) -> None:
        """All error types are importable from amplifier_core."""
        import amplifier_core

        error_names = [
            "LLMError",
            "RateLimitError",
            "AuthenticationError",
            "ContextLengthError",
            "ContentFilterError",
            "InvalidRequestError",
            "ProviderUnavailableError",
            "LLMTimeoutError",
        ]
        for name in error_names:
            assert hasattr(amplifier_core, name), (
                f"{name} not exported from amplifier_core"
            )
            cls = getattr(amplifier_core, name)
            assert issubclass(cls, Exception), f"{name} is not an Exception subclass"
            assert issubclass(cls, LLMError), f"{name} is not an LLMError subclass"
