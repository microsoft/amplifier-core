# PR #39 Code Review Fixes — Implementation Plan

## Goal

Address all Critical and High priority findings from the PR #39 code review
before merge (Critical) and before production deployment (High), ensuring the
cross-language SDK is secure, correct, and production-ready.

## Background

Four independent review agents (architecture, Rust quality, security audit,
bindings/coverage) converged on 6 Critical and 7 High findings in the
+34,727-line cross-language SDK PR. The code has zero compiler errors, zero
clippy warnings, and strong test coverage — the findings are all design-level
and semantic issues that static analysis cannot catch.

Critical items are merge-blocking. High items block production deployment.
Medium, Low, and Test Gap items are follow-up work.

## Approach

Work through findings in dependency order:

1. **C-02 + H-01** — WASM resource limits + WASI restriction (shared `create_linker_and_store`)
2. **C-03** — Hook fail-closed (quick, high-impact security fix)
3. **C-05** — Streaming endpoint fix (quick, isolated)
4. **C-06** — Guest SDK compile gate (quick, isolated)
5. **C-01** — gRPC authentication (larger, foundational for H-04)
6. **C-04** — Node.js detached instance fix (interim rename + tests)
7. **H-02** — Sanitize gRPC errors (12 sites, mechanical)
8. **H-03** — Path traversal check
9. **H-04** — Session ID routing (document limitation, depends on C-01)
10. **H-05** — WIT HTTP import removal
11. **H-06** — TypeScript type fixes
12. **H-07** — JSON payload size limits

Each task follows TDD: write the failing test first, then implement the fix,
then verify all tests pass.

---

## Task 1: C-03 — Hook Parse Failure Must Fail Closed (Deny, Not Continue)

**Priority:** Critical — security controls can be silently bypassed
**Estimated effort:** 30 minutes
**Files:**
- `bindings/node/src/lib.rs` (line 353)
- `bindings/python/src/lib.rs` (line 172)
- `crates/amplifier-core/tests/` (new test)

### 1a. Write failing test — Node bindings

Add a test in `bindings/node/__tests__/hooks.test.ts` that registers a hook
returning invalid JSON and asserts the result action is `"deny"`:

```typescript
it('returns Deny when hook handler returns invalid JSON (fail-closed)', async () => {
  const registry = new JsHookRegistry()
  registry.register(
    'tool:pre',
    (_event: string, _data: string) => 'NOT VALID JSON {{{',
    10,
    'bad-json-hook'
  )
  const result = await registry.emit('tool:pre', '{}')
  expect(result.action).toBe('deny')
  expect(result.reason).toContain('invalid')
})
```

### 1b. Fix Node bindings — fail closed

In `bindings/node/src/lib.rs`, replace the `unwrap_or_else` at line 353:

```rust
// BEFORE (line 353-358):
let hook_result: HookResult = serde_json::from_str(&result_str).unwrap_or_else(|e| {
    eprintln!(
        "amplifier-core-node: failed to parse HookResult from JS handler: {e}. Defaulting to Continue."
    );
    HookResult::default()
});

// AFTER:
let hook_result: HookResult = serde_json::from_str(&result_str).unwrap_or_else(|e| {
    log::error!(
        "SECURITY: Hook handler returned unparseable result — failing closed (Deny): {e}"
    );
    HookResult {
        action: HookAction::Deny,
        reason: Some("Hook handler returned invalid response".to_string()),
        ..Default::default()
    }
});
```

### 1c. Fix Python bindings — fail closed

In `bindings/python/src/lib.rs`, replace the `unwrap_or_else` at line 172:

```rust
// BEFORE (line 172-177):
let hook_result: HookResult = serde_json::from_str(&result_json).unwrap_or_else(|e| {
    log::warn!(
        "Failed to parse hook handler result JSON (defaulting to Continue): {e} — json: {result_json}"
    );
    HookResult::default()
});

// AFTER:
let hook_result: HookResult = serde_json::from_str(&result_json).unwrap_or_else(|e| {
    log::error!(
        "SECURITY: Hook handler returned unparseable result — failing closed (Deny): {e} — json: {result_json}"
    );
    HookResult {
        action: HookAction::Deny,
        reason: Some("Hook handler returned invalid response".to_string()),
        ..Default::default()
    }
});
```

### 1d. Verify

```bash
cd bindings/node && npm test -- --grep "fail-closed"
cd crates/amplifier-core && cargo test
```

---

## Task 2: C-05 — Fix Fake Streaming Endpoint

**Priority:** Critical — silent send failure + misleading contract
**Estimated effort:** 30 minutes
**File:** `crates/amplifier-core/src/grpc_server.rs` (lines 99-107)

### 2a. Write failing test

Add a test in `crates/amplifier-core/src/grpc_server.rs` `mod tests` that
verifies the streaming endpoint logs on send failure (or at minimum, add
a test that exercises the streaming path and asserts it produces exactly
one response):

