# Phase 1: Rust Public API for AmplifierSession — Implementation Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan task-by-task.

**Goal:** Make the Rust crate (`amplifier-core`) usable as a standalone library with the universal AmplifierSession API — same names, same lifecycle as Python.

**Architecture:** Add `AmplifierSession` type alias, `Coordinator::to_dict()`, fix `cleanup()` to clear initialized flag, thread real hooks/coordinator through `execute()`. Do NOT touch the Python bridge or module loading (that stays Python-side for now).

**Tech Stack:** Rust, serde_json, existing amplifier-core crate.

**Design Document:** `docs/plans/2026-03-02-cross-language-session-sdk-design.md`

**Build commands:**
- Rust tests: `cargo test -p amplifier-core`
- Python tests: `cd amplifier-core && maturin develop && uv run pytest tests/ bindings/python/tests/`
- Clippy: `cargo clippy -p amplifier-core -- -D warnings`
- Format: `cargo fmt -p amplifier-core --check`

**Invariant:** ALL existing tests (233 unit + 4 native_tool_e2e + 1 grpc_tool_e2e + 13 doc-tests = 251 total) must pass unchanged throughout. The Python bridge is NOT modified.

**Baseline:** 233 unit tests passing, 0 failures as of 2026-03-02.

---

### Task 1: Add `AmplifierSession` type alias

**Files:**
- Modify: `crates/amplifier-core/src/lib.rs` (lines 70-71)

**Step 1: Add the alias and re-export**

In `crates/amplifier-core/src/lib.rs`, find the existing re-export at line 71:
```rust
pub use session::{Session, SessionConfig};
```

Change it to:
```rust
pub use session::{Session, SessionConfig};

/// `AmplifierSession` is the universal name for the session type across all language SDKs.
/// `Session` remains available for backward compatibility.
pub type AmplifierSession = Session;
```

**Step 2: Run tests to verify nothing broke**
```bash
cargo test -p amplifier-core
```
Expected: All 251 tests pass (233 unit + 4 + 1 + 13 doc). The alias is purely additive.

**Step 3: Run clippy**
```bash
cargo clippy -p amplifier-core -- -D warnings
```
Expected: No warnings.

**Step 4: Commit**
```bash
git add crates/amplifier-core/src/lib.rs && git commit -m "feat: add AmplifierSession type alias for cross-language consistency"
```

---

### Task 2: Add `Coordinator::to_dict()` in Rust

**Files:**
- Modify: `crates/amplifier-core/src/coordinator.rs`

**Step 1: Write the failing tests**

Add these tests to the existing `#[cfg(test)] mod tests` block at the bottom of `crates/amplifier-core/src/coordinator.rs` (before the closing `}` of the `mod tests` block, which is at line 633):

```rust
    #[test]
    fn to_dict_includes_all_mount_points() {
        let coord = Coordinator::new_for_test();
        let dict = coord.to_dict();
        assert!(dict.contains_key("tools"));
        assert!(dict.contains_key("providers"));
        assert!(dict.contains_key("has_orchestrator"));
        assert!(dict.contains_key("has_context"));
        assert!(dict.contains_key("capabilities"));
    }

    #[test]
    fn to_dict_reflects_mounted_state() {
        let coord = Coordinator::new_for_test();
        let tool = Arc::new(FakeTool::new("echo", "echoes"));
        coord.mount_tool("echo", tool);
        coord.register_capability("streaming", serde_json::json!(true));
        let dict = coord.to_dict();
        let tools = dict["tools"].as_array().unwrap();
        assert!(tools.contains(&serde_json::json!("echo")));
        let caps = dict["capabilities"].as_array().unwrap();
        assert!(caps.contains(&serde_json::json!("streaming")));
    }
```

**Step 2: Run to verify they fail**
```bash
cargo test -p amplifier-core -- coordinator::tests::to_dict
```
Expected: FAIL — `to_dict` method doesn't exist yet.

**Step 3: Implement `to_dict()`**

In `crates/amplifier-core/src/coordinator.rs`, add this method to the `impl Coordinator` block. Insert it after the `capability_names()` method (after line 216, before the `// -- Subsystem accessors --` comment at line 218):

