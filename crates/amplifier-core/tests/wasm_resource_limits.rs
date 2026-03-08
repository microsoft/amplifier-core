//! Verifies that WASM modules with infinite loops are terminated
//! by epoch interruption and do not hang indefinitely.

#[cfg(feature = "wasm")]
#[tokio::test]
async fn infinite_loop_wasm_module_is_terminated() {
    use std::time::{Duration, Instant};

    let engine = amplifier_core::bridges::create_wasm_engine().unwrap();

    // Locate the infinite-loop fixture relative to CARGO_MANIFEST_DIR.
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest.join("../../../tests/fixtures/wasm/infinite-loop.wasm"),
        manifest.join("../../tests/fixtures/wasm/infinite-loop.wasm"),
    ];
    let bytes = candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| std::fs::read(p).unwrap())
        .expect("infinite-loop.wasm not found");

    let start = Instant::now();
    let result = amplifier_core::bridges::wasm_tool::WasmToolBridge::from_bytes(&bytes, engine);
    let elapsed = start.elapsed();

    assert!(result.is_err(), "Infinite loop should be trapped");
    assert!(
        elapsed < Duration::from_secs(60),
        "Should terminate within timeout, took {:?}",
        elapsed
    );
}