```rust
#[tokio::test]
async fn streaming_endpoint_returns_single_response() {
    use crate::testing::FakeProvider;
    let coord = Arc::new(Coordinator::new(Default::default()));
    let provider = Arc::new(FakeProvider::new("test-provider"));
    coord.mount_provider(provider);
    let service = KernelServiceImpl::new(coord);

    let request = Request::new(amplifier_module::CompleteWithProviderRequest {
        provider_name: "test-provider".to_string(),
        request: Some(/* minimal ChatRequest */),
    });

    let response = service.complete_with_provider_streaming(request).await.unwrap();
    let mut stream = response.into_inner();

    let first = stream.next().await;
    assert!(first.is_some(), "Should yield exactly one response");
    let second = stream.next().await;
    assert!(second.is_none(), "Stream should end after single response");
}
```

### 2b. Fix: log send failure + add doc comment

In `crates/amplifier-core/src/grpc_server.rs`, replace lines 99-107:

```rust
// BEFORE:
// Wrap in a one-shot stream: send the single response then drop the sender
// to signal end-of-stream to the client.
let (tx, rx) = tokio::sync::mpsc::channel(1);
let _ = tx.send(Ok(proto_response)).await;
// `tx` is dropped here, closing the channel and ending the stream.

// AFTER:
// NOTE: This is a one-shot "streaming" endpoint — it awaits the full provider
// response, then sends it as a single stream element. True token-level streaming
// requires provider.complete_stream() → Stream<Item = ChatResponse>, which is
// not yet implemented. This endpoint exists for proto/gRPC API compatibility
// so clients can use the streaming RPC shape ahead of the real implementation.
let (tx, rx) = tokio::sync::mpsc::channel(1);
if tx.send(Ok(proto_response)).await.is_err() {
    log::debug!("Streaming client disconnected before response was sent");
}
// `tx` is dropped here, closing the channel and ending the stream.
```

### 2c. Verify

```bash
cd crates/amplifier-core && cargo test grpc_server
```

---

## Task 3: C-06 — Guest SDK Kernel Stubs Compile Gate

**Priority:** Critical — runtime failure instead of compile-time failure
**Estimated effort:** 30 minutes
**Files:**
- `crates/amplifier-guest/src/lib.rs` (line 547-591)
- `crates/amplifier-guest/Cargo.toml`

### 3a. Add `kernel-stub` feature to Cargo.toml

In `crates/amplifier-guest/Cargo.toml`:

```toml
[features]
default = ["kernel-stub"]
kernel-stub = []
```

### 3b. Add compile_error! gate

In `crates/amplifier-guest/src/lib.rs`, above the `pub mod kernel` block
(before line 553):

```rust
#[cfg(all(target_arch = "wasm32", not(feature = "kernel-stub")))]
compile_error!(
    "kernel:: functions are not yet wired to WIT imports. \
     Set feature = 'kernel-stub' for testing only."
);
```

### 3c. Verify

```bash
# Normal build (with default kernel-stub feature) should pass:
cd crates/amplifier-guest && cargo check

# Existing tests should still pass:
cargo test -p amplifier-guest
```

---

## Task 4: C-02 + H-01 — WASM Resource Limits + Restrict WASI Capabilities

**Priority:** Critical (C-02) + High (H-01) — DoS via infinite loop / OOM + information leakage
**Estimated effort:** 4-6 hours
**Files:**
- `crates/amplifier-core/src/bridges/wasm_tool.rs` (lines 54-64)
- `crates/amplifier-core/src/bridges/mod.rs`
- All `wasm_*.rs` bridge files (they all import `create_linker_and_store`)
- `crates/amplifier-core/Cargo.toml` (no new deps needed)

### 4a. Write failing test — epoch interruption kills infinite loop

Create `crates/amplifier-core/tests/wasm_resource_limits.rs`:

```rust
//! Verifies that WASM modules with infinite loops are terminated
//! by epoch interruption and do not hang indefinitely.

#[cfg(feature = "wasm")]
#[tokio::test]
async fn infinite_loop_wasm_module_is_terminated() {
    use std::time::{Duration, Instant};
    // Load the infinite-loop test fixture (a WASM component that runs `loop {}`)
    let engine = amplifier_core::bridges::wasm_tool::create_wasm_engine().unwrap();
    let bytes = std::fs::read("tests/fixtures/wasm/infinite-loop/module.wasm").unwrap();

    let start = Instant::now();
    let result = amplifier_core::bridges::wasm_tool::WasmToolBridge::from_bytes(
        &bytes, engine,
    );
    let elapsed = start.elapsed();

    // Should fail (trap), not hang forever
    assert!(result.is_err(), "Infinite loop should be trapped");
    assert!(elapsed < Duration::from_secs(60), "Should terminate within timeout");
}
```

### 4b. Create engine factory with epoch interruption