```rust
    /// Return a JSON-compatible dict of all coordinator state for serialization/introspection.
    ///
    /// Returns a `HashMap` with keys: `tools`, `providers`, `has_orchestrator`,
    /// `has_context`, `capabilities` — matching the universal Coordinator API.
    pub fn to_dict(&self) -> HashMap<String, serde_json::Value> {
        let mut dict = HashMap::new();
        dict.insert(
            "tools".to_string(),
            serde_json::json!(self.tool_names()),
        );
        dict.insert(
            "providers".to_string(),
            serde_json::json!(self.provider_names()),
        );
        dict.insert(
            "has_orchestrator".to_string(),
            serde_json::json!(self.has_orchestrator()),
        );
        dict.insert(
            "has_context".to_string(),
            serde_json::json!(self.has_context()),
        );
        dict.insert(
            "capabilities".to_string(),
            serde_json::json!(self.capability_names()),
        );
        dict
    }
```

**Step 4: Run the new tests**
```bash
cargo test -p amplifier-core -- coordinator::tests::to_dict
```
Expected: Both `to_dict_includes_all_mount_points` and `to_dict_reflects_mounted_state` PASS.

**Step 5: Run full suite + clippy**
```bash
cargo test -p amplifier-core && cargo clippy -p amplifier-core -- -D warnings
```
Expected: All tests pass. No clippy warnings.

**Step 6: Commit**
```bash
git add crates/amplifier-core/src/coordinator.rs && git commit -m "feat: add Coordinator::to_dict() — universal introspection method"
```

---

### Task 3: Fix `Session::cleanup()` to clear initialized flag

**Files:**
- Modify: `crates/amplifier-core/src/session.rs`

**Context:** Currently `initialized` is a plain `bool` field. `set_initialized()` and `clear_initialized()` take `&mut self`. But `cleanup()` takes `&self`, so it can't call `clear_initialized()`. The fix is to change `initialized` to `AtomicBool` for interior mutability, then update all accessors to `&self` and add `self.clear_initialized()` to `cleanup()`.

**Step 1: Write the failing test**

Add this test to the existing `#[cfg(test)] mod tests` block at the bottom of `crates/amplifier-core/src/session.rs` (before the closing `}` of the `mod tests` block at line 703):

```rust
    #[tokio::test]
    async fn cleanup_clears_initialized_flag() {
        let config = SessionConfig::minimal("test-orch", "test-ctx");
        let mut session = Session::new(config, None, None);
        session.set_initialized();
        assert!(session.is_initialized());
        session.cleanup().await;
        assert!(
            !session.is_initialized(),
            "cleanup should clear initialized flag"
        );
    }
```

**Step 2: Run to verify it fails**
```bash
cargo test -p amplifier-core -- session::tests::cleanup_clears_initialized
```
Expected: FAIL — `cleanup()` doesn't clear the flag currently (it only calls `coordinator.cleanup()`).

**Step 3: Add AtomicBool import**

At the top of `crates/amplifier-core/src/session.rs`, add the `AtomicBool` import. Find the existing imports (line 20):
```rust
use std::collections::HashMap;
```
Change to:
```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
```

**Step 4: Change the `initialized` field type**

In the `Session` struct definition (line 120-127), change:
```rust
pub struct Session {
    session_id: String,
    parent_id: Option<String>,
    coordinator: Coordinator,
    initialized: bool,
    status: SessionState,
    is_resumed: bool,
}
```
to:
```rust
pub struct Session {
    session_id: String,
    parent_id: Option<String>,
    coordinator: Coordinator,
    initialized: AtomicBool,
    status: SessionState,
    is_resumed: bool,
}
```

**Step 5: Update `Session::new()` initialization**

In the `new()` method (line 151-158), change:
```rust
        Self {
            session_id: id,
            parent_id,
            coordinator,
            initialized: false,
            status: SessionState::Running,
            is_resumed: false,
        }
```
to:
```rust
        Self {
            session_id: id,
            parent_id,
            coordinator,
            initialized: AtomicBool::new(false),
            status: SessionState::Running,
            is_resumed: false,
        }
```

**Step 6: Update `is_initialized()`**

Change (line 198-200):
```rust
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
```
to:
```rust
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }
```

**Step 7: Update `set_initialized()` to use `&self`**

Change (line 217-219):
```rust
    pub fn set_initialized(&mut self) {
        self.initialized = true;
    }
```
to:
```rust
    pub fn set_initialized(&self) {
        self.initialized.store(true, Ordering::Relaxed);
    }
```

