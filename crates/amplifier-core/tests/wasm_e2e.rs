//! WASM E2E integration tests.
//!
//! Tests all 6 WASM module types end-to-end using pre-compiled .wasm fixtures.
//! Each test loads a fixture via `transport::load_wasm_*` and calls trait methods
//! directly — this is the public API surface, not the bridge internals.
//!
//! Run with: cargo test -p amplifier-core --features wasm --test wasm_e2e

#![cfg(feature = "wasm")]

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use amplifier_core::messages::{ChatRequest, Message, MessageContent, Role};
use amplifier_core::models::{ApprovalRequest, HookAction};
use amplifier_core::transport::{
    load_wasm_approval, load_wasm_context, load_wasm_hook, load_wasm_orchestrator,
    load_wasm_provider, load_wasm_tool,
};
use amplifier_core::wasm_engine::WasmEngine;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a pre-compiled .wasm fixture by name.
///
/// CARGO_MANIFEST_DIR = `.../crates/amplifier-core`; fixtures live two
/// levels up at the workspace root under `tests/fixtures/wasm/`.
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
// Test 1: Tool — load from bytes
// ---------------------------------------------------------------------------

/// Load `echo-tool.wasm` via `load_wasm_tool` and verify the public API surface:
/// - `name()` returns "echo-tool"
/// - `get_spec()` has the correct name and a description
#[test]
fn tool_load_from_bytes() {
    let engine = make_engine();
    let bytes = fixture("echo-tool.wasm");

    let tool = load_wasm_tool(&bytes, engine).expect("load_wasm_tool should succeed");

    assert_eq!(tool.name(), "echo-tool", "name() mismatch");

    let spec = tool.get_spec();
    assert_eq!(spec.name, "echo-tool", "spec.name mismatch");
    assert!(
        spec.description.is_some(),
        "spec.description should be set, got None"
    );
    assert!(
        !spec.description.as_deref().unwrap_or("").is_empty(),
        "spec.description should be non-empty"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Tool — execute roundtrip
// ---------------------------------------------------------------------------

/// Load echo-tool, execute with JSON input, verify the output echoes the input.
#[tokio::test]
async fn tool_execute_roundtrip() {
    let engine = make_engine();
    let bytes = fixture("echo-tool.wasm");

    let tool = load_wasm_tool(&bytes, engine).expect("load_wasm_tool should succeed");

    let input = json!({"message": "hello", "count": 42});
    let result = tool
        .execute(input.clone())
        .await
        .expect("execute should succeed");

    assert!(result.success, "ToolResult.success should be true");
    assert_eq!(
        result.output,
        Some(input),
        "ToolResult.output should echo the input"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Hook — deny action
// ---------------------------------------------------------------------------

/// Load `deny-hook.wasm`, handle an event, verify the hook returns Deny.
#[tokio::test]
async fn hook_handler_deny() {
    let engine = make_engine();
    let bytes = fixture("deny-hook.wasm");

    let hook = load_wasm_hook(&bytes, engine).expect("load_wasm_hook should succeed");

    let result = hook
        .handle("tool:before_execute", json!({"tool": "bash"}))
        .await
        .expect("handle should succeed");

    assert_eq!(
        result.action,
        HookAction::Deny,
        "expected action == Deny, got {:?}",
        result.action
    );
    assert!(
        result.reason.is_some(),
        "expected a denial reason, got None"
    );
    let reason = result.reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("Denied") || reason.contains("denied"),
        "expected reason to contain 'Denied', got: {reason:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Context — stateful roundtrip
// ---------------------------------------------------------------------------

/// Load `memory-context.wasm` and exercise the full stateful cycle:
///   get (empty) → add → add → get (2 messages) → clear → get (empty)
#[tokio::test]
async fn context_manager_roundtrip() {
    let engine = make_engine();
    let bytes = fixture("memory-context.wasm");

    let ctx = load_wasm_context(&bytes, engine).expect("load_wasm_context should succeed");

    // Initially empty.
    let initial = ctx
        .get_messages()
        .await
        .expect("get_messages should succeed");
    assert!(
        initial.is_empty(),
        "expected empty context on fresh load, got {} messages",
        initial.len()
    );

    // Add two messages.
    let msg1 = json!({"role": "user", "content": "Hello"});
    let msg2 = json!({"role": "assistant", "content": "Hi there!"});
    ctx.add_message(msg1.clone())
        .await
        .expect("add_message 1 should succeed");
    ctx.add_message(msg2.clone())
        .await
        .expect("add_message 2 should succeed");

    // Now there should be 2 messages.
    let messages = ctx
        .get_messages()
        .await
        .expect("get_messages should succeed");
    assert_eq!(
        messages.len(),
        2,
        "expected 2 messages after two add_message calls, got {}",
        messages.len()
    );

    // Clear the context.
    ctx.clear().await.expect("clear should succeed");

    // Back to empty.
    let after_clear = ctx
        .get_messages()
        .await
        .expect("get_messages after clear should succeed");
    assert!(
        after_clear.is_empty(),
        "expected empty context after clear, got {} messages",
        after_clear.len()
    );
}

// ---------------------------------------------------------------------------
// Test 5: Approval — auto-approve
// ---------------------------------------------------------------------------

/// Load `auto-approve.wasm` and verify that every request is auto-approved.
#[tokio::test]
async fn approval_auto_approve() {
    let engine = make_engine();
    let bytes = fixture("auto-approve.wasm");

    let approval = load_wasm_approval(&bytes, engine).expect("load_wasm_approval should succeed");

    let request = ApprovalRequest {
        tool_name: "bash".to_string(),
        action: "Execute shell command".to_string(),
        details: HashMap::new(),
        risk_level: "medium".to_string(),
        timeout: Some(30.0),
    };

    let response = approval
        .request_approval(request)
        .await
        .expect("request_approval should succeed");

    assert!(
        response.approved,
        "auto-approve fixture should always approve, got approved=false"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Provider — complete roundtrip
// ---------------------------------------------------------------------------

/// Load `echo-provider.wasm`, verify name/info/models, and call complete().
#[tokio::test]
async fn provider_complete() {
    let engine = make_engine();
    let bytes = fixture("echo-provider.wasm");

    let provider = load_wasm_provider(&bytes, engine).expect("load_wasm_provider should succeed");

    // Verify name.
    assert_eq!(
        provider.name(),
        "echo-provider",
        "provider.name() mismatch"
    );

    // Verify get_info().
    let info = provider.get_info();
    assert_eq!(info.id, "echo-provider", "info.id mismatch");
    assert!(
        !info.display_name.is_empty(),
        "info.display_name should be non-empty"
    );

    // Verify list_models() returns at least one model.
    let models = provider
        .list_models()
        .await
        .expect("list_models should succeed");
    assert!(
        !models.is_empty(),
        "list_models() should return at least one model"
    );

    // Call complete() with a minimal ChatRequest and verify non-empty content.
    let request = ChatRequest {
        messages: vec![Message {
            role: Role::User,
            content: MessageContent::Text("Hello, echo provider!".to_string()),
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

    let response = provider
        .complete(request)
        .await
        .expect("complete() should succeed");

    assert!(
        !response.content.is_empty(),
        "provider.complete() should return non-empty content"
    );
}

// ---------------------------------------------------------------------------
// Test 7: Orchestrator — calls echo-tool via kernel-service
// ---------------------------------------------------------------------------

/// Load `passthrough-orchestrator.wasm` with a coordinator that has `echo-tool`
/// mounted (the real WASM echo-tool bridge), then call `execute()` and verify
/// a non-empty response is returned.
///
/// Flow:
///   load_wasm_orchestrator → WASM execute() → kernel-service::execute-tool (host import)
///   → coordinator.get_tool("echo-tool") → WasmToolBridge → echo-tool WASM
///   → ToolResult back → orchestrator serialises it → non-empty String
#[tokio::test]
async fn orchestrator_calls_kernel() {
    let engine = make_engine();
    let orch_bytes = fixture("passthrough-orchestrator.wasm");
    let echo_bytes = fixture("echo-tool.wasm");

    // Build a coordinator and mount the WASM echo-tool bridge.
    let coordinator = Arc::new(amplifier_core::coordinator::Coordinator::new_for_test());
    let echo_tool = load_wasm_tool(&echo_bytes, Arc::clone(&engine))
        .expect("load echo-tool for coordinator");
    coordinator.mount_tool("echo-tool", echo_tool);

    // Load the orchestrator with the prepared coordinator.
    let orchestrator =
        load_wasm_orchestrator(&orch_bytes, Arc::clone(&engine), Arc::clone(&coordinator))
            .expect("load_wasm_orchestrator should succeed");

    // Execute the orchestrator with a simple prompt.
    let result = orchestrator
        .execute(
            "test prompt".to_string(),
            Arc::new(amplifier_core::testing::FakeContextManager::new()),
            Default::default(),
            Default::default(),
            json!({}),
            json!({}),
        )
        .await;

    let response = result.expect("orchestrator.execute() should succeed");
    assert!(
        !response.is_empty(),
        "orchestrator should return a non-empty response, got empty string"
    );
}

// ---------------------------------------------------------------------------
// Test 8: Calculator tool — loads and resolves from examples/wasm-modules/
// ---------------------------------------------------------------------------

/// Load `calculator-tool.wasm` from the examples directory via `load_wasm_tool`
/// and verify that spec.name == "calculator".
///
/// This proves the developer authoring workflow: a fresh project using the
/// amplifier-guest SDK compiles, produces a valid .wasm component, and loads
/// correctly through the standard transport pipeline.
#[test]
fn calculator_tool_loads_and_resolves() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../examples/wasm-modules/calculator-tool.wasm");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("calculator-tool.wasm not found at {}: {}", path.display(), e));

    let engine = make_engine();
    let tool = load_wasm_tool(&bytes, engine).expect("load_wasm_tool should succeed");

    assert_eq!(
        tool.name(),
        "calculator",
        "spec.name should be 'calculator'"
    );

    let spec = tool.get_spec();
    assert_eq!(spec.name, "calculator", "spec.name mismatch");
    assert!(
        spec.description.is_some(),
        "spec.description should be set"
    );
}
