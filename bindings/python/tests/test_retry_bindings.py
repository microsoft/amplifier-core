"""Tests for retry utility PyO3 bindings."""

import warnings

from amplifier_core._engine import (
    ProviderError,
    RetryConfig,
    classify_error_message,
    compute_delay,
)


# ---------------------------------------------------------------------------
# RetryConfig construction
# ---------------------------------------------------------------------------


def test_retry_config_defaults():
    """RetryConfig() with no args should use default values."""
    config = RetryConfig()
    assert config.max_retries == 3
    assert config.initial_delay == 1.0
    assert config.max_delay == 60.0
    assert config.backoff_factor == 2.0
    assert config.jitter == 0.2
    assert config.honor_retry_after is True


def test_retry_config_custom():
    """RetryConfig with custom values should store them all."""
    config = RetryConfig(
        max_retries=5,
        initial_delay=0.5,
        max_delay=30.0,
        backoff_factor=3.0,
        jitter=False,
        honor_retry_after=False,
    )
    assert config.max_retries == 5
    assert config.initial_delay == 0.5
    assert config.max_delay == 30.0
    assert config.backoff_factor == 3.0
    assert config.jitter == 0.0
    assert config.honor_retry_after is False


# ---------------------------------------------------------------------------
# RetryConfig backward compat aliases
# ---------------------------------------------------------------------------


def test_retry_config_min_delay_alias():
    """min_delay should map to initial_delay with a deprecation warning."""
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        config = RetryConfig(min_delay=2.0)
        assert config.initial_delay == 2.0
        assert len(w) == 1
        assert issubclass(w[0].category, DeprecationWarning)
        assert "min_delay" in str(w[0].message)


def test_retry_config_backoff_multiplier_alias():
    """backoff_multiplier should map to backoff_factor with a deprecation warning."""
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        config = RetryConfig(backoff_multiplier=4.0)
        assert config.backoff_factor == 4.0
        assert len(w) == 1
        assert issubclass(w[0].category, DeprecationWarning)
        assert "backoff_multiplier" in str(w[0].message)


def test_retry_config_jitter_float_coerced_to_bool():
    """Passing a float jitter should emit a deprecation warning and coerce to bool."""
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        config = RetryConfig(jitter=0.5)
        # Any non-zero float -> True -> getter returns 0.2
        assert config.jitter == 0.2
        assert len(w) == 1
        assert issubclass(w[0].category, DeprecationWarning)
        assert "jitter" in str(w[0].message)


def test_retry_config_deprecated_getter_min_delay():
    """Accessing .min_delay should return initial_delay with a deprecation warning."""
    config = RetryConfig(initial_delay=3.0)
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        val = config.min_delay
        assert val == 3.0
        assert len(w) == 1
        assert issubclass(w[0].category, DeprecationWarning)
        assert "min_delay" in str(w[0].message)


def test_retry_config_deprecated_getter_backoff_multiplier():
    """Accessing .backoff_multiplier should return backoff_factor with a deprecation warning."""
    config = RetryConfig(backoff_factor=5.0)
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        val = config.backoff_multiplier
        assert val == 5.0
        assert len(w) == 1
        assert issubclass(w[0].category, DeprecationWarning)
        assert "backoff_multiplier" in str(w[0].message)


# ---------------------------------------------------------------------------
# classify_error_message
# ---------------------------------------------------------------------------


def test_classify_error_message_rate_limit():
    """Rate limit messages should classify as 'rate_limit'."""
    assert classify_error_message("rate limit exceeded") == "rate_limit"


def test_classify_error_message_timeout():
    """Timeout messages should classify as 'timeout'."""
    assert classify_error_message("request timed out") == "timeout"


def test_classify_error_message_unknown():
    """Unrecognized messages should classify as 'unknown'."""
    assert classify_error_message("something unexpected") == "unknown"


# ---------------------------------------------------------------------------
# compute_delay
# ---------------------------------------------------------------------------


def test_compute_delay_basic():
    """Exponential backoff: delay doubles each attempt (no jitter)."""
    config = RetryConfig(jitter=False)
    # attempt 0: 1.0 * 2^0 = 1.0
    assert compute_delay(config, 0) == 1.0
    # attempt 1: 1.0 * 2^1 = 2.0
    assert compute_delay(config, 1) == 2.0
    # attempt 2: 1.0 * 2^2 = 4.0
    assert compute_delay(config, 2) == 4.0


def test_compute_delay_with_retry_after():
    """retry_after should act as a floor for the computed delay."""
    config = RetryConfig(jitter=False)
    # attempt 0: base = 1.0, retry_after = 5.0 -> max(1.0, 5.0) = 5.0
    delay = compute_delay(config, 0, retry_after=5.0)
    assert delay == 5.0


def test_compute_delay_with_delay_multiplier():
    """delay_multiplier should scale the computed delay."""
    config = RetryConfig(jitter=False)
    # attempt 0: base = 1.0, multiplier = 10.0 -> 10.0
    delay = compute_delay(config, 0, delay_multiplier=10.0)
    assert delay == 10.0


# ---------------------------------------------------------------------------
# ProviderError.delay_multiplier
# ---------------------------------------------------------------------------


def test_provider_error_delay_multiplier_default():
    """ProviderError with no delay_multiplier should default to None."""
    err = ProviderError(message="error")
    assert err.delay_multiplier is None


def test_provider_error_delay_multiplier_set():
    """ProviderError with delay_multiplier should expose it."""
    err = ProviderError(message="overloaded", delay_multiplier=10.0, retryable=True)
    assert err.delay_multiplier == 10.0


def test_provider_error_delay_multiplier_in_repr():
    """ProviderError repr should include delay_multiplier when set."""
    err = ProviderError(message="error", delay_multiplier=5.0)
    assert "delay_multiplier=5" in repr(err)
