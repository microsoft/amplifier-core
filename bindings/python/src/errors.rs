// ---------------------------------------------------------------------------
// PyProviderError — exposes amplifier_core::errors::ProviderError fields
// ---------------------------------------------------------------------------

use pyo3::prelude::*;

/// Python-visible provider error with structured fields.
///
/// Exposes `model` and `retry_after` as Python-accessible properties,
/// matching the Python `LLMError` API. This class can be:
/// - Constructed directly from Python for testing or provider modules
/// - Created from a Rust `ProviderError` when errors cross the PyO3 boundary
#[pyclass(name = "ProviderError")]
pub(crate) struct PyProviderError {
    message: String,
    provider: Option<String>,
    model: Option<String>,
    retry_after: Option<f64>,
    delay_multiplier: Option<f64>,
    retryable: bool,
    error_type: String,
}

#[pymethods]
impl PyProviderError {
    /// Create a new ProviderError with structured fields.
    ///
    /// Matches the field set of both the Rust `ProviderError` enum and
    /// the Python `LLMError` base class (`model`, `retry_after`).
    #[new]
    #[pyo3(signature = (message, *, provider=None, model=None, retry_after=None, delay_multiplier=None, retryable=false, error_type="Other"))]
    fn new(
        message: String,
        provider: Option<String>,
        model: Option<String>,
        retry_after: Option<f64>,
        delay_multiplier: Option<f64>,
        retryable: bool,
        error_type: &str,
    ) -> Self {
        Self {
            message,
            provider,
            model,
            retry_after,
            delay_multiplier,
            retryable,
            error_type: error_type.to_string(),
        }
    }

    /// The error message string.
    #[getter]
    fn message(&self) -> &str {
        &self.message
    }

    /// Provider name (e.g. "anthropic", "openai"), or None.
    #[getter]
    fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// Model identifier that caused the error (e.g. "gpt-4"), or None.
    #[getter]
    fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Seconds to wait before retrying, or None if not specified.
    #[getter]
    fn retry_after(&self) -> Option<f64> {
        self.retry_after
    }

    /// Per-error delay multiplier hint, or None if not specified.
    #[getter]
    fn delay_multiplier(&self) -> Option<f64> {
        self.delay_multiplier
    }

    /// Whether the caller should consider retrying the request.
    #[getter]
    fn retryable(&self) -> bool {
        self.retryable
    }

    /// The error variant name (e.g. "RateLimit", "Authentication", "Other").
    #[getter]
    fn error_type(&self) -> &str {
        &self.error_type
    }

    fn __repr__(&self) -> String {
        let mut parts = vec![format!("{:?}", self.message)];
        if let Some(ref p) = self.provider {
            parts.push(format!("provider={p:?}"));
        }
        if let Some(ref m) = self.model {
            parts.push(format!("model={m:?}"));
        }
        if let Some(ra) = self.retry_after {
            parts.push(format!("retry_after={ra}"));
        }
        if let Some(dm) = self.delay_multiplier {
            parts.push(format!("delay_multiplier={dm}"));
        }
        if self.retryable {
            parts.push("retryable=True".to_string());
        }
        format!("ProviderError({})", parts.join(", "))
    }

    fn __str__(&self) -> &str {
        &self.message
    }
}

impl PyProviderError {
    /// Create from a Rust `ProviderError`, preserving all structured fields.
    #[allow(dead_code)]
    pub(crate) fn from_rust(err: &amplifier_core::errors::ProviderError) -> Self {
        use amplifier_core::errors::ProviderError;
        let (message, provider, model, retry_after, delay_multiplier, retryable, error_type) =
            match err {
                ProviderError::RateLimit {
                    message,
                    provider,
                    model,
                    retry_after,
                    delay_multiplier,
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    *delay_multiplier,
                    true,
                    "RateLimit",
                ),
                ProviderError::Authentication {
                    message,
                    provider,
                    model,
                    retry_after,
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    None,
                    false,
                    "Authentication",
                ),
                ProviderError::ContextLength {
                    message,
                    provider,
                    model,
                    retry_after,
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    None,
                    false,
                    "ContextLength",
                ),
                ProviderError::ContentFilter {
                    message,
                    provider,
                    model,
                    retry_after,
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    None,
                    false,
                    "ContentFilter",
                ),
                ProviderError::InvalidRequest {
                    message,
                    provider,
                    model,
                    retry_after,
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    None,
                    false,
                    "InvalidRequest",
                ),
                ProviderError::Unavailable {
                    message,
                    provider,
                    model,
                    retry_after,
                    delay_multiplier,
                    ..
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    *delay_multiplier,
                    true,
                    "Unavailable",
                ),
                ProviderError::Timeout {
                    message,
                    provider,
                    model,
                    retry_after,
                    delay_multiplier,
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    *delay_multiplier,
                    true,
                    "Timeout",
                ),
                ProviderError::Other {
                    message,
                    provider,
                    model,
                    retry_after,
                    retryable,
                    delay_multiplier,
                    ..
                } => (
                    message.clone(),
                    provider.clone(),
                    model.clone(),
                    *retry_after,
                    *delay_multiplier,
                    *retryable,
                    "Other",
                ),
            };
        Self {
            message,
            provider,
            model,
            retry_after,
            delay_multiplier,
            retryable,
            error_type: error_type.to_string(),
        }
    }
}
