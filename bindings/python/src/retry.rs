// ---------------------------------------------------------------------------
// PyRetryConfig — wraps amplifier_core::retry::RetryConfig
// Retry utility functions: classify_error_message, compute_delay
// ---------------------------------------------------------------------------

use pyo3::prelude::*;

// ---------------------------------------------------------------------------
// PyRetryConfig — wraps amplifier_core::retry::RetryConfig
// ---------------------------------------------------------------------------

/// Python-visible retry configuration wrapper.
///
/// Exposes all fields of the Rust `RetryConfig` as read-only properties,
/// with sensible defaults matching the Rust `Default` impl.
#[pyclass(name = "RetryConfig", skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyRetryConfig {
    inner: amplifier_core::retry::RetryConfig,
}

#[pymethods]
impl PyRetryConfig {
    #[new]
    #[pyo3(signature = (
        max_retries = 3,
        initial_delay = 1.0,
        max_delay = 60.0,
        backoff_factor = 2.0,
        jitter = None,
        honor_retry_after = true,
        min_delay = None,
        backoff_multiplier = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        max_retries: u32,
        initial_delay: f64,
        max_delay: f64,
        backoff_factor: f64,
        jitter: Option<&Bound<'_, PyAny>>,
        honor_retry_after: bool,
        min_delay: Option<f64>,
        backoff_multiplier: Option<f64>,
    ) -> PyResult<Self> {
        // --- deprecated alias: min_delay → initial_delay ---
        // Only applies when initial_delay is still at its default (1.0); explicit new-style wins.
        let initial_delay = if let Some(md) = min_delay {
            if initial_delay == 1.0 {
                PyErr::warn(
                    py,
                    &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                    c"min_delay is deprecated, use initial_delay",
                    1,
                )?;
                md
            } else {
                initial_delay
            }
        } else {
            initial_delay
        };

        // --- deprecated alias: backoff_multiplier → backoff_factor ---
        // Only applies when backoff_factor is still at its default (2.0); explicit new-style wins.
        let backoff_factor = if let Some(bm) = backoff_multiplier {
            if backoff_factor == 2.0 {
                PyErr::warn(
                    py,
                    &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                    c"backoff_multiplier is deprecated, use backoff_factor",
                    1,
                )?;
                bm
            } else {
                backoff_factor
            }
        } else {
            backoff_factor
        };

        // --- jitter: accept bool or float (float is deprecated) ---
        let jitter_bool = match jitter {
            None => true, // default
            Some(obj) => {
                if let Ok(b) = obj.extract::<bool>() {
                    b
                } else if let Ok(f) = obj.extract::<f64>() {
                    PyErr::warn(
                        py,
                        &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                        c"passing a float for jitter is deprecated, use a bool",
                        1,
                    )?;
                    f != 0.0
                } else {
                    // Invalid type: fall back to default (True) with a warning rather than erroring.
                    PyErr::warn(
                        py,
                        &py.get_type::<pyo3::exceptions::PyUserWarning>(),
                        c"jitter received an unexpected type; defaulting to True",
                        1,
                    )?;
                    true
                }
            }
        };

        Ok(Self {
            inner: amplifier_core::retry::RetryConfig {
                max_retries,
                initial_delay,
                max_delay,
                backoff_factor,
                jitter: jitter_bool,
                honor_retry_after,
            },
        })
    }

    #[getter]
    fn max_retries(&self) -> u32 {
        self.inner.max_retries
    }

    #[getter]
    fn initial_delay(&self) -> f64 {
        self.inner.initial_delay
    }

    #[getter]
    fn max_delay(&self) -> f64 {
        self.inner.max_delay
    }

    #[getter]
    fn backoff_factor(&self) -> f64 {
        self.inner.backoff_factor
    }

    /// Returns 0.2 if jitter is enabled, 0.0 if disabled (numeric compat).
    #[getter]
    fn jitter(&self) -> f64 {
        if self.inner.jitter {
            0.2
        } else {
            0.0
        }
    }

    #[getter]
    fn honor_retry_after(&self) -> bool {
        self.inner.honor_retry_after
    }

    /// Deprecated: use `initial_delay` instead.
    #[getter]
    fn min_delay(&self, py: Python<'_>) -> PyResult<f64> {
        PyErr::warn(
            py,
            &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
            c"min_delay is deprecated, use initial_delay",
            1,
        )?;
        Ok(self.inner.initial_delay)
    }

    /// Deprecated: use `backoff_factor` instead.
    #[getter]
    fn backoff_multiplier(&self, py: Python<'_>) -> PyResult<f64> {
        PyErr::warn(
            py,
            &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
            c"backoff_multiplier is deprecated, use backoff_factor",
            1,
        )?;
        Ok(self.inner.backoff_factor)
    }
}

// ---------------------------------------------------------------------------
// Retry utility functions
// ---------------------------------------------------------------------------

/// Classify an error message string into an error category.
///
/// Returns one of: "rate_limit", "timeout", "authentication",
/// "context_length", "content_filter", "not_found",
/// "provider_unavailable", or "unknown".
#[pyfunction]
pub(crate) fn classify_error_message(message: &str) -> &'static str {
    amplifier_core::retry::classify_error_message(message)
}

/// Compute the delay for a given retry attempt.
///
/// Pure function (deterministic when `config.jitter` is false).
/// The caller is responsible for sleeping.
#[pyfunction]
#[pyo3(signature = (config, attempt, retry_after=None, delay_multiplier=None))]
pub(crate) fn compute_delay(
    config: &PyRetryConfig,
    attempt: u32,
    retry_after: Option<f64>,
    delay_multiplier: Option<f64>,
) -> f64 {
    amplifier_core::retry::compute_delay(&config.inner, attempt, retry_after, delay_multiplier)
}
