//! WASM bridge for sandboxed LLM provider modules (Component Model).
//!
//! [`WasmProviderBridge`] loads a WASM Component via wasmtime and implements the
//! [`Provider`] trait, enabling sandboxed in-process LLM completions. The guest
//! exports `get-info` (returns JSON-serialized `ProviderInfo` bytes), `list-models`,
//! `complete`, and `parse-tool-calls`.
//!
//! Gated behind the `wasm` feature flag.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use wasmtime::component::Component;
use wasmtime::{Engine, Store};

use crate::errors::ProviderError;
use crate::messages::{ChatRequest, ChatResponse, ToolCall};
use crate::models::{ModelInfo, ProviderInfo};
use crate::traits::Provider;

use super::wasm_tool::{create_linker_and_store, WasmState};

/// The WIT interface name used by `cargo component` for provider exports.
const INTERFACE_NAME: &str = "amplifier:modules/provider@1.0.0";

/// Shorthand for the fallible return type used by helper functions.
type WasmResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Convenience constructor for a non-retryable [`ProviderError::Other`].
fn wasm_provider_error(message: String) -> ProviderError {
    ProviderError::Other {
        message,
        provider: None,
        model: None,
        retry_after: None,
        status_code: None,
        retryable: false,
        delay_multiplier: None,
    }
}

/// Look up a typed function export from the provider component instance.
///
/// Tries:
/// 1. Direct root-level export by `func_name`
/// 2. Nested inside the [`INTERFACE_NAME`] exported instance
fn get_provider_func<Params, Results>(
    instance: &wasmtime::component::Instance,
    store: &mut Store<WasmState>,
    func_name: &str,
) -> WasmResult<wasmtime::component::TypedFunc<Params, Results>>
where
    Params: wasmtime::component::Lower + wasmtime::component::ComponentNamedList,
    Results: wasmtime::component::Lift + wasmtime::component::ComponentNamedList,
{
    // Try direct root-level export first.
    if let Ok(f) = instance.get_typed_func::<Params, Results>(&mut *store, func_name) {
        return Ok(f);
    }

    // Try nested inside the interface-exported instance.
    let iface_idx = instance
        .get_export_index(&mut *store, None, INTERFACE_NAME)
        .ok_or_else(|| format!("export instance '{INTERFACE_NAME}' not found"))?;
    let func_idx = instance
        .get_export_index(&mut *store, Some(&iface_idx), func_name)
        .ok_or_else(|| format!("export function '{func_name}' not found in '{INTERFACE_NAME}'"))?;
    let func = instance
        .get_typed_func::<Params, Results>(&mut *store, &func_idx)
        .map_err(|e| format!("typed func lookup failed for '{func_name}': {e}"))?;
    Ok(func)
}

/// Helper: call `get-info` on a fresh component instance.
///
/// Returns raw JSON bytes representing the provider's `ProviderInfo`.
/// Note: `get-info` returns `list<u8>` with **no** `result<>` wrapper.
fn call_get_info(engine: &Engine, component: &Component) -> WasmResult<Vec<u8>> {
    let (linker, mut store) = create_linker_and_store(engine)?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = get_provider_func::<(), (Vec<u8>,)>(&instance, &mut store, "get-info")?;
    let (info_bytes,) = func.call(&mut store, ())?;
    Ok(info_bytes)
}

/// Helper: call `list-models` on a fresh component instance.
///
/// Returns raw JSON bytes representing `Vec<ModelInfo>`.
fn call_list_models(engine: &Engine, component: &Component) -> WasmResult<Vec<u8>> {
    let (linker, mut store) = create_linker_and_store(engine)?;
    let instance = linker.instantiate(&mut store, component)?;

    let func =
        get_provider_func::<(), (Result<Vec<u8>, String>,)>(&instance, &mut store, "list-models")?;
    let (result,) = func.call(&mut store, ())?;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(err.into()),
    }
}

/// Helper: call `complete` on a fresh component instance.
///
/// `request_bytes` must be a JSON-serialized `ChatRequest`.
/// Returns raw JSON bytes representing `ChatResponse`.
fn call_complete(
    engine: &Engine,
    component: &Component,
    request_bytes: Vec<u8>,
) -> WasmResult<Vec<u8>> {
    let (linker, mut store) = create_linker_and_store(engine)?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = get_provider_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
        &instance, &mut store, "complete",
    )?;
    let (result,) = func.call(&mut store, (request_bytes,))?;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(err.into()),
    }
}

