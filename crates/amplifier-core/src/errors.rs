//! Error types for the Amplifier kernel.
//!
//! This module defines the full error taxonomy:
//!
//! - [`AmplifierError`] — top-level enum wrapping all component errors
//! - [`ProviderError`] — maps to the Python `LLMError` hierarchy (8 variants)
//! - [`SessionError`] — session lifecycle errors
//! - [`HookError`] — hook dispatch errors
//! - [`ToolError`] — tool execution errors
//!
//! All types derive `Serialize` so errors can cross the JSON boundary
//! to the PyO3 bridge.

use serde::Serialize;

// -- ProviderError --

/// LLM provider error taxonomy.
///
/// Maps 1:1 to Python's `llm_errors.py` hierarchy:
///
/// | Python class              | Rust variant             |
/// |---------------------------|--------------------------|
/// | `LLMError`                | `ProviderError::Other`   |
/// | `RateLimitError`          | `ProviderError::RateLimit` |
/// | `AuthenticationError`     | `ProviderError::Authentication` |
/// | `ContextLengthError`      | `ProviderError::ContextLength` |
/// | `ContentFilterError`      | `ProviderError::ContentFilter` |
/// | `InvalidRequestError`     | `ProviderError::InvalidRequest` |
/// | `ProviderUnavailableError`| `ProviderError::Unavailable` |
/// | `LLMTimeoutError`         | `ProviderError::Timeout` |
#[derive(Debug, thiserror::Error, Serialize)]
pub enum ProviderError {
    /// Provider rate limit exceeded (HTTP 429 or equivalent).
    /// Retryable by default.
    #[error("{message}")]
    RateLimit {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
    },

    /// Invalid or missing API credentials (HTTP 401/403).
    #[error("{message}")]
    Authentication {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
    },

    /// Request exceeds the model's context window.
    #[error("{message}")]
    ContextLength {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
    },

    /// Content blocked by the provider's safety filter.
    #[error("{message}")]
    ContentFilter {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
    },

    /// Malformed request rejected by the provider (HTTP 400/422).
    #[error("{message}")]
    InvalidRequest {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
    },

    /// Provider service unavailable (HTTP 5xx, network error).
    /// Retryable by default.
    #[error("{message}")]
    Unavailable {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
        status_code: Option<u16>,
    },

    /// Request timed out before the provider responded.
    /// Retryable by default.
    #[error("{message}")]
    Timeout {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
    },

    /// Generic LLM error (maps to Python's base `LLMError`).
    #[error("{message}")]
    Other {
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: f64,
        status_code: Option<u16>,
        retryable: bool,
    },
}

impl ProviderError {
    /// Whether the caller should consider retrying the request.
    ///
    /// Matches Python defaults: `RateLimit`, `Unavailable`, and `Timeout`
    /// are retryable by default. `Other` carries an explicit flag.
    pub fn retryable(&self) -> bool {
        match self {
            Self::RateLimit { .. } => true,
            Self::Unavailable { .. } => true,
            Self::Timeout { .. } => true,
            Self::Other { retryable, .. } => *retryable,
            _ => false,
        }
    }

    /// Model identifier that caused the error (e.g., "claude-opus-4-6").
    pub fn model(&self) -> Option<&str> {
        match self {
            Self::RateLimit { model, .. }
            | Self::Authentication { model, .. }
            | Self::ContextLength { model, .. }
            | Self::ContentFilter { model, .. }
            | Self::InvalidRequest { model, .. }
            | Self::Unavailable { model, .. }
            | Self::Timeout { model, .. }
            | Self::Other { model, .. } => model.as_deref(),
        }
    }

    /// Seconds to wait before retrying, if available.
    pub fn retry_after(&self) -> Option<f64> {
        match self {
            Self::RateLimit { retry_after, .. }
            | Self::Authentication { retry_after, .. }
            | Self::ContextLength { retry_after, .. }
            | Self::ContentFilter { retry_after, .. }
            | Self::InvalidRequest { retry_after, .. }
            | Self::Unavailable { retry_after, .. }
            | Self::Timeout { retry_after, .. }
            | Self::Other { retry_after, .. } => *retry_after,
        }
    }

    /// Multiplier applied to backoff delay (default 1.0).
    pub fn delay_multiplier(&self) -> f64 {
        match self {
            Self::RateLimit {
                delay_multiplier, ..
            }
            | Self::Authentication {
                delay_multiplier, ..
            }
            | Self::ContextLength {
                delay_multiplier, ..
            }
            | Self::ContentFilter {
                delay_multiplier, ..
            }
            | Self::InvalidRequest {
                delay_multiplier, ..
            }
            | Self::Unavailable {
                delay_multiplier, ..
            }
            | Self::Timeout {
                delay_multiplier, ..
            }
            | Self::Other {
                delay_multiplier, ..
            } => *delay_multiplier,
        }
    }
}

// -- SessionError --

/// Session lifecycle errors.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum SessionError {
    /// Session has not been initialized yet.
    #[error("session not initialized")]
    NotInitialized,

    /// A required configuration field is missing.
    #[error("missing required config: {field}")]
    ConfigMissing { field: String },

    /// Session has already completed.
    #[error("session already completed")]
    AlreadyCompleted,

    /// Catch-all for other session errors.
    #[error("{message}")]
    Other { message: String },
}

