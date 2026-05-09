// ---------------------------------------------------------------------------
// Shared helper utilities
// ---------------------------------------------------------------------------

use pyo3::prelude::*;
use pyo3::types::PyDict;

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
        Err(e) => {
            log::debug!("model_dump() failed (falling back to raw object): {e}");
            obj.clone()
        }
    }
}

/// Serialize a Python object to a JSON string with `default=str` fallback.
///
/// Like `json.dumps(obj)` but passes Python's built-in `str` as the `default=`
/// callable so non-JSON-native types (e.g. `decimal.Decimal`, `datetime`)
/// become their string representation instead of raising `TypeError`.
///
/// Use this everywhere we call `json.dumps()` at the Python/Rust FFI boundary.
pub(crate) fn json_dumps_safe<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<String> {
    let json_mod = py.import("json")?;
    let str_fn = py.import("builtins")?.getattr("str")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("default", &str_fn)?;
    json_mod
        .call_method("dumps", (obj,), Some(&kwargs))?
        .extract()
}
