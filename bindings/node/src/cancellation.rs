// ---------------------------------------------------------------------------
// JsCancellationToken — wraps amplifier_core::CancellationToken for Node.js
// ---------------------------------------------------------------------------

/// Wraps `amplifier_core::CancellationToken` for Node.js.
///
/// State machine: None → Graceful → Immediate, with reset back to None.
#[napi]
pub struct JsCancellationToken {
    inner: amplifier_core::CancellationToken,
}

impl Default for JsCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl JsCancellationToken {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: amplifier_core::CancellationToken::new(),
        }
    }

    /// Internal factory for wrapping an existing kernel token.
    pub fn from_inner(inner: amplifier_core::CancellationToken) -> Self {
        Self { inner }
    }

    #[napi(getter)]
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    #[napi(getter)]
    pub fn is_graceful(&self) -> bool {
        self.inner.is_graceful()
    }

    #[napi(getter)]
    pub fn is_immediate(&self) -> bool {
        self.inner.is_immediate()
    }

    #[napi]
    pub fn request_graceful(&self) {
        self.inner.request_graceful();
    }

    #[napi]
    pub fn request_immediate(&self) {
        self.inner.request_immediate();
    }

    #[napi]
    pub fn reset(&self) {
        self.inner.reset();
    }
}