The shared engine is created implicitly via `Engine::new(&config)` at each
bridge's construction site. Centralize this into `bridges/mod.rs`:

```rust
// bridges/mod.rs — add:

#[cfg(feature = "wasm")]
use std::sync::Arc;
#[cfg(feature = "wasm")]
use wasmtime::Engine;

/// Default WASM execution limits.
#[cfg(feature = "wasm")]
pub struct WasmLimits {
    /// Maximum epoch ticks before trap (at ~100 ticks/sec, 3000 = 30 seconds).
    pub max_epoch_ticks: u64,
    /// Maximum memory in bytes (default: 64 MB).
    pub max_memory_bytes: usize,
}

#[cfg(feature = "wasm")]
impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            max_epoch_ticks: 3000,       // ~30 seconds at 100Hz
            max_memory_bytes: 64 << 20,  // 64 MB
        }
    }
}

/// Create a wasmtime Engine with epoch interruption enabled and a background
/// ticker thread that increments the epoch every 10ms (~100Hz).
#[cfg(feature = "wasm")]
pub fn create_wasm_engine() -> Result<Arc<Engine>, Box<dyn std::error::Error + Send + Sync>> {
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    config.epoch_interruption(true);
    let engine = Arc::new(Engine::new(&config)?);

    // Background ticker — increments epoch every 10ms
    let engine_clone = Arc::clone(&engine);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(10));
            engine_clone.increment_epoch();
        }
    });

    Ok(engine)
}
```

### 4c. Update `create_linker_and_store` — add limits + null I/O

In `crates/amplifier-core/src/bridges/wasm_tool.rs`, update the shared function:

```rust
// BEFORE (lines 54-64):
pub(crate) fn create_linker_and_store(
    engine: &Engine,
) -> Result<(Linker<WasmState>, Store<WasmState>), Box<dyn std::error::Error + Send + Sync>> {
    let mut linker = Linker::<WasmState>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    let wasi = wasmtime_wasi::WasiCtxBuilder::new().build();
    let table = wasmtime::component::ResourceTable::new();
    let store = Store::new(engine, WasmState { wasi, table });
    Ok((linker, store))
}

// AFTER:
pub(crate) fn create_linker_and_store(
    engine: &Engine,
    limits: &super::WasmLimits,
) -> Result<(Linker<WasmState>, Store<WasmState>), Box<dyn std::error::Error + Send + Sync>> {
    let mut linker = Linker::<WasmState>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

    // H-01: Restrict WASI capabilities — null I/O, no inherited env/args
    let wasi = wasmtime_wasi::WasiCtxBuilder::new()
        .stdin(wasmtime_wasi::pipe::ClosedInputStream)
        .stdout(wasmtime_wasi::pipe::SinkOutputStream)
        .stderr(wasmtime_wasi::pipe::SinkOutputStream)
        .build();

    let table = wasmtime::component::ResourceTable::new();
    let mut store = Store::new(engine, WasmState { wasi, table });

    // C-02: CPU time limit via epoch interruption
    store.set_epoch_deadline(limits.max_epoch_ticks);

    // C-02: Memory limit via StoreLimitsBuilder
    store.limiter(|state| &mut state.limiter);

    Ok((linker, store))
}
```

Update `WasmState` to include the limiter:

```rust
pub(crate) struct WasmState {
    wasi: wasmtime_wasi::WasiCtx,
    table: wasmtime::component::ResourceTable,
    limiter: wasmtime::StoreLimits,
}
```

### 4d. Update all call sites

Every file that calls `create_linker_and_store(engine)` must pass the limits
parameter. These files import from `super::wasm_tool::create_linker_and_store`:

- `wasm_hook.rs:70` → `create_linker_and_store(engine, &WasmLimits::default())`
- `wasm_context.rs:95` → same
- `wasm_approval.rs:72` → same
- `wasm_orchestrator.rs:372` → same
- `wasm_provider.rs:81,93,114,136` → same
- `wasm_tool.rs:104,118` → same

### 4e. Verify

```bash
cd crates/amplifier-core && cargo test --features wasm
cargo clippy --features wasm
```

---

## Task 5: C-01 — Add Authentication to KernelService gRPC Server

**Priority:** Critical — all 9 RPCs completely unauthenticated
**Estimated effort:** 4-6 hours
**Files:**
- `crates/amplifier-core/src/grpc_server.rs`
- `crates/amplifier-core/src/lib.rs` (re-export)

### 5a. Write failing test — unauthenticated request is rejected

In `crates/amplifier-core/src/grpc_server.rs` `mod tests`:

