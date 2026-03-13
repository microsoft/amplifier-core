// ---------------------------------------------------------------------------
// JsHookHandlerBridge — lets JS functions act as Rust HookHandler trait objects
// JsHookRegistry — wraps amplifier_core::HookRegistry for Node.js
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction};

use amplifier_core::errors::HookError;
use amplifier_core::models as core_models;
use amplifier_core::models::HookResult;
use amplifier_core::traits::HookHandler;

use crate::hook_result::{hook_result_to_js, JsHookResult};

/// Bridges a JS callback function to the Rust `HookHandler` trait via
/// `ThreadsafeFunction`. The callback receives `(event: string, data: string)`
/// and returns a JSON string representing a `HookResult`.
struct JsHookHandlerBridge {
    callback: ThreadsafeFunction<(String, String), ErrorStrategy::Fatal>,
}

// Safety: ThreadsafeFunction is designed for cross-thread use in napi-rs.
unsafe impl Send for JsHookHandlerBridge {}
unsafe impl Sync for JsHookHandlerBridge {}

impl HookHandler for JsHookHandlerBridge {
    fn handle(
        &self,
        event: &str,
        data: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<HookResult, HookError>> + Send + '_>> {
        let event = event.to_string();
        let data_str = serde_json::to_string(&data).unwrap_or_else(|e| {
            eprintln!(
                "amplifier-core-node: failed to serialize hook data to JSON: {e}. Defaulting to empty object."
            );
            "{}".to_string()
        });
        Box::pin(async move {
            let result_str: String =
                self.callback
                    .call_async((event, data_str))
                    .await
                    .map_err(|e| HookError::HandlerFailed {
                        message: e.to_string(),
                        handler_name: None,
                    })?;
            let hook_result: HookResult = serde_json::from_str(&result_str).unwrap_or_else(|e| {
                log::error!(
                    "SECURITY: Hook handler returned unparseable result — failing closed (Deny): {e} — json: {result_str}"
                );
                HookResult {
                    action: core_models::HookAction::Deny,
                    reason: Some("Hook handler returned invalid response".to_string()),
                    ..Default::default()
                }
            });
            Ok(hook_result)
        })
    }
}

// ---------------------------------------------------------------------------
// JsHookRegistry — wraps amplifier_core::HookRegistry for Node.js
// ---------------------------------------------------------------------------

/// Wraps `amplifier_core::HookRegistry` for Node.js.
///
/// Provides register/emit/listHandlers/setDefaultFields — the event backbone
/// of the kernel.
#[napi]
pub struct JsHookRegistry {
    pub(crate) inner: Arc<amplifier_core::HookRegistry>,
}

impl Default for JsHookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl JsHookRegistry {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(amplifier_core::HookRegistry::new()),
        }
    }

    /// Register a hook handler for the given event name.
    ///
    /// ## Handler signature
    ///
    /// The `handler` callback receives two string arguments and must return a
    /// JSON-serialized `HookResult` (or a `Promise` that resolves to one):
    ///
    /// ```ts
    /// (event: string, dataJson: string) => string | Promise<string>
    /// ```
    ///
    /// Where the return value is a JSON string matching the `JsHookResult`
    /// shape, e.g. `'{"action":"Continue"}'`.  If the handler returns an
    /// invalid JSON string, the kernel fails closed and treats it as `Deny`.
    #[napi]
    pub fn register(
        &self,
        event: String,
        handler: JsFunction,
        priority: i32,
        name: String,
    ) -> Result<()> {
        let tsfn: ThreadsafeFunction<(String, String), ErrorStrategy::Fatal> = handler
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<(String, String)>| {
                let event_str = ctx.env.create_string(&ctx.value.0)?;
                let data_str = ctx.env.create_string(&ctx.value.1)?;
                Ok(vec![event_str.into_unknown(), data_str.into_unknown()])
            })?;

        let bridge = JsHookHandlerBridge { callback: tsfn };
        // HandlerId unused — unregister not yet exposed to JS
        let _ = self
            .inner
            .register(&event, Arc::new(bridge), priority, Some(name));
        Ok(())
    }

    #[napi]
    pub async fn emit(&self, event: String, data_json: String) -> Result<JsHookResult> {
        let data: serde_json::Value =
            serde_json::from_str(&data_json).map_err(|e| Error::from_reason(e.to_string()))?;
        let result = self.inner.emit(&event, data).await;
        Ok(hook_result_to_js(result))
    }

    #[napi]
    pub fn list_handlers(&self) -> HashMap<String, Vec<String>> {
        self.inner.list_handlers(None)
    }

    #[napi]
    pub fn set_default_fields(&self, defaults_json: String) -> Result<()> {
        let defaults: serde_json::Value =
            serde_json::from_str(&defaults_json).map_err(|e| Error::from_reason(e.to_string()))?;
        self.inner.set_default_fields(defaults);
        Ok(())
    }
}