**Step 8: Update `clear_initialized()` to use `&self`**

Change (line 224-226):
```rust
    pub fn clear_initialized(&mut self) {
        self.initialized = false;
    }
```
to:
```rust
    pub fn clear_initialized(&self) {
        self.initialized.store(false, Ordering::Relaxed);
    }
```

**Step 9: Update `execute()` initialized check**

In `execute()` (line 242), change:
```rust
        if !self.initialized {
```
to:
```rust
        if !self.is_initialized() {
```

**Step 10: Add `self.clear_initialized()` to `cleanup()`**

In the `cleanup()` method (lines 327-342), add `self.clear_initialized();` at the end:
```rust
    pub async fn cleanup(&self) {
        // Emit session:end event
        self.coordinator
            .hooks()
            .emit(
                events::SESSION_END,
                serde_json::json!({
                    "session_id": self.session_id,
                    "status": self.status(),
                }),
            )
            .await;

        // Run coordinator cleanup
        self.coordinator.cleanup().await;

        // Clear initialized flag so session cannot be re-executed
        self.clear_initialized();
    }
```

**Step 11: Run the new test**
```bash
cargo test -p amplifier-core -- session::tests::cleanup_clears_initialized
```
Expected: PASS.

**Step 12: Run full suite + clippy**
```bash
cargo test -p amplifier-core && cargo clippy -p amplifier-core -- -D warnings
```
Expected: ALL 251+ tests pass. The `AtomicBool` change is internal — `&self` is less restrictive than `&mut self` so all existing callers still compile. Clippy clean.

**Step 13: Commit**
```bash
git add crates/amplifier-core/src/session.rs && git commit -m "fix: cleanup() now clears initialized flag via AtomicBool

Changed initialized field from bool to AtomicBool for interior
mutability, allowing cleanup(&self) to clear the flag without
requiring &mut self. Matches Python bridge behavior."
```

---

### Task 4: Thread real hooks and coordinator state through `Session::execute()`

**Files:**
- Modify: `crates/amplifier-core/src/testing.rs` (add `CapturingOrchestrator`)
- Modify: `crates/amplifier-core/src/session.rs` (update `execute()`)

**Context:** Currently `execute()` passes `serde_json::json!({})` for both the hooks and coordinator arguments to the orchestrator (lines 298-299). We should pass serialized representations of the actual hooks list and coordinator state. We also need a test fake that captures what was passed to it.

**Step 1: Add `CapturingOrchestrator` to testing.rs**

In `crates/amplifier-core/src/testing.rs`, add this new struct after the `FakeOrchestrator` implementation (after line 405, before the `FakeApprovalProvider` section):

```rust
// ---------------------------------------------------------------------------
// CapturingOrchestrator
// ---------------------------------------------------------------------------

/// An orchestrator that captures the hooks and coordinator values passed to it.
///
/// Returns a pre-configured response (like `FakeOrchestrator`) but also
/// stores the `hooks` and `coordinator` `Value` arguments for test assertions.
pub struct CapturingOrchestrator {
    response: String,
    last_hooks: Mutex<Value>,
    last_coordinator: Mutex<Value>,
}

impl CapturingOrchestrator {
    /// Create a capturing orchestrator that returns `response`.
    pub fn new(response: &str) -> Self {
        Self {
            response: response.into(),
            last_hooks: Mutex::new(Value::Null),
            last_coordinator: Mutex::new(Value::Null),
        }
    }

    /// The last `hooks` value passed to `execute()`.
    pub fn last_hooks_value(&self) -> Value {
        self.last_hooks.lock().unwrap().clone()
    }

    /// The last `coordinator` value passed to `execute()`.
    pub fn last_coordinator_value(&self) -> Value {
        self.last_coordinator.lock().unwrap().clone()
    }
}

impl Orchestrator for CapturingOrchestrator {
    fn execute(
        &self,
        _prompt: String,
        _context: Arc<dyn ContextManager>,
        _providers: HashMap<String, Arc<dyn Provider>>,
        _tools: HashMap<String, Arc<dyn Tool>>,
        hooks: Value,
        coordinator: Value,
    ) -> Pin<Box<dyn Future<Output = Result<String, AmplifierError>> + Send + '_>> {
        *self.last_hooks.lock().unwrap() = hooks;
        *self.last_coordinator.lock().unwrap() = coordinator;
        let resp = self.response.clone();
        Box::pin(async move { Ok(resp) })
    }
}
```