```rust
#[tokio::test]
async fn unauthenticated_request_is_rejected() {
    let coord = Arc::new(Coordinator::new(Default::default()));
    let service = KernelServiceImpl::new(coord);
    let interceptor = auth_interceptor("test-secret-token");

    // Request WITHOUT the auth token
    let request = Request::new(amplifier_module::EmitHookRequest {
        event: "test:event".to_string(),
        data_json: "{}".to_string(),
    });

    let result = interceptor(request);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn authenticated_request_is_accepted() {
    let token = "test-secret-token";
    let mut request = Request::new(());
    request.metadata_mut().insert(
        "x-amplifier-token",
        token.parse().unwrap(),
    );

    let interceptor = auth_interceptor(token);
    let result = interceptor(request);
    assert!(result.is_ok());
}
```

### 5b. Implement shared-secret interceptor

Add to `crates/amplifier-core/src/grpc_server.rs`:

```rust
use tonic::service::Interceptor;

/// Creates a tonic interceptor that validates a shared secret token.
///
/// The token must be passed via the `x-amplifier-token` metadata header.
/// If the token is missing or does not match, the request is rejected with
/// `UNAUTHENTICATED`.
pub fn auth_interceptor(
    expected_token: impl Into<String>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    let expected = expected_token.into();
    move |req: Request<()>| {
        let token = req
            .metadata()
            .get("x-amplifier-token")
            .and_then(|v| v.to_str().ok());
        match token {
            Some(t) if t == expected => Ok(req),
            _ => Err(Status::unauthenticated("missing or invalid token")),
        }
    }
}
```

### 5c. Wire interceptor into server construction

Update the public API for constructing the gRPC server (wherever
`KernelServiceServer::new(service)` is called) to use
`KernelServiceServer::with_interceptor(service, auth_interceptor(token))`.

Add a builder method to `KernelServiceImpl`:

```rust
impl KernelServiceImpl {
    /// Build a tonic Router with authentication enabled.
    ///
    /// `token` is the shared secret that out-of-process modules must include
    /// in the `x-amplifier-token` gRPC metadata header.
    pub fn into_router(
        self,
        token: &str,
    ) -> tonic::transport::server::Router {
        let svc = amplifier_module::kernel_service_server::KernelServiceServer::with_interceptor(
            self,
            auth_interceptor(token),
        );
        tonic::transport::Server::builder().add_service(svc)
    }
}
```

### 5d. Generate token and pass to child modules

Token generation: use `uuid::Uuid::new_v4().to_string()` as the per-session
shared secret. The token is generated when the gRPC server starts and passed
to child module processes via the `AMPLIFIER_TOKEN` environment variable.

### 5e. Verify

```bash
cd crates/amplifier-core && cargo test grpc_server
```

---

## Task 6: C-04 — Node.js Detached Instance Fix (Interim)

**Priority:** Critical — entire Node.js hook system is non-functional
**Estimated effort:** 1-2 hours (interim rename + warnings + tests)
**Files:**
- `bindings/node/src/lib.rs` (lines 535-541, 648-661)
- `bindings/node/index.d.ts` (lines 157, 190)
- `bindings/node/__tests__/` (new tests)

### 6a. Write tests pinning detached-instance behavior

In `bindings/node/__tests__/coordinator.test.ts`:

```typescript
it('createHookRegistry() creates a new instance each call', () => {
  const coord = new JsCoordinator('{}')
  const h1 = coord.createHookRegistry()
  const h2 = coord.createHookRegistry()
  expect(h1).not.toBe(h2) // pins known detached behavior
})
```

In `bindings/node/__tests__/session.test.ts`:

```typescript
it('createCoordinator() creates a new instance each call', () => {
  const session = new JsAmplifierSession('{}')
  const c1 = session.createCoordinator()
  const c2 = session.createCoordinator()
  expect(c1).not.toBe(c2) // pins known detached behavior
})
```

### 6b. Rename getters to factory methods

In `bindings/node/src/lib.rs`:

**JsCoordinator — rename `hooks` getter to `createHookRegistry` method:**

```rust
// BEFORE (line 538-540):
#[napi(getter)]
pub fn hooks(&self) -> JsHookRegistry {
    JsHookRegistry::new_detached()
}

// AFTER:
/// Creates a new, detached HookRegistry instance.
///
/// WARNING: Each call returns a separate, empty registry — handlers registered
/// on one instance are NOT visible to another. Cache the result if you need
/// a shared registry. Structural fix tracked as Future TODO #1.
#[napi]
pub fn create_hook_registry(&self) -> JsHookRegistry {
    log::warn!(
        "JsCoordinator.createHookRegistry(): returns a detached instance. \
         Cache the result — each call creates a new empty registry."
    );
    JsHookRegistry::new_detached()
}
```

**JsAmplifierSession — rename `coordinator` getter to `createCoordinator` method:**

