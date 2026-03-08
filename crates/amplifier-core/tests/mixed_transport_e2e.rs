//! Mixed-transport E2E integration test.
//!
//! Proves that native Rust modules (simulating Python-loaded modules) and
//! WASM modules coexist in the same Coordinator without the coordinator being
//! able to tell them apart — all are just `Arc<dyn Trait>` at runtime.
//!
//! Test scenario:
//!   - Native `FakeOrchestrator` → simulates a Python-loaded orchestrator
//!   - Native `FakeProvider`     → simulates a Python-loaded provider
//!   - WASM `echo-tool.wasm`     → loaded via `load_wasm_tool()`
//!   - WASM `deny-hook.wasm`     → loaded via `load_wasm_hook()` and registered
//!
//! Run with:
//!   cargo test -p amplifier-core --features wasm --test mixed_transport_e2e -- --test-threads=1

#![cfg(feature = "wasm")]

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use amplifier_core::coordinator::Coordinator;
use amplifier_core::models::HookAction;
use amplifier_core::testing::{FakeContextManager, FakeOrchestrator, FakeProvider};
use amplifier_core::transport::{load_wasm_hook, load_wasm_tool};
use amplifier_core::wasm_engine::WasmEngine;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a pre-compiled .wasm fixture by name.
///
/// CARGO_MANIFEST_DIR = `.../crates/amplifier-core`; fixtures live at the
/// workspace root under `tests/fixtures/wasm/`.
fn fixture(name: &str) -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../tests/fixtures/wasm").join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture '{}' not found at {}: {}", name, path.display(), e))
}

/// Create a shared wasmtime Engine with Component Model enabled.
fn make_engine() -> Arc<wasmtime::Engine> {
    WasmEngine::new()
        .expect("WasmEngine::new() should succeed")
        .inner()
}

// ---------------------------------------------------------------------------
// Test: mixed-transport session — native + WASM modules coexist
// ---------------------------------------------------------------------------

