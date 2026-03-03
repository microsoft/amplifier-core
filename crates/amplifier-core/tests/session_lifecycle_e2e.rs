//! Integration test: AmplifierSession lifecycle in pure Rust.
//!
//! Proves the universal API works without Python, exercising:
//! - AmplifierSession type alias (Task 1)
//! - Coordinator::to_dict() (Task 2)
//! - cleanup() clears initialized (Task 3)
//! - SessionConfig::from_json() (Task 5)

use amplifier_core::testing::EchoTool;
use amplifier_core::transport::load_native_tool;
use amplifier_core::{AmplifierSession, SessionConfig};

#[test]
fn amplifier_session_type_alias_works() {
    let config = SessionConfig::minimal("test-orch", "test-ctx");
    let _session: AmplifierSession = AmplifierSession::new(config, None, None);
    // If this compiles, the type alias is correct
}

#[test]
fn coordinator_to_dict_from_session() {
    let config = SessionConfig::minimal("test-orch", "test-ctx");
    let session = AmplifierSession::new(config, None, None);
    let tool = load_native_tool(EchoTool);
    session.coordinator().mount_tool("echo", tool);

    let dict = session.coordinator().to_dict();
    assert!(dict.contains_key("tools"));
    let tools = dict["tools"].as_array().unwrap();
    assert!(tools.contains(&serde_json::json!("echo")));
    assert!(dict.contains_key("has_orchestrator"));
    assert_eq!(dict["has_orchestrator"], serde_json::json!(false));
}

#[test]
fn session_config_from_json() {
    let config = SessionConfig::from_json(
        r#"{
        "session": {"orchestrator": "loop-basic", "context": "context-simple"}
    }"#,
    )
    .unwrap();
    let session = AmplifierSession::new(config, None, None);
    assert!(!session.is_initialized());
    // session_id should be a UUID v4 (36 chars with hyphens)
    assert_eq!(session.session_id().len(), 36);
}

#[tokio::test]
async fn cleanup_resets_initialized() {
    let config = SessionConfig::minimal("test-orch", "test-ctx");
    let session = AmplifierSession::new(config, None, None);
    session.set_initialized();
    assert!(session.is_initialized());
    session.cleanup().await;
    assert!(
        !session.is_initialized(),
        "cleanup should clear initialized flag"
    );
}
