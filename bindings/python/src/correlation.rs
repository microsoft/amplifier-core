// ---------------------------------------------------------------------------
// Correlation id stamping for LLM call events
// ---------------------------------------------------------------------------
//
// `llm:request` and `llm:response` carried no field in common that identified
// the call they belonged to, so every consumer paired them *positionally*.
// Any concurrently-issued call (a background summarizer, a session-naming
// hook, a forked sub-agent) silently mis-files the pairing: both events parse,
// the counts look right, and the cost attribution is wrong.
//
// The kernel closes that gap on the emit path, so providers need no change:
// `llm:request` gets a generated `request_id` and the matching terminal event
// echoes it. The *policy* -- which events carry an id, and how the in-flight
// call is scoped -- lives in `amplifier_core.correlation` (pure Python), which
// is authoritative. This module is the thin bridge that applies it to the
// serialized event payload.
//
// Scoping is by `contextvars`, so the id follows the async task that issued
// the call. Concurrent calls run in separate tasks and hold separate slots --
// which is precisely the case positional pairing gets wrong.

use pyo3::prelude::*;
use serde_json::Value;

/// Event-data field carrying the correlation id.
///
/// Mirrors `amplifier_core.correlation.REQUEST_ID_FIELD`; the two are pinned
/// together by `tests/test_hooks_request_id.py`.
pub(crate) const REQUEST_ID_FIELD: &str = "request_id";

/// Cheap pre-filter: only the `llm:` and `provider:` families can carry a
/// correlation id, so every other event skips the Python round-trip entirely.
///
/// This is deliberately *broader* than the real set -- `resolve_request_id`
/// in `amplifier_core.correlation` remains the single authority on which
/// events actually get stamped, so the two cannot drift into disagreement.
fn maybe_correlated(event: &str) -> bool {
    event.starts_with("llm:") || event.starts_with("provider:")
}

/// Stamp the correlation id onto `data` for correlated events.
///
/// Returns `data` unchanged for every other event, for non-object payloads,
/// and whenever the policy declines to supply an id (e.g. a response with no
/// matching request in this context -- an absent id is always preferable to a
/// wrong one).
///
/// An explicit `request_id` already present in `data` always wins: the policy
/// adopts it as the id of the call in flight and the value is left untouched.
pub(crate) fn stamp_request_id(py: Python<'_>, event: &str, mut data: Value) -> PyResult<Value> {
    if !maybe_correlated(event) {
        return Ok(data);
    }

    let Value::Object(ref mut map) = data else {
        return Ok(data);
    };

    let explicit: Option<String> = map
        .get(REQUEST_ID_FIELD)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let resolved: Option<String> = py
        .import("amplifier_core.correlation")?
        .getattr("resolve_request_id")?
        .call1((event, explicit.clone()))?
        .extract()?;

    if let Some(request_id) = resolved {
        if explicit.as_deref() != Some(request_id.as_str()) {
            map.insert(REQUEST_ID_FIELD.to_string(), Value::String(request_id));
        }
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefilter_admits_llm_and_provider_families() {
        assert!(maybe_correlated("llm:request"));
        assert!(maybe_correlated("llm:response"));
        assert!(maybe_correlated("provider:error"));
        assert!(maybe_correlated("provider:retry"));
    }

    #[test]
    fn prefilter_rejects_unrelated_events() {
        assert!(!maybe_correlated("tool:pre"));
        assert!(!maybe_correlated("session:start"));
        assert!(!maybe_correlated("content_block:delta"));
    }
}
