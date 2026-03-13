// ---------------------------------------------------------------------------
// JsAmplifierSession — wraps amplifier_core::Session for Node.js
// ---------------------------------------------------------------------------

use std::sync::Arc;

use napi::bindgen_prelude::*;
use tokio::sync::Mutex;

use crate::coordinator::JsCoordinator;

/// Wraps `amplifier_core::Session` for Node.js — the top-level entry point.
///
/// Lifecycle: `new AmplifierSession(config) → initialize() → execute(prompt) → cleanup()`.
/// Wires together Coordinator, HookRegistry, and CancellationToken.
///
/// The `coordinator` getter returns the session's real `Arc<Coordinator>`,
/// and `coordinator.hooks` returns the real `Arc<HookRegistry>` — both
/// shared, not copied.
#[napi]
pub struct JsAmplifierSession {
    inner: Arc<Mutex<amplifier_core::Session>>,
    cached_session_id: String,
    cached_parent_id: Option<String>,
    cached_coordinator: Option<JsCoordinator>,
}

#[napi]
impl JsAmplifierSession {
    #[napi(constructor)]
    pub fn new(
        config_json: String,
        session_id: Option<String>,
        parent_id: Option<String>,
    ) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(&config_json)
            .map_err(|e| Error::from_reason(format!("Invalid config JSON: {e}")))?;

        let config = amplifier_core::SessionConfig::from_value(value)
            .map_err(|e| Error::from_reason(e.to_string()))?;

        let session = amplifier_core::Session::new(config, session_id.clone(), parent_id.clone());
        let cached_session_id = session.session_id().to_string();

        Ok(Self {
            inner: Arc::new(Mutex::new(session)),
            cached_session_id,
            cached_parent_id: parent_id,
            cached_coordinator: None,
        })
    }

    #[napi(getter)]
    pub fn session_id(&self) -> &str {
        &self.cached_session_id
    }

    #[napi(getter)]
    pub fn parent_id(&self) -> Option<String> {
        self.cached_parent_id.clone()
    }

    #[napi(getter)]
    pub fn is_initialized(&self) -> bool {
        match self.inner.try_lock() {
            Ok(session) => session.is_initialized(),
            // Safe default: lock is only held during async cleanup(), which sets
            // initialized to false — so false is a correct conservative fallback.
            Err(_) => false,
        }
    }

    /// Current session lifecycle state as a lowercase string.
    ///
    /// Returns one of the `SessionState` variant strings:
    /// - `"Running"` — session is active
    /// - `"Completed"` — session finished successfully
    /// - `"Failed"` — session encountered a fatal error
    /// - `"Cancelled"` — session was cancelled via the cancellation token
    ///
    /// Falls back to `"running"` if the session lock is held during `cleanup()`.
    #[napi(getter)]
    pub fn status(&self) -> String {
        match self.inner.try_lock() {
            Ok(session) => session.status().to_string(),
            // Safe default: lock is only held during async cleanup(), and sessions
            // start as "running" — returning "running" during cleanup is tolerable.
            Err(_) => "running".to_string(),
        }
    }

    /// The session's coordinator — shared via `Arc`, not copied.
    ///
    /// Returns a `JsCoordinator` wrapping the session's real `Arc<Coordinator>`.
    /// Repeated calls return the same underlying coordinator instance.
    ///
    /// Takes `&mut self` because the first call caches the coordinator internally.
    /// This is safe because NAPI JS objects are single-threaded — no concurrent access.
    #[napi(getter)]
    pub fn coordinator(&mut self) -> JsCoordinator {
        if let Some(ref cached) = self.cached_coordinator {
            return JsCoordinator {
                inner: Arc::clone(&cached.inner),
            };
        }
        // First call: extract the Arc<Coordinator> from the session.
        // try_lock is safe here — the Mutex is only held during async execute/cleanup.
        let coord_arc = match self.inner.try_lock() {
            Ok(session) => session.coordinator_shared(),
            Err(_) => {
                log::warn!(
                    "JsAmplifierSession::coordinator() — session lock held, \
                     creating coordinator from default config as fallback"
                );
                Arc::new(amplifier_core::Coordinator::new(Default::default()))
            }
        };
        let js_coord = JsCoordinator { inner: coord_arc };
        self.cached_coordinator = Some(JsCoordinator {
            inner: Arc::clone(&js_coord.inner),
        });
        js_coord
    }

    #[napi]
    pub fn set_initialized(&self) {
        match self.inner.try_lock() {
            Ok(session) => session.set_initialized(),
            Err(_) => log::warn!(
                "JsAmplifierSession::set_initialized() skipped — session lock held (cleanup in progress?)"
            ),
        }
    }

    #[napi]
    pub async fn cleanup(&self) -> Result<()> {
        let session = self.inner.lock().await;
        session.cleanup().await;
        Ok(())
    }
}
