//! Transport dispatch — routes module loading to the correct bridge.

use std::sync::Arc;

use crate::traits::Tool;

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
pub async fn load_grpc_tool(
    endpoint: &str,
) -> Result<Arc<dyn Tool>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::grpc_tool::GrpcToolBridge::connect(endpoint).await?;
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
) -> Result<Arc<dyn Tool>, Box<dyn std::error::Error + Send + Sync>> {
    let bridge = crate::bridges::wasm_tool::WasmToolBridge::from_bytes(wasm_bytes)?;
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
}
