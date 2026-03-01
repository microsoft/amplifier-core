"""Tests for LLM provider error taxonomy."""

import pytest
from amplifier_core.llm_errors import (
    AbortError,
    AccessDeniedError,
    AuthenticationError,
    ConfigurationError,
    ContentFilterError,
    ContextLengthError,
    InvalidRequestError,
    InvalidToolCallError,
    LLMError,
    LLMTimeoutError,
    NetworkError,
    NotFoundError,
    ProviderUnavailableError,
    QuotaExceededError,
    RateLimitError,
    StreamError,
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


class TestNotFoundError:
    """Tests for NotFoundError."""

    def test_instantiation(self) -> None:
        err = NotFoundError("Model gpt-99 not found", provider="openai", status_code=404)
        assert str(err) == "Model gpt-99 not found"
        assert err.provider == "openai"
        assert err.status_code == 404

    def test_inherits_from_llm_error(self) -> None:
        err = NotFoundError("not found")
        assert isinstance(err, LLMError)
        assert isinstance(err, Exception)

    def test_not_retryable_by_default(self) -> None:
        err = NotFoundError("not found")
        assert err.retryable is False

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise NotFoundError("not found")


class TestStreamError:
    """Tests for StreamError."""

    def test_retryable_by_default(self) -> None:
        err = StreamError("Connection dropped mid-stream")
        assert err.retryable is True

    def test_inherits_from_llm_error(self) -> None:
        err = StreamError("stream broke")
        assert isinstance(err, LLMError)

    def test_retryable_override(self) -> None:
        err = StreamError("corrupt", retryable=False)
        assert err.retryable is False

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise StreamError("stream broke")


class TestAbortError:
    """Tests for AbortError."""

    def test_not_retryable_by_default(self) -> None:
        err = AbortError("User cancelled")
        assert err.retryable is False

    def test_inherits_from_llm_error(self) -> None:
        err = AbortError("cancelled")
        assert isinstance(err, LLMError)

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise AbortError("cancelled")


class TestInvalidToolCallError:
    """Tests for InvalidToolCallError."""

    def test_not_retryable_by_default(self) -> None:
        err = InvalidToolCallError("Bad JSON in arguments")
        assert err.retryable is False

    def test_tool_name_and_raw_arguments(self) -> None:
        err = InvalidToolCallError(
            "Failed to parse arguments",
            tool_name="read_file",
            raw_arguments='{"path": broken}',
        )
        assert err.tool_name == "read_file"
        assert err.raw_arguments == '{"path": broken}'

    def test_tool_name_defaults_to_none(self) -> None:
        err = InvalidToolCallError("bad call")
        assert err.tool_name is None
        assert err.raw_arguments is None

    def test_inherits_from_llm_error(self) -> None:
        err = InvalidToolCallError("bad call")
        assert isinstance(err, LLMError)

    def test_accepts_provider_and_status_code(self) -> None:
        err = InvalidToolCallError(
            "bad call",
            tool_name="foo",
            raw_arguments="bar",
            provider="anthropic",
            status_code=400,
        )
        assert err.provider == "anthropic"
        assert err.status_code == 400

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise InvalidToolCallError("bad")


class TestConfigurationError:
    """Tests for ConfigurationError."""

    def test_not_retryable_by_default(self) -> None:
        err = ConfigurationError("Missing API key")
        assert err.retryable is False

    def test_inherits_from_llm_error(self) -> None:
        err = ConfigurationError("bad config")
        assert isinstance(err, LLMError)

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise ConfigurationError("bad config")


class TestAccessDeniedError:
    """Tests for AccessDeniedError (subclass of AuthenticationError)."""

    def test_not_retryable_by_default(self) -> None:
        err = AccessDeniedError("Forbidden")
        assert err.retryable is False

    def test_inherits_from_authentication_error(self) -> None:
        err = AccessDeniedError("forbidden")
        assert isinstance(err, AuthenticationError)

    def test_inherits_from_llm_error(self) -> None:
        err = AccessDeniedError("forbidden")
        assert isinstance(err, LLMError)

    def test_caught_by_except_authentication_error(self) -> None:
        """Backward compat: existing `except AuthenticationError:` catches this."""
        with pytest.raises(AuthenticationError):
            raise AccessDeniedError("forbidden")

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise AccessDeniedError("forbidden")


class TestNetworkError:
    """Tests for NetworkError (subclass of ProviderUnavailableError)."""

    def test_retryable_by_default(self) -> None:
        """Inherits retryable=True from ProviderUnavailableError."""
        err = NetworkError("DNS resolution failed")
        assert err.retryable is True

    def test_inherits_from_provider_unavailable(self) -> None:
        err = NetworkError("connection refused")
        assert isinstance(err, ProviderUnavailableError)

    def test_inherits_from_llm_error(self) -> None:
        err = NetworkError("connection refused")
        assert isinstance(err, LLMError)

    def test_caught_by_except_provider_unavailable(self) -> None:
        """Backward compat: existing `except ProviderUnavailableError:` catches this."""
        with pytest.raises(ProviderUnavailableError):
            raise NetworkError("connection refused")

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise NetworkError("connection refused")


class TestQuotaExceededError:
    """Tests for QuotaExceededError (subclass of RateLimitError)."""

    def test_not_retryable_by_default(self) -> None:
        """Unlike parent RateLimitError (retryable=True), QuotaExceededError defaults to False."""
        err = QuotaExceededError("Monthly quota exhausted")
        assert err.retryable is False

    def test_inherits_from_rate_limit_error(self) -> None:
        err = QuotaExceededError("quota exceeded")
        assert isinstance(err, RateLimitError)

    def test_inherits_from_llm_error(self) -> None:
        err = QuotaExceededError("quota exceeded")
        assert isinstance(err, LLMError)

    def test_has_retry_after(self) -> None:
        """Inherits retry_after from RateLimitError."""
        err = QuotaExceededError("quota exceeded", retry_after=3600.0)
        assert err.retry_after == 3600.0

    def test_caught_by_except_rate_limit_error(self) -> None:
        """Backward compat: existing `except RateLimitError:` catches this."""
        with pytest.raises(RateLimitError):
            raise QuotaExceededError("quota exceeded")

    def test_caught_by_except_llm_error(self) -> None:
        with pytest.raises(LLMError):
            raise QuotaExceededError("quota exceeded")

    def test_retryable_can_be_overridden(self) -> None:
        err = QuotaExceededError("quota exceeded", retryable=True)
        assert err.retryable is True


class TestNewErrorsInAllSubtypesCheck:
    """Verify all 15 error types are caught by except LLMError."""

    def test_all_types_are_llm_errors(self) -> None:
        errors = [
            # Original 7
            RateLimitError("rate limited"),
            AuthenticationError("bad key"),
            ContextLengthError("too long"),
            ContentFilterError("blocked"),
            InvalidRequestError("bad request"),
            ProviderUnavailableError("down"),
            LLMTimeoutError("timed out"),
            # New 8
            NotFoundError("not found"),
            StreamError("stream broke"),
            AbortError("cancelled"),
            InvalidToolCallError("bad tool call"),
            ConfigurationError("bad config"),
            AccessDeniedError("forbidden"),
            NetworkError("connection refused"),
            QuotaExceededError("quota exceeded"),
        ]
        for err in errors:
            assert isinstance(err, LLMError), f"{type(err).__name__} is not an LLMError"
            assert isinstance(err, Exception)


class TestNewErrorsImportFromCore:
    """Verify all 8 new error types are importable from amplifier_core."""

    def test_import_new_types_from_top_level(self) -> None:
        import amplifier_core

        new_error_names = [
            "NotFoundError",
            "StreamError",
            "AbortError",
            "InvalidToolCallError",
            "ConfigurationError",
            "AccessDeniedError",
            "NetworkError",
            "QuotaExceededError",
        ]
        for name in new_error_names:
            assert hasattr(amplifier_core, name), (
                f"{name} not exported from amplifier_core"
            )
            cls = getattr(amplifier_core, name)
            assert issubclass(cls, Exception), f"{name} is not an Exception subclass"
            assert issubclass(cls, LLMError), f"{name} is not an LLMError subclass"
