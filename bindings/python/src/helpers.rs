// ---------------------------------------------------------------------------
// Shared helper utilities
// ---------------------------------------------------------------------------

use pyo3::prelude::*;

/// Parse an approval system's decision string into a boolean.
///
/// Fail-CLOSED: only explicit allow-family strings return `true`.
/// Any unexpected, empty, or unknown string is treated as a denial.
/// This ensures that a misbehaving approval system cannot grant access
/// by returning a surprising value.
///
/// Accepted allow strings (case-insensitive): "allow", "allow once", "allow always".
pub(crate) fn is_approval_granted(decision: &str) -> bool {
    matches!(
        decision.to_lowercase().as_str(),
        "allow" | "allow once" | "allow always"
    )
}

/// Wrap a future_into_py result in a Python coroutine via _async_compat._wrap().
/// This makes PyO3 async methods return proper coroutines (not just awaitables),
/// ensuring compatibility with asyncio.create_task(), inspect.iscoroutine(), etc.
pub(crate) fn wrap_future_as_coroutine<'py>(
    py: Python<'py>,
    future: PyResult<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let future = future?;
    let wrapper = py
        .import("amplifier_core._async_compat")?
        .getattr("_wrap")?;
    wrapper.call1((&future,))
}

/// Try `model_dump()` on a Python object (Pydantic BaseModel → dict).
/// Falls back to the original object reference if not a Pydantic model.
pub(crate) fn try_model_dump<'py>(obj: &Bound<'py, PyAny>) -> Bound<'py, PyAny> {
    match obj.call_method0("model_dump") {
        Ok(dict) => dict,
        Err(_) => obj.clone(),
    }
}
