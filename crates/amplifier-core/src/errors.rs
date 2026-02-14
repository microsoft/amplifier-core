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
        retry_after: Option<f64>,
    },

    /// Invalid or missing API credentials (HTTP 401/403).
    #[error("{message}")]
    Authentication {
        message: String,
        provider: Option<String>,
    },

    /// Request exceeds the model's context window.
    #[error("{message}")]
    ContextLength {
        message: String,
        provider: Option<String>,
    },

    /// Content blocked by the provider's safety filter.
    #[error("{message}")]
    ContentFilter {
        message: String,
        provider: Option<String>,
    },

    /// Malformed request rejected by the provider (HTTP 400/422).
    #[error("{message}")]
    InvalidRequest {
        message: String,
        provider: Option<String>,
    },

    /// Provider service unavailable (HTTP 5xx, network error).
    /// Retryable by default.
    #[error("{message}")]
    Unavailable {
        message: String,
        provider: Option<String>,
        status_code: Option<u16>,
    },

    /// Request timed out before the provider responded.
    /// Retryable by default.
    #[error("{message}")]
    Timeout {
        message: String,
        provider: Option<String>,
    },

    /// Generic LLM error (maps to Python's base `LLMError`).
    #[error("{message}")]
    Other {
        message: String,
        provider: Option<String>,
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

    /// Seconds to wait before retrying, if available.
    ///
    /// Only `RateLimit` carries this field (parsed from the provider's
    /// `Retry-After` header).
    pub fn retry_after(&self) -> Option<f64> {
        match self {
            Self::RateLimit { retry_after, .. } => *retry_after,
            _ => None,
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
        };
        assert!(!err.retryable());
    }

    #[test]
    fn rate_limit_error_is_retryable() {
        let err = ProviderError::RateLimit {
            message: "429".into(),
            provider: Some("openai".into()),
            retry_after: Some(1.5),
        };
        assert!(err.retryable());
        assert_eq!(err.retry_after(), Some(1.5));
    }

    #[test]
    fn provider_unavailable_is_retryable() {
        let err = ProviderError::Unavailable {
            message: "503".into(),
            provider: None,
            status_code: Some(503),
        };
        assert!(err.retryable());
    }

    #[test]
    fn timeout_is_retryable() {
        let err = ProviderError::Timeout {
            message: "timed out".into(),
            provider: Some("gemini".into()),
        };
        assert!(err.retryable());
    }

    #[test]
    fn amplifier_error_wraps_provider_error() {
        let inner = ProviderError::RateLimit {
            message: "429".into(),
            provider: None,
            retry_after: None,
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
            retry_after: Some(2.0),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("429"));
    }
}
