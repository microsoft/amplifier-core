// ---------------------------------------------------------------------------
// Error bridging — Rust errors → typed JS error objects
// ---------------------------------------------------------------------------

use napi::bindgen_prelude::*;

/// Structured error object returned to JS with a typed `code` property.
#[napi(object)]
pub struct JsAmplifierError {
    pub code: String,
    pub message: String,
}

/// Maps a lowercase variant name to its error code string.
///
/// Variant mapping:
/// - `"session"` → `"SessionError"`
/// - `"tool"` → `"ToolError"`
/// - `"provider"` → `"ProviderError"`
/// - `"hook"` → `"HookError"`
/// - `"context"` → `"ContextError"`
/// - anything else → `"AmplifierError"`
fn error_code_for_variant(variant: &str) -> &'static str {
    match variant {
        "session" => "SessionError",
        "tool" => "ToolError",
        "provider" => "ProviderError",
        "hook" => "HookError",
        "context" => "ContextError",
        _ => "AmplifierError",
    }
}

/// Converts an error variant name and message into a typed `JsAmplifierError`.
///
/// See [`error_code_for_variant`] for the variant → code mapping.
#[napi]
pub fn amplifier_error_to_js(variant: String, message: String) -> JsAmplifierError {
    let code = error_code_for_variant(&variant).to_string();
    JsAmplifierError { code, message }
}

/// Internal helper: converts an `AmplifierError` into a `napi::Error` with a
/// `[Code] message` format suitable for crossing the FFI boundary.
///
/// Uses [`error_code_for_variant`] for consistent code mapping.
#[allow(dead_code)] // Used when async methods expose Result<T, AmplifierError> across FFI
pub(crate) fn amplifier_error_to_napi(err: amplifier_core::errors::AmplifierError) -> napi::Error {
    let (variant, msg) = match &err {
        amplifier_core::errors::AmplifierError::Session(e) => ("session", e.to_string()),
        amplifier_core::errors::AmplifierError::Tool(e) => ("tool", e.to_string()),
        amplifier_core::errors::AmplifierError::Provider(e) => ("provider", e.to_string()),
        amplifier_core::errors::AmplifierError::Hook(e) => ("hook", e.to_string()),
        amplifier_core::errors::AmplifierError::Context(e) => ("context", e.to_string()),
    };
    let code = error_code_for_variant(variant);
    Error::from_reason(format!("[{code}] {msg}"))
}