/// Helper: call `parse-tool-calls` on a fresh component instance.
///
/// `response_bytes` must be a JSON-serialized `ChatResponse`.
/// Returns raw JSON bytes representing `Vec<ToolCall>`.
fn call_parse_tool_calls(
    engine: &Engine,
    component: &Component,
    response_bytes: Vec<u8>,
) -> WasmResult<Vec<u8>> {
    let (linker, mut store) = create_linker_and_store(engine)?;
    let instance = linker.instantiate(&mut store, component)?;

    let func = get_provider_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
        &instance,
        &mut store,
        "parse-tool-calls",
    )?;
    let (result,) = func.call(&mut store, (response_bytes,))?;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(err.into()),
    }
}

/// A bridge that loads a WASM Component and exposes it as a native [`Provider`].
///
/// The component is compiled once and can be instantiated for each call.
/// `get-info` is called once at construction time to cache the provider name and
/// metadata. Per-call async methods (`list-models`, `complete`) run inside
/// `spawn_blocking` tasks because wasmtime is synchronous.
/// `parse_tool_calls` is a synchronous trait method; it calls WASM directly.
pub struct WasmProviderBridge {
    engine: Arc<Engine>,
    component: Component,
    /// Provider name, cached at load time from `get-info`.
    name: String,
    /// Provider metadata, cached at load time from `get-info`.
    info: ProviderInfo,
}

impl WasmProviderBridge {
    /// Load a WASM provider component from raw bytes.
    ///
    /// Compiles the Component, instantiates it once to call `get-info`,
    /// and caches the resulting name and provider info.
    pub fn from_bytes(
        wasm_bytes: &[u8],
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let component = Component::new(&engine, wasm_bytes)?;

        // Call get-info to discover the provider's name and metadata.
        let info_bytes = call_get_info(&engine, &component)?;
        let info: ProviderInfo = serde_json::from_slice(&info_bytes)?;

        // The guest's ProviderInfo uses `id` as the canonical identifier.
        // Use it as the provider name (consistent with Python convention).
        let name = info.id.clone();

        Ok(Self {
            engine,
            component,
            name,
            info,
        })
    }

    /// Convenience: load a WASM provider component from a file path.
    pub fn from_file(
        path: &Path,
        engine: Arc<Engine>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes, engine)
    }
}

impl Provider for WasmProviderBridge {
    fn name(&self) -> &str {
        &self.name
    }

    fn get_info(&self) -> ProviderInfo {
        self.info.clone()
    }

    fn list_models(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send + '_>> {
        Box::pin(async move {
            let engine = Arc::clone(&self.engine);
            let component = self.component.clone(); // Component is Arc-backed, cheap clone

            let result_bytes =
                tokio::task::spawn_blocking(move || call_list_models(&engine, &component))
                    .await
                    .map_err(|e| {
                        wasm_provider_error(format!("WASM provider list-models task panicked: {e}"))
                    })?
                    .map_err(|e| wasm_provider_error(format!("WASM list-models failed: {e}")))?;

            let models: Vec<ModelInfo> = serde_json::from_slice(&result_bytes).map_err(|e| {
                wasm_provider_error(format!(
                    "WASM provider: failed to deserialize Vec<ModelInfo>: {e}"
                ))
            })?;

            Ok(models)
        })
    }

    fn complete(
        &self,
        request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, ProviderError>> + Send + '_>> {
        Box::pin(async move {
            // Serialize the ChatRequest to JSON bytes for the WASM guest.
            let request_bytes = serde_json::to_vec(&request).map_err(|e| {
                wasm_provider_error(format!(
                    "WASM provider: failed to serialize ChatRequest: {e}"
                ))
            })?;

            let engine = Arc::clone(&self.engine);
            let component = self.component.clone();

            let result_bytes = tokio::task::spawn_blocking(move || {
                call_complete(&engine, &component, request_bytes)
            })
            .await
            .map_err(|e| wasm_provider_error(format!("WASM provider complete task panicked: {e}")))?
            .map_err(|e| wasm_provider_error(format!("WASM complete failed: {e}")))?;

            let response: ChatResponse = serde_json::from_slice(&result_bytes).map_err(|e| {
                wasm_provider_error(format!(
                    "WASM provider: failed to deserialize ChatResponse: {e}"
                ))
            })?;

            Ok(response)
        })
    }

