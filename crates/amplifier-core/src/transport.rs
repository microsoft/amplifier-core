//! Transport dispatch — routes module loading to the correct bridge.

use std::sync::Arc;

use crate::traits::{ApprovalProvider, ContextManager, HookHandler, Orchestrator, Provider, Tool};

/// Supported transport types.
#[derive(Debug, Clone, PartialEq)]
pub enum Transport {
    Python,
    Grpc,
    Native,
    Wasm,
}

impl Transport {
    /// Parse a transport string from module configuration.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "grpc" => Transport::Grpc,
            "native" => Transport::Native,
            "wasm" => Transport::Wasm,
            _ => Transport::Python,
        }
    }
}

/// Load a tool module via gRPC transport.
///
/// # Arguments
///
/// * `endpoint` — gRPC endpoint URL (e.g. `"http://localhost:50051"`).
pub async fn load_grpc_tool(
    endpoint: &str,
) -> Result<Arc<dyn Tool>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::grpc_tool::GrpcToolBridge::connect(endpoint).await?;
    Ok(Arc::new(bridge))
}

/// Load an orchestrator module via gRPC transport.
///
/// # Arguments
///
/// * `endpoint` — gRPC endpoint URL (e.g. `"http://localhost:50051"`).
/// * `session_id` — Session identifier threaded through execute requests so
///   the remote orchestrator can route KernelService callbacks back to the
///   correct session.
pub async fn load_grpc_orchestrator(
    endpoint: &str,
    session_id: &str,
) -> Result<Arc<dyn Orchestrator>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge =
        crate::bridges::grpc_orchestrator::GrpcOrchestratorBridge::connect(endpoint, session_id)
            .await?;
    Ok(Arc::new(bridge))
}

/// Load a provider module via gRPC transport.
///
/// Connects to a remote `ProviderService` and returns an `Arc<dyn Provider>`
/// that is indistinguishable from a local provider.
///
/// # Arguments
///
/// * `endpoint` — gRPC endpoint URL (e.g. `"http://localhost:50051"`).
///
/// # Examples
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// use amplifier_core::transport::load_grpc_provider;
///
/// let provider = load_grpc_provider("http://localhost:50051").await?;
/// println!("Connected to provider: {}", provider.name());
/// # Ok(())
/// # }
/// ```
pub async fn load_grpc_provider(
    endpoint: &str,
) -> Result<Arc<dyn Provider>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::grpc_provider::GrpcProviderBridge::connect(endpoint).await?;
    Ok(Arc::new(bridge))
}

/// Load a hook handler module via gRPC transport.
///
/// Connects to a remote `HookService` and returns an `Arc<dyn HookHandler>`
/// that is indistinguishable from a local hook handler.
///
/// # Arguments
///
/// * `endpoint` — gRPC endpoint URL (e.g. `"http://localhost:50051"`).
///
/// # Examples
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// use amplifier_core::transport::load_grpc_hook;
///
/// let hook = load_grpc_hook("http://localhost:50051").await?;
/// # Ok(())
/// # }
/// ```
pub async fn load_grpc_hook(
    endpoint: &str,
) -> Result<Arc<dyn HookHandler>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::grpc_hook::GrpcHookBridge::connect(endpoint).await?;
    Ok(Arc::new(bridge))
}

/// Load a context manager module via gRPC transport.
///
/// Connects to a remote `ContextService` and returns an `Arc<dyn ContextManager>`
/// that is indistinguishable from a local context manager.
///
/// # Arguments
///
/// * `endpoint` — gRPC endpoint URL (e.g. `"http://localhost:50051"`).
///
/// # Examples
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// use amplifier_core::transport::load_grpc_context;
///
/// let context = load_grpc_context("http://localhost:50051").await?;
/// # Ok(())
/// # }
/// ```
pub async fn load_grpc_context(
    endpoint: &str,
) -> Result<Arc<dyn ContextManager>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::grpc_context::GrpcContextBridge::connect(endpoint).await?;
    Ok(Arc::new(bridge))
}