```rust
// BEFORE (line 656-661):
#[napi(getter)]
pub fn coordinator(&self) -> JsCoordinator {
    JsCoordinator {
        inner: Arc::new(amplifier_core::Coordinator::new(self.cached_config.clone())),
    }
}

// AFTER:
/// Creates a new Coordinator instance from cached config.
///
/// WARNING: Each call allocates a fresh Coordinator — state is NOT shared
/// between instances. Cache the result. Structural fix tracked as Future TODO #1.
#[napi]
pub fn create_coordinator(&self) -> JsCoordinator {
    log::warn!(
        "JsAmplifierSession.createCoordinator(): returns a new instance. \
         Cache the result — state is not shared between instances."
    );
    JsCoordinator {
        inner: Arc::new(amplifier_core::Coordinator::new(self.cached_config.clone())),
    }
}
```

### 6c. Update TypeScript type definitions

In `bindings/node/index.d.ts`, replace the getter declarations:

```typescript
// In JsCoordinator (was: get hooks(): JsHookRegistry):
/**
 * Creates a new, detached HookRegistry instance.
 *
 * WARNING: Each call returns a separate empty registry. Cache the result.
 */
createHookRegistry(): JsHookRegistry

// In JsAmplifierSession (was: get coordinator(): JsCoordinator):
/**
 * Creates a new Coordinator from cached config.
 *
 * WARNING: Each call creates a fresh instance. Cache the result.
 */
createCoordinator(): JsCoordinator
```

### 6d. Update any existing tests/examples that use the old getter names

Search for `coord.hooks`, `session.coordinator`, `.hooks.` in test files
and update to the new method names.

### 6e. Verify

```bash
cd bindings/node && npm test
```

---

## Task 7: H-02 — Sanitize gRPC Error Messages (12 Sites)

**Priority:** High — internal error details exposed to callers
**Estimated effort:** 1-2 hours
**File:** `crates/amplifier-core/src/grpc_server.rs` — 12 sites

### 7a. Write test — error messages do not contain internal details

```rust
#[tokio::test]
async fn tool_execution_error_does_not_leak_internals() {
    use crate::testing::FailingTool;
    let coord = Arc::new(Coordinator::new(Default::default()));
    let tool = Arc::new(FailingTool::new("secret-tool", "internal DB error at /var/lib/data.db"));
    coord.mount_tool(tool);
    let service = KernelServiceImpl::new(coord);

    let request = Request::new(amplifier_module::ExecuteToolRequest {
        tool_name: "secret-tool".to_string(),
        input_json: "{}".to_string(),
    });

    let err = service.execute_tool(request).await.unwrap_err();
    let msg = err.message();
    assert!(!msg.contains("/var/lib"), "Error should not contain file paths");
    assert!(!msg.contains("DB error"), "Error should not contain internal details");
}
```

### 7b. Fix all 12 sites

Pattern: log the full error server-side, return a generic message to the caller.

| Line | Current | Fixed |
|------|---------|-------|
| 47 | `Status::not_found(format!("Provider not mounted: {provider_name}"))` | `log::debug!("Provider lookup failed: {provider_name}"); Status::not_found("Provider not available")` |
| 63 | `Status::internal(format!("Provider completion failed: {e}"))` | `log::error!("Provider completion failed for {provider_name}: {e}"); Status::internal("Provider completion failed")` |
| 95 | `Status::internal(format!("Provider completion failed: {e}"))` | Same pattern as line 63 |
| 121 | `Status::not_found(format!("Tool not found: {tool_name}"))` | `log::debug!("Tool lookup failed: {tool_name}"); Status::not_found("Tool not available")` |
| 125 | `Status::invalid_argument(format!("Invalid input JSON: {e}"))` | `Status::invalid_argument("Invalid input JSON")` (safe — no internals) |
| 154 | `Status::internal(format!("Tool execution failed: {e}"))` | `log::error!("Tool execution failed for {tool_name}: {e}"); Status::internal("Tool execution failed")` |
| 168 | `Status::invalid_argument(format!("Invalid data_json: {e}"))` | `Status::invalid_argument("Invalid data_json")` |
| 186 | Same pattern | Same fix |
| 228 | `Status::internal(format!("Failed to get messages: {e}"))` | `log::error!("Failed to get messages: {e}"); Status::internal("Failed to get messages")` |
| 259 | `Status::invalid_argument(format!("Invalid message: {e}"))` | `Status::invalid_argument("Invalid message format")` |
| 262 | `Status::internal(format!("Failed to serialize message: {e}"))` | `log::error!("Failed to serialize message: {e}"); Status::internal("Failed to process message")` |
| 272 | `Status::internal(format!("Failed to add message: {e}"))` | `log::error!("Failed to add message: {e}"); Status::internal("Failed to add message")` |

### 7c. Verify

```bash
cd crates/amplifier-core && cargo test grpc_server
```

---

## Task 8: H-03 — Path Traversal Check in Module Resolver

**Priority:** High — arbitrary filesystem read via malicious amplifier.toml
**Estimated effort:** 1 hour
**File:** `crates/amplifier-core/src/module_resolver.rs` (lines 158-167)

