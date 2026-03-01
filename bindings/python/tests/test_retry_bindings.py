"""Tests for retry utility PyO3 bindings."""

from amplifier_core._engine import RetryConfig, classify_error_message, compute_delay


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
    assert config.jitter is True
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
    assert config.jitter is False
    assert config.honor_retry_after is False


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
