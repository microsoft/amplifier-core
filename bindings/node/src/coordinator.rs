// ---------------------------------------------------------------------------
// JsCoordinator — wraps amplifier_core::Coordinator for Node.js
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use napi::bindgen_prelude::*;

use crate::cancellation::JsCancellationToken;
use crate::hooks::JsHookRegistry;

/// Wraps `amplifier_core::Coordinator` for Node.js — the central hub holding
/// module mount points, capabilities, hook registry, cancellation token, and config.
///
/// Implements the hybrid coordinator pattern: JS-side storage for TS module
/// objects, Rust kernel for everything else.
#[napi]
pub struct JsCoordinator {
    pub(crate) inner: Arc<amplifier_core::Coordinator>,
}

#[napi]
impl JsCoordinator {
    #[napi(constructor)]
    pub fn new(config_json: String) -> Result<Self> {
        let config: HashMap<String, serde_json::Value> =
            serde_json::from_str(&config_json).map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(amplifier_core::Coordinator::new(config)),
        })
    }

    #[napi(getter)]
    pub fn tool_names(&self) -> Vec<String> {
        self.inner.tool_names()
    }

    #[napi(getter)]
    pub fn provider_names(&self) -> Vec<String> {
        self.inner.provider_names()
    }

    #[napi(getter)]
    pub fn has_orchestrator(&self) -> bool {
        self.inner.has_orchestrator()
    }

    #[napi(getter)]
    pub fn has_context(&self) -> bool {
        self.inner.has_context()
    }

    #[napi]
    pub fn register_capability(&self, name: String, value_json: String) -> Result<()> {
        let value: serde_json::Value =
            serde_json::from_str(&value_json).map_err(|e| Error::from_reason(e.to_string()))?;
        self.inner.register_capability(&name, value);
        Ok(())
    }

    #[napi]
    pub fn get_capability(&self, name: String) -> Result<Option<String>> {
        match self.inner.get_capability(&name) {
            Some(v) => serde_json::to_string(&v)
                .map(Some)
                .map_err(|e| Error::from_reason(e.to_string())),
            None => Ok(None),
        }
    }

    /// The coordinator's hook registry — shared via `Arc`, not copied.
    ///
    /// Returns a `JsHookRegistry` wrapping the coordinator's real
    /// `Arc<HookRegistry>` obtained via `hooks_shared()`. Hooks registered
    /// on the returned instance are visible to the Coordinator and vice versa.
    #[napi(getter)]
    pub fn hooks(&self) -> JsHookRegistry {
        JsHookRegistry {
            inner: self.inner.hooks_shared(),
        }
    }

    #[napi(getter)]
    pub fn cancellation(&self) -> JsCancellationToken {
        JsCancellationToken::from_inner(self.inner.cancellation().clone())
    }

    #[napi(getter)]
    pub fn config(&self) -> Result<String> {
        serde_json::to_string(self.inner.config()).map_err(|e| Error::from_reason(e.to_string()))
    }

    #[napi]
    pub fn reset_turn(&self) {
        self.inner.reset_turn();
    }

    #[napi]
    pub fn to_dict(&self) -> HashMap<String, serde_json::Value> {
        self.inner.to_dict()
    }

    #[napi]
    pub async fn cleanup(&self) -> Result<()> {
        self.inner.cleanup().await;
        Ok(())
    }
}