### 8a. Write failing test

In `crates/amplifier-core/tests/module_resolver_e2e.rs`:

```rust
#[test]
fn artifact_path_traversal_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "../../../etc/passwd"
"#;
    std::fs::write(dir.path().join("amplifier.toml"), toml_content).unwrap();

    let result = amplifier_core::module_resolver::resolve(dir.path());
    assert!(result.is_err(), "Path traversal should be rejected");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("simple filename") || err_msg.contains("escapes"),
        "Error should mention path restriction: {err_msg}"
    );
}

#[test]
fn artifact_with_dot_prefix_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = ".hidden-module.wasm"
"#;
    std::fs::write(dir.path().join("amplifier.toml"), toml_content).unwrap();

    let result = amplifier_core::module_resolver::resolve(dir.path());
    assert!(result.is_err(), "Dot-prefixed artifact should be rejected");
}
```

### 8b. Fix: validate artifact is a simple filename

In `crates/amplifier-core/src/module_resolver.rs`, after extracting
`wasm_filename` (line 162), add validation:

```rust
Transport::Wasm => {
    let wasm_filename = module_section
        .get("artifact")
        .and_then(|v| v.as_str())
        .unwrap_or("module.wasm");

    // Reject path separators and dot-prefixed names — artifact must be
    // a simple filename within the module directory.
    if wasm_filename.contains('/')
        || wasm_filename.contains('\\')
        || wasm_filename.starts_with('.')
    {
        return Err(ModuleResolverError::TomlParseError {
            path: module_path.to_path_buf(),
            reason: format!(
                "artifact must be a simple filename (got '{wasm_filename}'). \
                 Path separators and dot-prefixed names are not allowed."
            ),
        });
    }

    let wasm_path = module_path.join(wasm_filename);

    // Defense in depth: verify resolved path stays inside module directory
    // (only possible when the file exists on disk; skip for deferred-load)
    if wasm_path.exists() {
        let canonical = wasm_path.canonicalize().map_err(|e| {
            ModuleResolverError::TomlParseError {
                path: module_path.to_path_buf(),
                reason: format!("Failed to canonicalize artifact path: {e}"),
            }
        })?;
        let canonical_base = module_path.canonicalize().map_err(|e| {
            ModuleResolverError::TomlParseError {
                path: module_path.to_path_buf(),
                reason: format!("Failed to canonicalize module path: {e}"),
            }
        })?;
        if !canonical.starts_with(&canonical_base) {
            return Err(ModuleResolverError::TomlParseError {
                path: module_path.to_path_buf(),
                reason: "artifact path escapes module directory".to_string(),
            });
        }
    }

    ModuleArtifact::WasmBytes {
        bytes: Vec::new(),
        path: wasm_path,
    }
}
```

### 8c. Verify

```bash
cd crates/amplifier-core && cargo test module_resolver
```

---

## Task 9: H-04 — Document Session ID Routing Limitation

**Priority:** High — cross-session access possible (depends on C-01 for full fix)
**Estimated effort:** 30 minutes
**File:** `crates/amplifier-core/src/grpc_server.rs` (lines 216-218, 248-252)

### 9a. Add log warning + doc comments

At the `get_messages` method:

```rust
async fn get_messages(
    &self,
    _request: Request<amplifier_module::GetMessagesRequest>,
) -> Result<Response<amplifier_module::GetMessagesResponse>, Status> {
    // TODO(security): session_id in GetMessagesRequest is currently ignored.
    // All connected modules share a single context (the coordinator's).
    // Per-session routing requires caller identity from authentication (C-01)
    // and a session-to-coordinator mapping. Tracked as post-merge follow-up.
    log::debug!(
        "get_messages: session_id routing not yet implemented — \
         returning shared coordinator context"
    );
    // ...existing code...
```

Same pattern for `add_message`:

```rust
async fn add_message(
    &self,
    request: Request<amplifier_module::KernelAddMessageRequest>,
) -> Result<Response<amplifier_module::Empty>, Status> {
    let req = request.into_inner();
    // TODO(security): session_id is currently ignored — messages are added
    // to the shared coordinator context regardless of which session the
    // caller claims. Per-session isolation requires C-01 (authentication).
    if !req.session_id.is_empty() {
        log::warn!(
            "add_message: session_id '{}' provided but per-session routing \
             is not yet implemented — message added to shared context",
            req.session_id
        );
    }
    // ...existing code unchanged...
```

### 9b. Verify

```bash
cd crates/amplifier-core && cargo test grpc_server
```

---

## Task 10: H-05 — Remove HTTP Import from WIT

**Priority:** High — declared interface not provided at runtime
**Estimated effort:** 30 minutes
**File:** `wit/amplifier-modules.wit` (lines 140-144)

### 10a. Write test — provider module world does not require HTTP

Verify the existing provider WASM test fixture still compiles/loads after
removing the HTTP import. (If no provider fixture exists, this is a
compile-only check.)