    fn parse_tool_calls(&self, response: &ChatResponse) -> Vec<ToolCall> {
        // Serialize the host ChatResponse for the WASM guest.
        let response_bytes = match serde_json::to_vec(response) {
            Ok(b) => b,
            Err(_) => return vec![],
        };

        // Call WASM synchronously. parse_tool_calls is not async in the trait,
        // and WASM parse-tool-calls is pure computation (no I/O), so this is acceptable.
        let result_bytes =
            match call_parse_tool_calls(&self.engine, &self.component, response_bytes) {
                Ok(b) => b,
                Err(_) => return vec![],
            };

        // Deserialize the result bytes as Vec<ToolCall>.
        // The WASM guest serializes its tool-call values as JSON; they must
        // share the same shape as the host's ToolCall (id, name, arguments fields).
        serde_json::from_slice::<Vec<ToolCall>>(&result_bytes).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::messages::{Message, MessageContent, Role};

    /// Compile-time check: WasmProviderBridge satisfies Arc<dyn Provider>.
    ///
    /// If the trait impl is broken this fails at compile time.
    #[allow(dead_code)]
    fn _assert_wasm_provider_bridge_is_provider(bridge: WasmProviderBridge) {
        let _: Arc<dyn crate::traits::Provider> = Arc::new(bridge);
    }

    /// Helper: read the echo-provider.wasm fixture bytes.
    ///
    /// The fixture lives at the workspace root under `tests/fixtures/wasm/`.
    /// CARGO_MANIFEST_DIR points to `amplifier-core/crates/amplifier-core`,
    /// so we walk up to the workspace root first.
    fn echo_provider_wasm_bytes() -> Vec<u8> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // Two candidates because the workspace root may be at different depths
        // depending on how the repo is checked out:
        //   - 3 levels up: used as a git submodule (super-repo/amplifier-core/crates/amplifier-core)
        //   - 2 levels up: standalone checkout (amplifier-core/crates/amplifier-core)
        let candidates = [
            manifest.join("../../../tests/fixtures/wasm/echo-provider.wasm"),
            manifest.join("../../tests/fixtures/wasm/echo-provider.wasm"),
        ];
        for p in &candidates {
            if p.exists() {
                return std::fs::read(p)
                    .unwrap_or_else(|e| panic!("Failed to read echo-provider.wasm at {p:?}: {e}"));
            }
        }
        panic!(
            "echo-provider.wasm not found. Tried: {:?}",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        );
    }

    /// Helper: create a shared engine with component model enabled.
    fn make_engine() -> Arc<Engine> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        Arc::new(Engine::new(&config).expect("engine creation failed"))
    }

    /// E2E: load echo-provider.wasm and verify name().
    #[test]
    fn load_echo_provider_name() {
        let engine = make_engine();
        let bytes = echo_provider_wasm_bytes();
        let bridge =
            WasmProviderBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");
        assert_eq!(bridge.name(), "echo-provider");
    }

    /// E2E: get_info() returns expected provider metadata.
    #[test]
    fn echo_provider_get_info() {
        let engine = make_engine();
        let bytes = echo_provider_wasm_bytes();
        let bridge =
            WasmProviderBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        let info = bridge.get_info();
        assert_eq!(
            info.id, "echo-provider",
            "expected info.id == 'echo-provider'"
        );
        assert_eq!(
            info.display_name, "Echo Provider",
            "expected info.display_name == 'Echo Provider'"
        );
    }

    /// E2E: list_models() returns at least one model with id "echo-model".
    #[tokio::test]
    async fn echo_provider_list_models() {
        let engine = make_engine();
        let bytes = echo_provider_wasm_bytes();
        let bridge =
            WasmProviderBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        let models = bridge
            .list_models()
            .await
            .expect("list_models should succeed");

        assert!(!models.is_empty(), "expected at least one model");
        assert!(
            models.iter().any(|m| m.id == "echo-model"),
            "expected a model with id 'echo-model', got: {:?}",
            models.iter().map(|m| &m.id).collect::<Vec<_>>()
        );
    }

    /// E2E: complete() with minimal request returns a ChatResponse with content.
    #[tokio::test]
    async fn echo_provider_complete() {
        let engine = make_engine();
        let bytes = echo_provider_wasm_bytes();
        let bridge =
            WasmProviderBridge::from_bytes(&bytes, engine).expect("from_bytes should succeed");

        let request = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("hello".to_string()),
                name: None,
                tool_call_id: None,
                metadata: None,
                extensions: HashMap::new(),
            }],
            tools: None,
            response_format: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            conversation_id: None,
            stream: None,
            metadata: None,
            model: Some("echo-model".to_string()),
            tool_choice: None,
            stop: None,
            reasoning_effort: None,
            timeout: None,
            extensions: HashMap::new(),
        };

        let response = bridge
            .complete(request)
            .await
            .expect("complete should succeed");

        assert!(
            !response.content.is_empty(),
            "expected non-empty content in ChatResponse"
        );
    }
}