/// Load an approval provider module via gRPC transport.
///
/// Connects to a remote `ApprovalService` and returns an `Arc<dyn ApprovalProvider>`
/// that is indistinguishable from a local approval provider.
///
/// # Arguments
///
/// * `endpoint` — gRPC endpoint URL (e.g. `"http://localhost:50051"`).
///
/// # Examples
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// use amplifier_core::transport::load_grpc_approval;
///
/// let approval = load_grpc_approval("http://localhost:50051").await?;
/// # Ok(())
/// # }
/// ```
pub async fn load_grpc_approval(
    endpoint: &str,
) -> Result<Arc<dyn ApprovalProvider>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::grpc_approval::GrpcApprovalBridge::connect(endpoint).await?;
    Ok(Arc::new(bridge))
}

/// Load a native Rust tool module (zero-overhead, no bridge).
pub fn load_native_tool(tool: impl Tool + 'static) -> Arc<dyn Tool> {
    Arc::new(tool)
}

/// Load a WASM tool module from raw bytes (requires `wasm` feature).
#[cfg(feature = "wasm")]
pub fn load_wasm_tool(
    wasm_bytes: &[u8],
    engine: Arc<wasmtime::Engine>,
) -> Result<Arc<dyn Tool>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::wasm_tool::WasmToolBridge::from_bytes(wasm_bytes, engine)?;
    Ok(Arc::new(bridge))
}

/// Load a WASM hook handler from raw bytes (requires `wasm` feature).
#[cfg(feature = "wasm")]
pub fn load_wasm_hook(
    wasm_bytes: &[u8],
    engine: Arc<wasmtime::Engine>,
) -> Result<Arc<dyn HookHandler>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::wasm_hook::WasmHookBridge::from_bytes(wasm_bytes, engine)?;
    Ok(Arc::new(bridge))
}

/// Load a WASM context manager from raw bytes (requires `wasm` feature).
#[cfg(feature = "wasm")]
pub fn load_wasm_context(
    wasm_bytes: &[u8],
    engine: Arc<wasmtime::Engine>,
) -> Result<Arc<dyn ContextManager>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::wasm_context::WasmContextBridge::from_bytes(wasm_bytes, engine)?;
    Ok(Arc::new(bridge))
}

/// Load a WASM approval provider from raw bytes (requires `wasm` feature).
#[cfg(feature = "wasm")]
pub fn load_wasm_approval(
    wasm_bytes: &[u8],
    engine: Arc<wasmtime::Engine>,
) -> Result<Arc<dyn ApprovalProvider>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::wasm_approval::WasmApprovalBridge::from_bytes(wasm_bytes, engine)?;
    Ok(Arc::new(bridge))
}

/// Load a WASM provider from raw bytes (requires `wasm` feature).
#[cfg(feature = "wasm")]
pub fn load_wasm_provider(
    wasm_bytes: &[u8],
    engine: Arc<wasmtime::Engine>,
) -> Result<Arc<dyn Provider>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::wasm_provider::WasmProviderBridge::from_bytes(wasm_bytes, engine)?;
    Ok(Arc::new(bridge))
}

