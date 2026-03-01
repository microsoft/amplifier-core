//! Retry utilities for LLM provider operations.
//!
//! Provides:
//! - [`RetryConfig`]: Configuration for retry behavior with exponential backoff.
//! - [`classify_error_message`]: Heuristic error classifier for provider error strings.
//! - [`compute_delay`]: Pure delay computation for a given retry attempt.
//!
//! The actual async retry loop (`retry_with_backoff`) stays in Python where
//! `asyncio.sleep` is available. These Rust functions are called from Python
//! via PyO3 bindings.

use serde::{Deserialize, Serialize};

/// Configuration for retry behavior.
///
/// Follows exponential backoff with optional jitter. Respects
/// error-provided `retry_after` hints when `honor_retry_after` is true.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts. 0 means no retries (single attempt).
    pub max_retries: u32,
    /// Initial delay in seconds before the first retry.
    pub initial_delay: f64,
    /// Maximum delay between retries in seconds.
    pub max_delay: f64,
    /// Exponential backoff factor. Delay = initial_delay * backoff_factor^attempt.
    pub backoff_factor: f64,
    /// If true, apply random jitter (±50%) to the computed delay.
    pub jitter: bool,
    /// If true, use max(calculated_delay, retry_after) when the error provides a hint.
    pub honor_retry_after: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: 1.0,
            max_delay: 60.0,
            backoff_factor: 2.0,
            jitter: true,
            honor_retry_after: true,
        }
    }
}

/// Classify an error message string into an error category.
///
/// Returns one of: `"rate_limit"`, `"timeout"`, `"authentication"`,
/// `"context_length"`, `"content_filter"`, `"not_found"`,
/// `"provider_unavailable"`, or `"unknown"`.
pub fn classify_error_message(message: &str) -> &'static str {
    let lower = message.to_lowercase();

    // Order matters: more specific patterns first (matches Python impl).
    if lower.contains("context length")
        || lower.contains("too many tokens")
        || lower.contains("maximum context")
        || lower.contains("token limit")
        || lower.contains("too long")
    {
        "context_length"
    } else if lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("too many requests")
        || lower.contains("429")
    {
        "rate_limit"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "timeout"
    } else if lower.contains("authentication")
        || lower.contains("api key")
        || lower.contains("unauthorized")
        || lower.contains("401")
    {
        "authentication"
    } else if lower.contains("content filter")
        || lower.contains("safety")
        || lower.contains("blocked")
    {
        "content_filter"
    } else if lower.contains("not found") || lower.contains("404") {
        "not_found"
    } else if lower.contains("overloaded")
        || lower.contains("503")
        || lower.contains("502")
        || lower.contains("unavailable")
    {
        "provider_unavailable"
    } else {
        "unknown"
    }
}