// -- HookError --

/// Hook dispatch errors.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum HookError {
    /// A hook handler failed during dispatch.
    #[error("hook handler failed: {message}")]
    HandlerFailed {
        message: String,
        handler_name: Option<String>,
    },

    /// Hook dispatch timed out.
    #[error("hook dispatch timeout")]
    Timeout,

    /// Catch-all for other hook errors.
    #[error("{message}")]
    Other { message: String },
}

// -- ToolError --

/// Tool execution errors.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum ToolError {
    /// Tool execution failed.
    #[error("tool execution failed: {message}")]
    ExecutionFailed {
        message: String,
        stdout: Option<String>,
        stderr: Option<String>,
        exit_code: Option<i32>,
    },

    /// Requested tool was not found.
    #[error("tool not found: {name}")]
    NotFound { name: String },

    /// Catch-all for other tool errors.
    #[error("{message}")]
    Other { message: String },
}

// -- ContextError --

/// Context management errors.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum ContextError {
    /// Context compaction failed.
    #[error("context compaction failed: {message}")]
    CompactionFailed { message: String },

    /// Catch-all for other context errors.
    #[error("{message}")]
    Other { message: String },
}

// -- AmplifierError --

/// Top-level error enum wrapping all component errors.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum AmplifierError {
    /// An LLM provider error.
    #[error(transparent)]
    Provider(#[from] ProviderError),

    /// A session lifecycle error.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// A hook dispatch error.
    #[error(transparent)]
    Hook(#[from] HookError),

    /// A tool execution error.
    #[error(transparent)]
    Tool(#[from] ToolError),

    /// A context management error.
    #[error(transparent)]
    Context(#[from] ContextError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_error_default_not_retryable() {
        let err = ProviderError::Authentication {
            message: "bad key".into(),
            provider: Some("anthropic".into()),
            model: None,
            retry_after: None,
            delay_multiplier: 1.0,
        };
        assert!(!err.retryable());
    }

    #[test]
    fn rate_limit_error_is_retryable() {
        let err = ProviderError::RateLimit {
            message: "429".into(),
            provider: Some("openai".into()),
            model: None,
            retry_after: Some(1.5),
            delay_multiplier: 1.0,
        };
        assert!(err.retryable());
        assert_eq!(err.retry_after(), Some(1.5));
    }

    #[test]
    fn provider_unavailable_is_retryable() {
        let err = ProviderError::Unavailable {
            message: "503".into(),
            provider: None,
            model: None,
            retry_after: None,
            delay_multiplier: 1.0,
            status_code: Some(503),
        };
        assert!(err.retryable());
    }

    #[test]
    fn timeout_is_retryable() {
        let err = ProviderError::Timeout {
            message: "timed out".into(),
            provider: Some("gemini".into()),
            model: None,
            retry_after: None,
            delay_multiplier: 1.0,
        };
        assert!(err.retryable());
    }

    #[test]
    fn amplifier_error_wraps_provider_error() {
        let inner = ProviderError::RateLimit {
            message: "429".into(),
            provider: None,
            model: None,
            retry_after: None,
            delay_multiplier: 1.0,
        };
        let outer = AmplifierError::Provider(inner);
        assert!(matches!(outer, AmplifierError::Provider(_)));
    }

    #[test]
    fn session_error_display() {
        let err = SessionError::NotInitialized;
        assert_eq!(err.to_string(), "session not initialized");
    }

    #[test]
    fn errors_are_serializable() {
        let err = ProviderError::RateLimit {
            message: "429".into(),
            provider: Some("openai".into()),
            model: None,
            retry_after: Some(2.0),
            delay_multiplier: 1.0,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("429"));
    }

    // -- New field tests (Task 6) --

    #[test]
    fn test_provider_error_has_model_field() {
        // model defaults to None when not specified
        let err = ProviderError::Authentication {
            message: "bad key".into(),
            provider: Some("anthropic".into()),
            model: None,
            retry_after: None,
            delay_multiplier: 1.0,
        };
        assert_eq!(err.model(), None);
    }

    #[test]
    fn test_provider_error_has_retry_after_field() {
        // retry_after is now available on all variants, not just RateLimit
        let err = ProviderError::Timeout {
            message: "timed out".into(),
            provider: None,
            model: None,
            retry_after: None,
            delay_multiplier: 1.0,
        };
        assert_eq!(err.retry_after(), None);
    }

    #[test]
    fn test_provider_error_has_delay_multiplier_field() {
        // delay_multiplier defaults to 1.0
        let err = ProviderError::ContentFilter {
            message: "blocked".into(),
            provider: None,
            model: None,
            retry_after: None,
            delay_multiplier: 1.0,
        };
        assert!((err.delay_multiplier() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_provider_error_with_all_new_fields() {
        let err = ProviderError::RateLimit {
            message: "429".into(),
            provider: Some("openai".into()),
            model: Some("gpt-4".into()),
            retry_after: Some(2.5),
            delay_multiplier: 1.5,
        };
        assert_eq!(err.model(), Some("gpt-4"));
        assert_eq!(err.retry_after(), Some(2.5));
        assert!((err.delay_multiplier() - 1.5).abs() < f64::EPSILON);
    }
}
