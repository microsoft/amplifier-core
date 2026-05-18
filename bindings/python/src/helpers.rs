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

/// Try `model_dump(mode="json")` on a Python object (Pydantic BaseModel → JSON-safe dict).
///
/// Serialization strategy (three-tier):
///
/// 1. `model_dump(mode="json")` — preferred. Pydantic emits only JSON-native Python types
///    (str, int, float, list, dict, bool, None). Fields like `cost_usd: Decimal` that have a
///    `@field_serializer` convert correctly here. Any field type that Pydantic knows how to
///    JSON-encode is handled without extra effort.
///
/// 2. `model_dump()` — fallback for objects whose `model_dump()` does not accept a `mode`
///    kwarg (e.g. hand-rolled fakes, legacy Pydantic v1 models). The caller (`emit()`) then
///    calls `json.dumps()` on the result; if any field is still non-JSON-native, that will
///    surface as a `TypeError` rather than silently returning the raw object.
///
/// 3. Return the original object — if neither `model_dump` variant is callable. The caller
///    attempts `json.dumps()` on the raw object; if it is not serializable, a `TypeError`
///    propagates naturally.
pub(crate) fn try_model_dump<'py>(obj: &Bound<'py, PyAny>) -> Bound<'py, PyAny> {
    let py = obj.py();
    // Tier 1: model_dump(mode="json") — JSON-safe output from real Pydantic models.
    let kwargs = PyDict::new(py);
    if kwargs.set_item("mode", "json").is_ok() {
        if let Ok(dict) = obj.call_method("model_dump", (), Some(&kwargs)) {
            return dict;
        }
    }
    // Tier 2: bare model_dump() — for objects without a mode kwarg (fakes, Pydantic v1).
    if let Ok(dict) = obj.call_method0("model_dump") {
        return dict;
    }
    // Tier 3: raw object — let the caller's json.dumps() surface any TypeError.
    log::debug!("model_dump() not available — passing raw object to json serialiser");
    obj.clone()
}

/// Serialize a Python object to a JSON string with `default=str` fallback.
///
/// Like `json.dumps(obj)` but passes Python's built-in `str` as the `default=`
/// callable so non-JSON-native types (e.g. `decimal.Decimal`, `datetime`)
/// become their string representation instead of raising `TypeError`.
///
/// Use this everywhere we call `json.dumps()` at the Python/Rust FFI boundary.
pub(crate) fn json_dumps_safe<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<String> {
    let json_mod = py.import("json")?;
    let str_fn = py.import("builtins")?.getattr("str")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("default", &str_fn)?;
    json_mod
        .call_method("dumps", (obj,), Some(&kwargs))?
        .extract()
}