/// Load a WASM orchestrator from raw bytes (requires `wasm` feature).
///
/// The orchestrator bridge requires a [`Coordinator`](crate::coordinator::Coordinator)
/// for kernel-service host imports used during execution.
#[cfg(feature = "wasm")]
pub fn load_wasm_orchestrator(
    wasm_bytes: &[u8],
    engine: Arc<wasmtime::Engine>,
    coordinator: Arc<crate::coordinator::Coordinator>,
) -> Result<Arc<dyn Orchestrator>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::wasm_orchestrator::WasmOrchestratorBridge::from_bytes(
        wasm_bytes,
        engine,
        coordinator,
    )?;
    Ok(Arc::new(bridge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_parsing() {
        assert_eq!(Transport::from_str("python"), Transport::Python);
        assert_eq!(Transport::from_str("grpc"), Transport::Grpc);
        assert_eq!(Transport::from_str("native"), Transport::Native);
        assert_eq!(Transport::from_str("wasm"), Transport::Wasm);
        assert_eq!(Transport::from_str("unknown"), Transport::Python);
    }

    #[cfg(feature = "wasm")]
    fn fixture(name: &str) -> Vec<u8> {
        // CARGO_MANIFEST_DIR = …/crates/amplifier-core; fixtures live at workspace root.
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest.join("../../tests/fixtures/wasm").join(name);
        std::fs::read(&path)
            .unwrap_or_else(|e| panic!("fixture {name} not found at {}: {e}", path.display()))
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_wasm_tool_returns_arc_dyn_tool() {
        let wasm_bytes = fixture("echo-tool.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let tool = super::load_wasm_tool(&wasm_bytes, engine.inner());
        assert!(tool.is_ok());
        assert_eq!(tool.unwrap().name(), "echo-tool");
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_wasm_hook_returns_arc_dyn_hook_handler() {
        let wasm_bytes = fixture("deny-hook.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let hook = super::load_wasm_hook(&wasm_bytes, engine.inner());
        assert!(hook.is_ok());
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_wasm_context_returns_arc_dyn_context_manager() {
        let wasm_bytes = fixture("memory-context.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let ctx = super::load_wasm_context(&wasm_bytes, engine.inner());
        assert!(ctx.is_ok());
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_wasm_approval_returns_arc_dyn_approval_provider() {
        let wasm_bytes = fixture("auto-approve.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let approval = super::load_wasm_approval(&wasm_bytes, engine.inner());
        assert!(approval.is_ok());
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_wasm_provider_returns_arc_dyn_provider() {
        let wasm_bytes = fixture("echo-provider.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let provider = super::load_wasm_provider(&wasm_bytes, engine.inner());
        assert!(provider.is_ok());
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn load_wasm_orchestrator_returns_arc_dyn_orchestrator() {
        let wasm_bytes = fixture("passthrough-orchestrator.wasm");
        let engine = crate::wasm_engine::WasmEngine::new().unwrap();
        let coordinator = std::sync::Arc::new(crate::coordinator::Coordinator::new_for_test());
        let orch = super::load_wasm_orchestrator(&wasm_bytes, engine.inner(), coordinator);
        assert!(orch.is_ok());
    }

    // ---------------------------------------------------------------
    // gRPC transport functions — compile-time + type verification
    // ---------------------------------------------------------------

    /// Verify load_grpc_provider exists and returns the correct type.
    /// Uses a non-listening endpoint so connect() will fail — we only
    /// care that the function exists and has the right signature.
    #[tokio::test]
    async fn load_grpc_provider_returns_result_arc_dyn_provider() {
        let result = super::load_grpc_provider("http://[::1]:59001").await;
        // Connection to non-listening port should fail
        assert!(
            result.is_err(),
            "expected connection error to non-listening port"
        );
    }

    /// Verify load_grpc_hook exists and returns the correct type.
    #[tokio::test]
    async fn load_grpc_hook_returns_result_arc_dyn_hook_handler() {
        let result = super::load_grpc_hook("http://[::1]:59002").await;
        assert!(
            result.is_err(),
            "expected connection error to non-listening port"
        );
    }

    /// Verify load_grpc_context exists and returns the correct type.
    #[tokio::test]
    async fn load_grpc_context_returns_result_arc_dyn_context_manager() {
        let result = super::load_grpc_context("http://[::1]:59003").await;
        assert!(
            result.is_err(),
            "expected connection error to non-listening port"
        );
    }

    /// Verify load_grpc_approval exists and returns the correct type.
    #[tokio::test]
    async fn load_grpc_approval_returns_result_arc_dyn_approval_provider() {
        let result = super::load_grpc_approval("http://[::1]:59004").await;
        assert!(
            result.is_err(),
            "expected connection error to non-listening port"
        );
    }
}