### 10b. Remove the HTTP import

In `wit/amplifier-modules.wit`, change:

```wit
// BEFORE:
/// Tier 2: Provider module — needs outbound HTTP for LLM API calls.
world provider-module {
    import wasi:http/outgoing-handler@0.2.0;
    export provider;
}

// AFTER:
/// Tier 2: Provider module.
///
/// NOTE: HTTP outbound is not yet supported. Provider modules that need
/// network access should use the gRPC transport (out-of-process) for now.
/// When wasi:http support is added, it will be gated behind an explicit
/// allow-list configuration.
world provider-module {
    export provider;
}
```

### 10c. Verify

```bash
cd crates/amplifier-core && cargo check --features wasm
cargo test --features wasm
```

---

## Task 11: H-06 — Fix TypeScript Type Definitions

**Priority:** High — type safety eliminated for key APIs
**Estimated effort:** 1 hour
**File:** `bindings/node/index.d.ts`

### 11a. Fix `status` getter return type

```typescript
// BEFORE (line 179):
get status(): string

// AFTER:
get status(): 'running' | 'completed' | 'failed' | 'cancelled'
```

### 11b. Fix hook handler signature

```typescript
// BEFORE (line 130):
register(event: string, handler: (...args: any[]) => any, priority: number, name: string): void

// AFTER:
register(
  event: string,
  handler: (event: string, dataJson: string) => string | Promise<string>,
  priority: number,
  name: string
): void
```

### 11c. Fix JsModuleManifest string literal unions

```typescript
// BEFORE (lines 83-87):
transport: string
moduleType: string
artifactType: string

// AFTER:
transport: 'python' | 'wasm' | 'grpc' | 'native'
moduleType: 'tool' | 'hook' | 'context' | 'approval' | 'provider' | 'orchestrator' | 'resolver'
artifactType: 'wasm' | 'grpc' | 'python'
```

### 11d. Verify

```bash
cd bindings/node && npx tsc --noEmit
npm test
```

---

## Task 12: H-07 — Add JSON Payload Size Limits in gRPC Server

**Priority:** High — DoS via unbounded JSON payloads
**Estimated effort:** 1 hour
**File:** `crates/amplifier-core/src/grpc_server.rs`

### 12a. Write failing test — oversized payload is rejected

```rust
#[tokio::test]
async fn oversized_json_payload_is_rejected() {
    let coord = Arc::new(Coordinator::new(Default::default()));
    coord.mount_tool(Arc::new(crate::testing::EchoTool::new("echo")));
    let service = KernelServiceImpl::new(coord);

    // 1 MB of nested JSON — exceeds 64 KB limit
    let huge_json = "{\"a\":".repeat(1024 * 128) + "null" + &"}".repeat(1024 * 128);

    let request = Request::new(amplifier_module::ExecuteToolRequest {
        tool_name: "echo".to_string(),
        input_json: huge_json,
    });

    let err = service.execute_tool(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("exceeds maximum size"));
}
```

### 12b. Add size validation helper + apply to all JSON fields

At the top of `grpc_server.rs`:

```rust
/// Maximum allowed size for JSON string fields in gRPC requests.
const MAX_JSON_PAYLOAD_BYTES: usize = 64 * 1024; // 64 KB

/// Validates that a JSON string field does not exceed the maximum size.
fn validate_json_size(json: &str, field_name: &str) -> Result<(), Status> {
    if json.len() > MAX_JSON_PAYLOAD_BYTES {
        Err(Status::invalid_argument(format!(
            "{field_name} exceeds maximum size of {MAX_JSON_PAYLOAD_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}
```

Apply before every `serde_json::from_str` call:

```rust
// execute_tool (line 124):
validate_json_size(&req.input_json, "input_json")?;

// emit_hook (line 164):
validate_json_size(&req.data_json, "data_json")?;

// emit_hook_and_collect (line 185):
validate_json_size(&req.data_json, "data_json")?;

// register_capability (line 343):
validate_json_size(&req.value_json, "value_json")?;
```

### 12c. Verify

```bash
cd crates/amplifier-core && cargo test grpc_server
```

---

## Verification — Full Suite

After all tasks are complete, run the full verification:

```bash
# Rust — all crates
cd /path/to/amplifier-core
cargo fmt --check
cargo clippy --all-features -- -D warnings
cargo test --all-features

# Node bindings
cd bindings/node
npm test
npx tsc --noEmit

# Python bindings (if test infrastructure exists)
cd bindings/python
maturin develop
pytest
```

---

## Follow-Up Work (Not Tasked — Track as Issues)

The following items were identified in the code review but are not
merge-blocking or production-blocking. They should be tracked as follow-up
issues and addressed in subsequent PRs.

### Medium Priority (M-01 through M-09)