**Step 2: Write the failing test in session.rs**

Add this test to the existing `#[cfg(test)] mod tests` block in `crates/amplifier-core/src/session.rs`. First, update the `use` import at the top of the test module. Find (line 352-355):
```rust
    use crate::testing::{
        FakeContextManager, FakeHookHandler, FakeOrchestrator, FakeProvider, FakeTool,
    };
```
Change to:
```rust
    use crate::testing::{
        CapturingOrchestrator, FakeContextManager, FakeHookHandler, FakeOrchestrator, FakeProvider,
        FakeTool,
    };
```

Then add this test (before the closing `}` of the `mod tests` block):

```rust
    #[tokio::test]
    async fn execute_passes_hooks_and_coordinator_to_orchestrator() {
        let config = SessionConfig::minimal("loop-basic", "context-simple");
        let mut session = Session::new(config, None, None);

        let orch = Arc::new(CapturingOrchestrator::new("test response"));
        session.coordinator_mut().set_orchestrator(orch.clone());
        session
            .coordinator_mut()
            .set_context(Arc::new(FakeContextManager::new()));
        session
            .coordinator_mut()
            .mount_provider("test", Arc::new(FakeProvider::new("test", "hi")));
        session
            .coordinator_mut()
            .mount_tool("echo", Arc::new(FakeTool::new("echo", "echoes")));
        session.set_initialized();

        let result = session.execute("hello").await;
        assert!(result.is_ok());

        // Verify the orchestrator received non-empty coordinator data
        let captured_coord = orch.last_coordinator_value();
        assert_ne!(
            captured_coord,
            serde_json::json!({}),
            "coordinator value should not be empty"
        );
        // Verify the coordinator value contains tools
        assert!(
            captured_coord.get("tools").is_some(),
            "coordinator value should contain 'tools' key"
        );
    }
```

**Step 3: Run to verify it fails**
```bash
cargo test -p amplifier-core -- session::tests::execute_passes_hooks_and_coordinator
```
Expected: FAIL — currently `execute()` passes `json!({})` so `captured_coord` is `{}`.

**Step 4: Update `Session::execute()` to pass real data**

In `crates/amplifier-core/src/session.rs`, find the orchestrator call in `execute()` (lines 292-301):
```rust
        match orchestrator
            .execute(
                prompt.to_string(),
                context,
                providers,
                tools,
                serde_json::json!({}), // hooks placeholder (serialised)
                serde_json::json!({}), // coordinator placeholder (serialised)
            )
            .await
```

Replace with:
```rust
        // Serialize hooks handler list and coordinator state for the orchestrator
        let hooks_value = serde_json::to_value(self.coordinator.hooks().list_handlers(None))
            .unwrap_or(serde_json::json!({}));
        let coordinator_value = serde_json::to_value(self.coordinator.to_dict())
            .unwrap_or(serde_json::json!({}));

        match orchestrator
            .execute(
                prompt.to_string(),
                context,
                providers,
                tools,
                hooks_value,
                coordinator_value,
            )
            .await
```

**Step 5: Run the new test**
```bash
cargo test -p amplifier-core -- session::tests::execute_passes_hooks_and_coordinator
```
Expected: PASS.

**Step 6: Run full suite + clippy**
```bash
cargo test -p amplifier-core && cargo clippy -p amplifier-core -- -D warnings
```
Expected: All tests pass. Clippy clean.

**Step 7: Commit**
```bash
git add crates/amplifier-core/src/session.rs crates/amplifier-core/src/testing.rs && git commit -m "feat: thread real hooks and coordinator state through execute()

Orchestrator now receives serialized hook handler list and coordinator
mount-point state instead of empty json!({}) placeholders. Added
CapturingOrchestrator test fake for verifying passed values."
```

---

### Task 5: Add `SessionConfig::from_json()` convenience constructor

**Files:**
- Modify: `crates/amplifier-core/src/session.rs`

**Step 1: Write the failing tests**

Add these tests to the existing `#[cfg(test)] mod tests` block in `crates/amplifier-core/src/session.rs` (before the closing `}` of the `mod tests` block):

