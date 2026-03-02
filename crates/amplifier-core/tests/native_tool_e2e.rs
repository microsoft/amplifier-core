//! End-to-end integration test for native Rust tool modules.
//!
//! Creates an `EchoTool`, loads it via `load_native_tool`, and verifies
//! direct execution and coordinator-mounted execution.

use amplifier_core::coordinator::Coordinator;
use amplifier_core::testing::EchoTool;
use amplifier_core::traits::Tool;
use amplifier_core::transport::load_native_tool;

#[test]
fn echo_tool_name() {
    let tool = EchoTool;
    assert_eq!(tool.name(), "echo");
}

#[test]
fn echo_tool_description() {
    let tool = EchoTool;
    assert_eq!(tool.description(), "Echoes input back unchanged");
}

#[tokio::test]
async fn echo_tool_execute_returns_input() {
    let tool = load_native_tool(EchoTool);
    let input = serde_json::json!({"hello": "world"});
    let result = tool.execute(input.clone()).await.unwrap();
    assert!(result.success);
    assert_eq!(result.output, Some(input));
    assert!(result.error.is_none());
}

#[tokio::test]
async fn echo_tool_via_coordinator() {
    let coord = Coordinator::new(Default::default());
    let tool = load_native_tool(EchoTool);
    coord.mount_tool("echo", tool);

    let mounted = coord.get_tool("echo").expect("tool should be mounted");
    let result = mounted
        .execute(serde_json::json!({"test": 42}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.output, Some(serde_json::json!({"test": 42})));
}