/// Prove that native Rust modules (simulating Python-loaded) and WASM modules
/// coexist in the same Coordinator session.
///
/// Steps:
///   1. Create Coordinator
///   2. Mount native FakeOrchestrator  (simulates Python-loaded module)
///   3. Mount native FakeProvider      (simulates Python-loaded module)
///   4. Mount WASM echo-tool           (loaded via load_wasm_tool)
///   5. Register WASM deny-hook        (loaded via load_wasm_hook)
///   6. Assert all four are present and queryable
///   7. Execute WASM tool via coordinator → verify result
///   8. Emit hook event → verify WASM deny-hook fires and returns Deny
///   9. Call native provider via coordinator → verify response
///  10. Call native orchestrator via coordinator → verify response
#[tokio::test]
async fn mixed_transport_session() {
    let engine = make_engine();

    // ── Step 1: Create Coordinator ──────────────────────────────────────────
    let coordinator = Arc::new(Coordinator::new_for_test());

    // ── Step 2: Mount native FakeOrchestrator ──────────────────────────────
    let native_orch = Arc::new(FakeOrchestrator::new("native-orchestrator-response"));
    coordinator.set_orchestrator(Arc::clone(&native_orch) as _);

    // ── Step 3: Mount native FakeProvider ──────────────────────────────────
    let native_provider = Arc::new(FakeProvider::new(
        "fake-provider",
        "native-provider-response",
    ));
    coordinator.mount_provider("fake-provider", Arc::clone(&native_provider) as _);

    // ── Step 4: Mount WASM echo-tool ───────────────────────────────────────
    let echo_bytes = fixture("echo-tool.wasm");
    let wasm_tool =
        load_wasm_tool(&echo_bytes, Arc::clone(&engine)).expect("load_wasm_tool should succeed");
    coordinator.mount_tool("echo-tool", Arc::clone(&wasm_tool));

    // ── Step 5: Register WASM deny-hook ────────────────────────────────────
    let deny_bytes = fixture("deny-hook.wasm");
    let wasm_hook =
        load_wasm_hook(&deny_bytes, Arc::clone(&engine)).expect("load_wasm_hook should succeed");
    // `register` returns an unregister closure; bind it so the must_use warning is satisfied.
    let _unregister_deny_hook = coordinator.hooks().register(
        "tool:before_execute",
        wasm_hook,
        0,
        Some("wasm-deny-hook".to_string()),
    );

    // ── Step 6: Verify all four modules are present and queryable ───────────

    // Orchestrator is set
    assert!(
        coordinator.has_orchestrator(),
        "coordinator should have an orchestrator mounted"
    );

    // Provider is queryable by name
    let retrieved_provider = coordinator
        .get_provider("fake-provider")
        .expect("fake-provider should be mounted");
    assert_eq!(
        retrieved_provider.name(),
        "fake-provider",
        "provider name mismatch"
    );

    // WASM tool is queryable by name
    let retrieved_tool = coordinator
        .get_tool("echo-tool")
        .expect("echo-tool should be mounted");
    assert_eq!(
        retrieved_tool.name(),
        "echo-tool",
        "WASM tool name mismatch"
    );

    // Confirm tool/provider counts (coordinator sees 1 tool, 1 provider)
    assert_eq!(
        coordinator.tools().len(),
        1,
        "coordinator should have exactly 1 tool mounted"
    );
    assert_eq!(
        coordinator.providers().len(),
        1,
        "coordinator should have exactly 1 provider mounted"
    );

    // ── Step 7: Execute WASM tool via coordinator → verify result ───────────
    let tool = coordinator
        .get_tool("echo-tool")
        .expect("echo-tool must be mounted");
    let input = json!({"mixed": "transport", "source": "wasm"});
    let tool_result = tool
        .execute(input.clone())
        .await
        .expect("WASM echo-tool execute() should succeed");

    assert!(
        tool_result.success,
        "WASM echo-tool should return success=true"
    );
    assert_eq!(
        tool_result.output,
        Some(input),
        "WASM echo-tool should echo the input back"
    );

    // ── Step 8: Emit hook event → verify WASM deny-hook fires (returns Deny) ─
    let hook_result = coordinator
        .hooks()
        .emit("tool:before_execute", json!({"tool": "bash"}))
        .await;

    assert_eq!(
        hook_result.action,
        HookAction::Deny,
        "WASM deny-hook should return Deny, got {:?}",
        hook_result.action
    );
    assert!(
        hook_result.reason.is_some(),
        "WASM deny-hook should provide a denial reason"
    );

    // ── Step 9: Call native provider via coordinator → verify response ──────
    use amplifier_core::messages::{ChatRequest, Message, MessageContent, Role};

    let provider = coordinator
        .get_provider("fake-provider")
        .expect("fake-provider must be mounted");
    let chat_request = ChatRequest {
        messages: vec![Message {
            role: Role::User,
            content: MessageContent::Text("hello from mixed session".to_string()),
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
        model: None,
        tool_choice: None,
        stop: None,
        reasoning_effort: None,
        timeout: None,
        extensions: HashMap::new(),
    };
    let provider_response = provider
        .complete(chat_request)
        .await
        .expect("native FakeProvider.complete() should succeed");

    assert!(
        !provider_response.content.is_empty(),
        "native FakeProvider should return non-empty content"
    );

    // ── Step 10: Call native orchestrator via coordinator → verify response ──
    let orchestrator = coordinator
        .orchestrator()
        .expect("orchestrator must be mounted");
    let orch_response = orchestrator
        .execute(
            "mixed transport test prompt".to_string(),
            Arc::new(FakeContextManager::new()),
            Default::default(),
            Default::default(),
            json!({}),
            json!({}),
        )
        .await
        .expect("native FakeOrchestrator.execute() should succeed");

    assert_eq!(
        orch_response, "native-orchestrator-response",
        "native orchestrator should return the configured response"
    );
}

// ---------------------------------------------------------------------------
// Test: coordinator to_dict reflects both native and WASM modules
// ---------------------------------------------------------------------------

/// Verify that `Coordinator::to_dict()` correctly reports mixed-transport state.
///
/// The coordinator's introspection API must be unaware of transport origin —
/// both native and WASM modules appear identically in the registry.
#[test]
fn coordinator_to_dict_reflects_mixed_modules() {
    let engine = make_engine();

    let coordinator = Coordinator::new_for_test();

    // Mount native provider
    let native_provider = Arc::new(FakeProvider::new("native-llm", "response"));
    coordinator.mount_provider("native-llm", native_provider as _);

    // Mount WASM tool
    let echo_bytes = fixture("echo-tool.wasm");
    let wasm_tool = load_wasm_tool(&echo_bytes, engine).expect("load_wasm_tool should succeed");
    coordinator.mount_tool("echo-tool", wasm_tool);

    // Mount native orchestrator
    let orch = Arc::new(FakeOrchestrator::new("ok"));
    coordinator.set_orchestrator(orch as _);

    // Inspect via to_dict
    let dict = coordinator.to_dict();

    let tools = dict["tools"].as_array().expect("tools must be array");
    assert!(
        tools.contains(&json!("echo-tool")),
        "to_dict should list WASM echo-tool, got: {tools:?}"
    );

    let providers = dict["providers"]
        .as_array()
        .expect("providers must be array");
    assert!(
        providers.contains(&json!("native-llm")),
        "to_dict should list native provider, got: {providers:?}"
    );

    assert_eq!(
        dict["has_orchestrator"],
        json!(true),
        "to_dict should report orchestrator as mounted"
    );
}