```rust
    #[test]
    fn session_config_from_json_string() {
        let json = r#"{
            "session": {
                "orchestrator": "loop-basic",
                "context": "context-simple"
            }
        }"#;
        let config = SessionConfig::from_json(json).unwrap();
        // Verify the parsed config contains the expected values
        let session_block = config.config.get("session").unwrap();
        assert_eq!(session_block["orchestrator"], "loop-basic");
        assert_eq!(session_block["context"], "context-simple");
    }

    #[test]
    fn session_config_from_json_invalid() {
        let result = SessionConfig::from_json("not json");
        assert!(result.is_err());
    }
```

**Step 2: Run to verify they fail**
```bash
cargo test -p amplifier-core -- session::tests::session_config_from_json
```
Expected: FAIL — `from_json` method doesn't exist yet.

**Step 3: Implement `from_json()`**

In `crates/amplifier-core/src/session.rs`, add this method to the `impl SessionConfig` block. Insert it after the `from_value()` method (after line 78, before the `minimal()` method):

```rust
    /// Parse a `SessionConfig` from a JSON string.
    ///
    /// Convenience constructor for Rust applications that have config as a
    /// JSON string rather than a `serde_json::Value`.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::Other` if the string is not valid JSON, or
    /// the usual validation errors from [`from_value`](Self::from_value).
    pub fn from_json(json: &str) -> Result<Self, SessionError> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
            SessionError::Other {
                message: format!("invalid JSON: {e}"),
            }
        })?;
        Self::from_value(value)
    }
```

**Step 4: Run the new tests**
```bash
cargo test -p amplifier-core -- session::tests::session_config_from_json
```
Expected: Both `session_config_from_json_string` and `session_config_from_json_invalid` PASS.

**Step 5: Run full suite + clippy**
```bash
cargo test -p amplifier-core && cargo clippy -p amplifier-core -- -D warnings
```
Expected: All tests pass. Clippy clean.

**Step 6: Commit**
```bash
git add crates/amplifier-core/src/session.rs && git commit -m "feat: add SessionConfig::from_json() convenience constructor"
```

---

### Task 6: Integration test — Rust-native AmplifierSession lifecycle

**Files:**
- Create: `crates/amplifier-core/tests/session_lifecycle_e2e.rs`

This end-to-end test proves the universal API works from pure Rust, exercising all the features added in Tasks 1-5.

**Step 1: Create the integration test file**

Create `crates/amplifier-core/tests/session_lifecycle_e2e.rs` with this content:

```rust
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
```

**Step 2: Run the integration tests**
```bash
cargo test -p amplifier-core --test session_lifecycle_e2e -- --nocapture
```
Expected: All 4 tests PASS.

**Step 3: Full suite verification (Rust + Python)**
```bash
cargo test -p amplifier-core && cargo clippy -p amplifier-core -- -D warnings && cargo fmt -p amplifier-core --check
```
Expected: All tests pass. Clippy clean. Format clean.

**Step 4: Run Python tests to verify invariant**
```bash
cd amplifier-core && maturin develop && uv run pytest tests/ bindings/python/tests/ -q --tb=short
```
Expected: ALL existing Python tests pass unchanged.

**Step 5: Commit**
```bash
git add crates/amplifier-core/tests/session_lifecycle_e2e.rs && git commit -m "test: add AmplifierSession lifecycle integration test — pure Rust

Proves the universal API works: AmplifierSession alias, Coordinator
to_dict, SessionConfig::from_json, native tool mounting, cleanup
clears initialized.

Phase 1 complete."
```

---

## Summary

| Task | What | Files | New Tests |
|------|------|-------|-----------|
| 1 | `AmplifierSession` type alias | `src/lib.rs` | 0 (compile-only) |
| 2 | `Coordinator::to_dict()` | `src/coordinator.rs` | 2 |
| 3 | `cleanup()` clears initialized (`AtomicBool`) | `src/session.rs` | 1 |
| 4 | Thread real hooks/coordinator through `execute()` | `src/session.rs`, `src/testing.rs` | 1 |
| 5 | `SessionConfig::from_json()` convenience | `src/session.rs` | 2 |
| 6 | Integration test — full lifecycle | `tests/session_lifecycle_e2e.rs` | 4 |

**Invariant:** All existing Python and Rust tests pass unchanged at every step.

**Total new tests:** 10

**Total tasks:** 6 (bite-sized, ~2-5 minutes each)

**All file paths are relative to `crates/amplifier-core/`.**
