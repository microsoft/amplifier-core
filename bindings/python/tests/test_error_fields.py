"""Tests for ProviderError field access via PyO3.

Verifies that the Rust ProviderError exposes model, retry_after, and
delay_multiplier as Python-accessible properties on the _engine.ProviderError
class.
"""

from amplifier_core._engine import ProviderError


def test_provider_error_has_model_field():
    """ProviderError with model='test-model' exposes .model == 'test-model'."""
    err = ProviderError(
        message="test error",
        model="test-model",
    )
    assert err.model == "test-model"


def test_provider_error_has_retry_after_field():
    """ProviderError with retry_after=2.5 exposes .retry_after == 2.5."""
    err = ProviderError(
        message="rate limit exceeded",
        retry_after=2.5,
    )
    assert err.retry_after == 2.5


def test_provider_error_has_delay_multiplier_field():
    """ProviderError exposes .delay_multiplier, defaulting to 1.0."""
    err = ProviderError(message="test error")
    assert err.delay_multiplier == 1.0


def test_provider_error_delay_multiplier_custom():
    """ProviderError with delay_multiplier=2.0 exposes .delay_multiplier == 2.0."""
    err = ProviderError(
        message="test error",
        delay_multiplier=2.0,
    )
    assert err.delay_multiplier == 2.0


def test_provider_error_fields_default_to_none():
    """model and retry_after default to None when not set."""
    err = ProviderError(message="generic error")
    assert err.model is None
    assert err.retry_after is None
    assert err.delay_multiplier == 1.0


def test_provider_error_all_fields_set():
    """All three new fields can be set and read back together."""
    err = ProviderError(
        message="rate limit",
        model="gpt-4",
        retry_after=3.0,
        delay_multiplier=1.5,
    )
    assert err.model == "gpt-4"
    assert err.retry_after == 3.0
    assert err.delay_multiplier == 1.5


def test_provider_error_message_field():
    """ProviderError exposes .message for the error message string."""
    err = ProviderError(message="something went wrong")
    assert err.message == "something went wrong"


def test_provider_error_provider_field():
    """ProviderError exposes .provider for backward compat with LLMError."""
    err = ProviderError(message="error", provider="anthropic")
    assert err.provider == "anthropic"


def test_provider_error_provider_defaults_to_none():
    """provider defaults to None when not set."""
    err = ProviderError(message="error")
    assert err.provider is None


def test_provider_error_retryable_field():
    """ProviderError exposes .retryable, defaulting to False."""
    err = ProviderError(message="error")
    assert err.retryable is False

    err2 = ProviderError(message="rate limit", retryable=True)
    assert err2.retryable is True


def test_provider_error_error_type_field():
    """ProviderError exposes .error_type for the variant name."""
    err = ProviderError(message="429", error_type="RateLimit")
    assert err.error_type == "RateLimit"


def test_provider_error_error_type_defaults_to_other():
    """error_type defaults to 'Other' when not specified."""
    err = ProviderError(message="unknown")
    assert err.error_type == "Other"