/// Compute the delay for a given retry attempt.
///
/// This is a pure function (deterministic when `config.jitter` is false).
/// The caller is responsible for sleeping.
///
/// # Arguments
///
/// * `config` — Retry configuration.
/// * `attempt` — Zero-based attempt number (0 = first retry).
/// * `retry_after` — Optional server-provided retry-after hint in seconds.
/// * `delay_multiplier` — Error-specific multiplier (1.0 = no change).
pub fn compute_delay(
    config: &RetryConfig,
    attempt: u32,
    retry_after: Option<f64>,
    delay_multiplier: f64,
) -> f64 {
    // Exponential backoff: initial_delay * backoff_factor^attempt
    let mut delay = config.initial_delay * config.backoff_factor.powi(attempt as i32);

    // Cap at max_delay
    delay = delay.min(config.max_delay);

    // Apply delay_multiplier (from error, can exceed max_delay)
    if delay_multiplier != 1.0 {
        delay *= delay_multiplier;
    }

    // Respect retry_after (floor)
    if config.honor_retry_after {
        if let Some(ra) = retry_after {
            delay = delay.max(ra);
        }
    }

    // Add jitter: multiply by random factor in [0.5, 1.5)
    if config.jitter {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        delay *= rng.gen_range(0.5..1.5);
    }

    delay
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // RetryConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_retry_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert!((config.initial_delay - 1.0).abs() < f64::EPSILON);
        assert!((config.max_delay - 60.0).abs() < f64::EPSILON);
        assert!((config.backoff_factor - 2.0).abs() < f64::EPSILON);
        assert!(config.jitter);
        assert!(config.honor_retry_after);
    }

    // -----------------------------------------------------------------------
    // classify_error_message
    // -----------------------------------------------------------------------

    #[test]
    fn test_classify_rate_limit() {
        assert_eq!(classify_error_message("rate limit exceeded"), "rate_limit");
        assert_eq!(
            classify_error_message("Too Many Requests (429)"),
            "rate_limit"
        );
        assert_eq!(
            classify_error_message("rate_limit_exceeded"),
            "rate_limit"
        );
    }

    #[test]
    fn test_classify_timeout() {
        assert_eq!(classify_error_message("request timed out"), "timeout");
        assert_eq!(classify_error_message("Connection timeout"), "timeout");
    }

    #[test]
    fn test_classify_authentication() {
        assert_eq!(
            classify_error_message("invalid api key"),
            "authentication"
        );
        assert_eq!(
            classify_error_message("Authentication failed"),
            "authentication"
        );
        assert_eq!(
            classify_error_message("Unauthorized (401)"),
            "authentication"
        );
    }

    #[test]
    fn test_classify_context_length() {
        assert_eq!(
            classify_error_message("context length exceeded"),
            "context_length"
        );
        assert_eq!(
            classify_error_message("too many tokens"),
            "context_length"
        );
        assert_eq!(
            classify_error_message("maximum context reached"),
            "context_length"
        );
    }

    #[test]
    fn test_classify_content_filter() {
        assert_eq!(
            classify_error_message("content filter triggered"),
            "content_filter"
        );
        assert_eq!(
            classify_error_message("blocked by safety system"),
            "content_filter"
        );
    }

    #[test]
    fn test_classify_not_found() {
        assert_eq!(classify_error_message("model not found"), "not_found");
        assert_eq!(classify_error_message("error 404"), "not_found");
    }

    #[test]
    fn test_classify_provider_unavailable() {
        assert_eq!(
            classify_error_message("server overloaded"),
            "provider_unavailable"
        );
        assert_eq!(
            classify_error_message("503 service unavailable"),
            "provider_unavailable"
        );
        assert_eq!(
            classify_error_message("502 bad gateway"),
            "provider_unavailable"
        );
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(classify_error_message("something weird"), "unknown");
        assert_eq!(classify_error_message(""), "unknown");
    }

    // -----------------------------------------------------------------------
    // compute_delay
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_delay_basic() {
        // No jitter for deterministic testing
        let config = RetryConfig {
            jitter: false,
            ..RetryConfig::default()
        };

        // attempt 0: initial_delay * 2^0 = 1.0
        let d0 = compute_delay(&config, 0, None, 1.0);
        assert!((d0 - 1.0).abs() < f64::EPSILON);

        // attempt 1: initial_delay * 2^1 = 2.0
        let d1 = compute_delay(&config, 1, None, 1.0);
        assert!((d1 - 2.0).abs() < f64::EPSILON);

        // attempt 2: initial_delay * 2^2 = 4.0
        let d2 = compute_delay(&config, 2, None, 1.0);
        assert!((d2 - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_delay_respects_max() {
        let config = RetryConfig {
            max_delay: 10.0,
            jitter: false,
            ..RetryConfig::default()
        };

        // attempt 5: 1.0 * 2^5 = 32.0, but capped at 10.0
        let d = compute_delay(&config, 5, None, 1.0);
        assert!((d - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_delay_respects_retry_after() {
        let config = RetryConfig {
            jitter: false,
            ..RetryConfig::default()
        };

        // attempt 0: base delay = 1.0, retry_after = 5.0 → max(1.0, 5.0) = 5.0
        let d = compute_delay(&config, 0, Some(5.0), 1.0);
        assert!((d - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_delay_ignores_retry_after_when_disabled() {
        let config = RetryConfig {
            jitter: false,
            honor_retry_after: false,
            ..RetryConfig::default()
        };

        // retry_after should be ignored
        let d = compute_delay(&config, 0, Some(5.0), 1.0);
        assert!((d - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_delay_applies_multiplier() {
        let config = RetryConfig {
            jitter: false,
            ..RetryConfig::default()
        };

        // attempt 0: base = 1.0, multiplier = 3.0 → 3.0
        let d = compute_delay(&config, 0, None, 3.0);
        assert!((d - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_delay_multiplier_can_exceed_max() {
        let config = RetryConfig {
            max_delay: 10.0,
            jitter: false,
            ..RetryConfig::default()
        };

        // attempt 3: base = min(1.0 * 2^3, 10.0) = 8.0, multiplier = 5.0 → 40.0
        // multiplier is applied AFTER cap, so it can exceed max_delay
        let d = compute_delay(&config, 3, None, 5.0);
        assert!((d - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_delay_with_jitter_in_range() {
        let config = RetryConfig {
            jitter: true,
            ..RetryConfig::default()
        };

        // With jitter, delay should be in [0.5 * base, 1.5 * base]
        // attempt 0: base = 1.0, so jittered ∈ [0.5, 1.5]
        for _ in 0..100 {
            let d = compute_delay(&config, 0, None, 1.0);
            assert!(d >= 0.5, "delay {d} below 0.5");
            assert!(d <= 1.5, "delay {d} above 1.5");
        }
    }
}