| ID | Summary | File(s) |
|----|---------|---------|
| M-01 | Extract duplicated `get_typed_func` across 4+ WASM bridges into `bridges/mod.rs` | `wasm_tool.rs`, `wasm_provider.rs`, `wasm_context.rs`, `wasm_hook.rs`, `wasm_orchestrator.rs` |
| M-02 | Extract `to_json_or_warn` / `from_json_or_warn` helpers (30+ repeated sites) | `generated/conversions.rs` |
| M-03 | Replace `unwrap()` with `swap_remove(0)` in module resolver match arm | `module_resolver.rs:71` |
| M-04 | Separate `WasmPath(PathBuf)` from `WasmBytes` to eliminate deferred-load ambiguity | `module_resolver.rs:164` |
| M-05 | Add `debug_assert!` for `block_on` context safety in orchestrator bridge | `wasm_orchestrator.rs:111,147,185,215,244` |
| M-06 | Wire or remove `_reason` parameter in Node.js cancellation methods | `bindings/node/src/lib.rs:301-313` |
| M-07 | Remove dead exported types (`JsToolResult`, `JsToolSpec`, `JsSessionConfig`, `Role`) or wire into API | `bindings/node/src/lib.rs`, `index.d.ts` |
| M-08 | Add WASM module integrity verification (sha256 hash in `amplifier.toml`) | `module_resolver.rs:271-290` |
| M-09 | Clamp `timeout_seconds` in `EmitHookAndCollect` to max 300s | `grpc_server.rs:189-193` |

### Low Priority / Style (L-01 through L-07)

| ID | Summary | File(s) |
|----|---------|---------|
| L-01 | Replace `eprintln!` with `log::warn!` in Node bindings (3 sites) | `bindings/node/src/lib.rs:338,353,750` |
| L-02 | Remove duplicate stale doc comment on `export_tool!` macro | `amplifier-guest/src/lib.rs:30-54` |
| L-03 | Fix float sentinel 0.0 conflation with "not set" in proto conversions | `generated/conversions.rs:849-858` |
| L-04 | Make `ContentBlock::None` return error instead of silent empty text | `generated/conversions.rs:484-491` |
| L-05 | Narrow `unsafe impl Sync` safety comment for `PyHookHandlerBridge` | `bindings/python/src/lib.rs:67-68` |
| L-06 | Replace `blocking_lock()` with `try_lock()` in Python bindings (panic risk in async) | `bindings/python/src/lib.rs:389,417,505` |
| L-07 | Add name validation + overwrite protection to `register_capability` | `grpc_server.rs:338-347` |

### Test Coverage Gaps (TG-01 through TG-08)

| ID | Summary | File(s) |
|----|---------|---------|
| TG-01 | Add async (Promise-returning) hook handler test | `bindings/node/__tests__/hooks.test.ts` |
| TG-02 | Add invalid/corrupted WASM bytes test | `crates/amplifier-core/tests/wasm_e2e.rs` |
| TG-03 | Add JS-side error path tests for `resolveModule` / `loadWasmFromPath` | `bindings/node/__tests__/node-wasm-session.test.ts` |
| TG-04 | Add tests pinning detached coordinator/hooks behavior | `bindings/node/__tests__/` |
| TG-05 | Add hook handler throw test | `bindings/node/__tests__/hooks.test.ts` |
| TG-06 | Add test for directory with multiple `.wasm` files without `amplifier.toml` | `tests/module_resolver_e2e.rs` |
| TG-07 | Add test for invalid `transport` value in `amplifier.toml` | `tests/module_resolver_e2e.rs` |
| TG-08 | Add mixed-transport failure path test (WASM tool fails mid-session) | `tests/mixed_transport_e2e.rs` |

### Dependency Updates

| Item | Action |
|------|--------|
| `rand = "0.8"` → `"0.9"` | Update in `crates/amplifier-core/Cargo.toml`, fix any breaking API changes |
| Add `cargo-audit` to CI | Add `cargo audit` step to GitHub Actions workflow |

---

## Open Questions

1. **C-01 token transport:** Should the token be passed via environment
   variable (`AMPLIFIER_TOKEN`), command-line argument, or a token file?
   Environment variable is simplest but visible in `/proc/*/environ`.
   Token file with `chmod 0600` is more secure.

2. **C-02 epoch limits:** What are appropriate default values for
   `max_epoch_ticks` and `max_memory_bytes`? The plan uses 3000 ticks
   (~30 seconds) and 64 MB. These may need tuning based on real workloads.

3. **C-04 structural fix timeline:** The interim rename + warnings approach
   is correct for pre-merge, but the structural fix (Arc-wrapping the
   Coordinator's HookRegistry) should be prioritized. Should this be a
   separate follow-up PR or part of this batch?

4. **H-05 HTTP removal:** Does any existing provider module test fixture
   compile against the `provider-module` world with the HTTP import? If so,
   that fixture needs updating when the import is removed.