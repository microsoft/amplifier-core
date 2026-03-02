//! WASM transport integration test.
//!
//! Verifies Transport::Wasm parsing and compile-time trait satisfaction.

#![cfg(feature = "wasm")]

use amplifier_core::transport::Transport;

#[test]
fn transport_wasm_parsing() {
    assert_eq!(Transport::from_str("wasm"), Transport::Wasm);
}

#[test]
fn wasm_tool_bridge_satisfies_tool_trait() {
    use amplifier_core::bridges::wasm_tool::WasmToolBridge;
    use amplifier_core::traits::Tool;
    use std::sync::Arc;

    // Compile-time check only — if this compiles, the trait is satisfied
    fn _check(bridge: WasmToolBridge) -> Arc<dyn Tool> {
        Arc::new(bridge)
    }
}
