# PR #39 Medium-Priority Code Quality Improvements — Implementation Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Implement 9 medium-priority code quality items (M-01 through M-09) identified by a 4-agent code review of PR #39 (Cross-Language SDK), each as an independent commit on the `fix/pr39-medium-priority-items` branch.

**Architecture:** Each task is a self-contained refactoring or fix targeting a specific file or set of files. Tasks are ordered quickest-first. No task changes public API contracts — all changes are internal quality improvements (clamping, dedup, helper extraction, dead code removal, type safety).

**Tech Stack:** Rust (amplifier-core crate, wasmtime, tonic/prost gRPC), TypeScript/Node.js (napi-rs bindings, Vitest tests)

---

## Prerequisites

### Step 0: Verify branch and working state

The branch `fix/pr39-medium-priority-items` should already exist. Verify you are on it:

```bash
cd /home/bkrabach/dev/rust-devrust-core/amplifier-core
git branch --show-current
# Expected: fix/pr39-medium-priority-items
```

If not on the correct branch:
```bash
git checkout fix/pr39-medium-priority-items
```

Verify clean working tree:
```bash
git status
# Should show clean or only untracked files
```

### Verification commands (gate EVERY commit)

After every task, run ALL of these. Do not commit if any fail:

```bash
# Rust checks with WASM (MUST pass)
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings

# Rust checks without WASM (MUST pass)
cargo clippy -p amplifier-core -- -D warnings
```

For Tasks 4 and 7 (Node bindings), also run:
```bash
cd bindings/node && npm run build && npx vitest run && cd ../..
```

---

## Task 1: M-09 — Clamp timeout_seconds to max 300s in EmitHookAndCollect

**Files:**
- Modify: `crates/amplifier-core/src/grpc_server.rs`

### Step 1: Write failing tests

Open `crates/amplifier-core/src/grpc_server.rs`. The `#[cfg(test)] mod tests` block starts at line 532. Find the `EmitHookAndCollect tests` section (line 652). After the last test in that section (`emit_hook_and_collect_invalid_json_returns_invalid_argument` ending around line 770), add these new tests:

```rust
    // -----------------------------------------------------------------------
    // EmitHookAndCollect timeout clamping tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn emit_hook_and_collect_normal_timeout_passes_through() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        // A normal timeout (e.g. 10s) should work without error
        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:timeout".to_string(),
            data_json: String::new(),
            timeout_seconds: 10.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(result.is_ok(), "Normal timeout should succeed: {result:?}");
    }

    #[tokio::test]
    async fn emit_hook_and_collect_huge_timeout_is_clamped_to_max() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        // A huge timeout (e.g. 999999s) should be clamped and still succeed
        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:timeout".to_string(),
            data_json: String::new(),
            timeout_seconds: 999_999.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "Huge timeout should be clamped and succeed: {result:?}"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_nan_timeout_falls_back_to_default() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:timeout".to_string(),
            data_json: String::new(),
            timeout_seconds: f64::NAN,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "NaN timeout should fall back to default: {result:?}"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_infinity_timeout_falls_back_to_default() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:timeout".to_string(),
            data_json: String::new(),
            timeout_seconds: f64::INFINITY,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "Infinity timeout should fall back to default: {result:?}"
        );
    }

    #[tokio::test]
    async fn emit_hook_and_collect_negative_timeout_falls_back_to_default() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let service = KernelServiceImpl::new(coord);

        let request = Request::new(amplifier_module::EmitHookAndCollectRequest {
            event: "test:timeout".to_string(),
            data_json: String::new(),
            timeout_seconds: -5.0,
        });

        let result = service.emit_hook_and_collect(request).await;
        assert!(
            result.is_ok(),
            "Negative timeout should fall back to default: {result:?}"
        );
    }
```

### Step 2: Run tests to verify they pass (they will, since we're testing behavior)

```bash
cargo test -p amplifier-core --features wasm -- emit_hook_and_collect_normal_timeout
cargo test -p amplifier-core --features wasm -- emit_hook_and_collect_huge_timeout
cargo test -p amplifier-core --features wasm -- emit_hook_and_collect_nan_timeout
cargo test -p amplifier-core --features wasm -- emit_hook_and_collect_infinity_timeout
cargo test -p amplifier-core --features wasm -- emit_hook_and_collect_negative_timeout
```

Note: The NaN and Infinity tests may panic due to `Duration::from_secs_f64` panicking on non-finite values. This confirms the bug exists and the fix is needed.

### Step 3: Implement the timeout clamping

In `crates/amplifier-core/src/grpc_server.rs`, find the `emit_hook_and_collect` method. Add a constant near the top of the file (above the `impl` block, or right inside the method). Then replace the timeout logic at lines 302-306.

**Add constant** — find a good location near other constants or at the top of the `emit_hook_and_collect` method. Place it just above the method or as a module-level constant:

Find this code (around line 279):
```rust
    async fn emit_hook_and_collect(
```

Add the constant just above the `impl` block for `KernelService`, or at module level. The simplest approach: add it right before the timeout logic inside the method. Replace lines 302-306:

**Find this exact code:**
```rust
        let timeout = if req.timeout_seconds > 0.0 {
            std::time::Duration::from_secs_f64(req.timeout_seconds)
        } else {
            std::time::Duration::from_secs(30)
        };
```

**Replace with:**
```rust
        const MAX_HOOK_COLLECT_TIMEOUT_SECS: f64 = 300.0;
        const DEFAULT_HOOK_COLLECT_TIMEOUT_SECS: u64 = 30;

        let timeout = if req.timeout_seconds.is_finite() && req.timeout_seconds > 0.0 {
            let clamped = req.timeout_seconds.min(MAX_HOOK_COLLECT_TIMEOUT_SECS);
            std::time::Duration::from_secs_f64(clamped)
        } else {
            std::time::Duration::from_secs(DEFAULT_HOOK_COLLECT_TIMEOUT_SECS)
        };
```

### Step 4: Run all tests to verify they pass

```bash
cargo test -p amplifier-core --features wasm -- emit_hook_and_collect -v
```

Expected: ALL emit_hook_and_collect tests pass, including the new ones.

### Step 5: Run full verification suite

```bash
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

### Step 6: Commit

```bash
git add crates/amplifier-core/src/grpc_server.rs
git commit -m "fix(grpc): clamp hook-collect timeout to 300s maximum

Add finite-check and upper bound clamping to emit_hook_and_collect
timeout_seconds. Previously, NaN/Infinity would panic in
Duration::from_secs_f64, and arbitrarily large values were uncapped.

Now: non-finite or non-positive values fall back to 30s default,
and positive values are clamped to 300s maximum.

Addresses M-09 from PR #39 code review."
```

---

## Task 2: M-03 — Replace unwrap() with swap_remove(0) in module resolver

**Files:**
- Modify: `crates/amplifier-core/src/module_resolver.rs`

### Step 1: Apply the fix

In `crates/amplifier-core/src/module_resolver.rs`, find line 71:

**Find this exact code:**
```rust
        1 => Ok(matched.into_iter().next().unwrap().1),
```

**Replace with:**
```rust
        1 => Ok(matched.swap_remove(0).1),
```

### Step 2: Run tests to verify nothing broke

```bash
cargo test -p amplifier-core --features wasm -- detect_wasm -v
```

Expected: All `detect_wasm_module_type` related tests pass.

### Step 3: Run full verification suite

```bash
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

### Step 4: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs
git commit -m "fix(resolver): replace unwrap with swap_remove in detect_wasm_module_type

The single-match arm used .into_iter().next().unwrap() where the length
was already verified to be 1. Replace with swap_remove(0) which is
panic-free and more idiomatic for extracting from a known-length vec.

Addresses M-03 from PR #39 code review."
```

---

## Task 3: M-05 — Add debug_assert! before 5 block_on calls in orchestrator bridge

**Files:**
- Modify: `crates/amplifier-core/src/bridges/wasm_orchestrator.rs`

### Step 1: Add debug_assert guards

In `crates/amplifier-core/src/bridges/wasm_orchestrator.rs`, there are 5 `block_on` calls inside `register_kernel_service_imports()`. Each one looks like:

```rust
                let result = tokio::runtime::Handle::current().block_on(async {
```

Add a `debug_assert!` immediately before each `block_on` call. The 5 locations are:

**Location 1 — line 111 (execute-tool closure):**
Find:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
```
Replace with:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                debug_assert!(
                    tokio::runtime::Handle::try_current().is_ok(),
                    "block_on requires an active Tokio runtime — must run inside spawn_blocking"
                );
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
```

**Location 2 — line 147 (complete-with-provider closure):**
Find:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("complete-with-provider: bad request: {e}"))?;
```
Replace with:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                debug_assert!(
                    tokio::runtime::Handle::try_current().is_ok(),
                    "block_on requires an active Tokio runtime — must run inside spawn_blocking"
                );
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("complete-with-provider: bad request: {e}"))?;
```

**Location 3 — line 185 (emit-hook closure):**
Find:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("emit-hook: bad request: {e}"))?;
```
Replace with:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                debug_assert!(
                    tokio::runtime::Handle::try_current().is_ok(),
                    "block_on requires an active Tokio runtime — must run inside spawn_blocking"
                );
                let result = tokio::runtime::Handle::current().block_on(async {
                    let req: Value = serde_json::from_slice(&request_bytes)
                        .map_err(|e| format!("emit-hook: bad request: {e}"))?;
```

**Location 4 — line 215 (get-messages closure):**
Find:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let context = coord
                        .context()
                        .ok_or_else(|| "get-messages: no context manager mounted".to_string())?;
```
Replace with:
```rust
                  -> wasmtime::Result<(Result<Vec<u8>, String>,)> {
                debug_assert!(
                    tokio::runtime::Handle::try_current().is_ok(),
                    "block_on requires an active Tokio runtime — must run inside spawn_blocking"
                );
                let result = tokio::runtime::Handle::current().block_on(async {
                    let context = coord
                        .context()
                        .ok_or_else(|| "get-messages: no context manager mounted".to_string())?;
```

**Location 5 — line 244 (add-message closure):**
Find:
```rust
                  -> wasmtime::Result<(Result<(), String>,)> {
                let result = tokio::runtime::Handle::current().block_on(async {
                    let message: Value = serde_json::from_slice(&request_bytes)
```
Replace with:
```rust
                  -> wasmtime::Result<(Result<(), String>,)> {
                debug_assert!(
                    tokio::runtime::Handle::try_current().is_ok(),
                    "block_on requires an active Tokio runtime — must run inside spawn_blocking"
                );
                let result = tokio::runtime::Handle::current().block_on(async {
                    let message: Value = serde_json::from_slice(&request_bytes)
```

### Step 2: Run full verification suite

```bash
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

### Step 3: Commit

```bash
git add crates/amplifier-core/src/bridges/wasm_orchestrator.rs
git commit -m "fix(wasm): add debug_assert guards before block_on calls in orchestrator bridge

All 5 block_on calls in register_kernel_service_imports() now have a
debug_assert verifying a Tokio runtime handle is available. These fire
only in debug builds and document the invariant that these closures
must execute inside spawn_blocking (which carries the runtime handle).

Addresses M-05 from PR #39 code review."
```

---

## Task 4: M-06 — Remove _reason parameter from cancellation methods

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Modify: `bindings/node/index.d.ts`
- Modify: `bindings/node/__tests__/cancellation.test.ts`

### Step 1: Remove _reason from Rust bindings

In `bindings/node/src/lib.rs`, find lines 300-308:

**Find:**
```rust
    #[napi]
    pub fn request_graceful(&self, _reason: Option<String>) {
        self.inner.request_graceful();
    }

    #[napi]
    pub fn request_immediate(&self, _reason: Option<String>) {
        self.inner.request_immediate();
    }
```

**Replace with:**
```rust
    #[napi]
    pub fn request_graceful(&self) {
        self.inner.request_graceful();
    }

    #[napi]
    pub fn request_immediate(&self) {
        self.inner.request_immediate();
    }
```

### Step 2: Update TypeScript declarations

In `bindings/node/index.d.ts`, find lines 137-138:

**Find:**
```typescript
  requestGraceful(reason?: string | undefined | null): void
  requestImmediate(reason?: string | undefined | null): void
```

**Replace with:**
```typescript
  requestGraceful(): void
  requestImmediate(): void
```

### Step 3: Update tests

In `bindings/node/__tests__/cancellation.test.ts`, find lines 47-59 (the two tests that pass reason strings):

**Find:**
```typescript
  it('requestGraceful accepts optional reason string', () => {
    const token = new JsCancellationToken()
    token.requestGraceful('user requested stop')
    expect(token.isCancelled).toBe(true)
    expect(token.isGraceful).toBe(true)
  })

  it('requestImmediate accepts optional reason string', () => {
    const token = new JsCancellationToken()
    token.requestImmediate('timeout exceeded')
    expect(token.isCancelled).toBe(true)
    expect(token.isImmediate).toBe(true)
  })
```

**Delete these two tests entirely** (lines 47-59). They test the parameter we just removed.

### Step 4: Build and test Node bindings

```bash
cd bindings/node && npm run build && npx vitest run && cd ../..
```

Expected: All tests pass, build succeeds.

### Step 5: Run Rust verification

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

### Step 6: Commit

```bash
git add bindings/node/src/lib.rs bindings/node/index.d.ts bindings/node/__tests__/cancellation.test.ts
git commit -m "fix(node): remove unused reason parameter from cancellation methods

The _reason parameter on requestGraceful() and requestImmediate() was
accepted but silently discarded — Rust's CancellationToken has no
reason field to wire it to. Remove the parameter entirely rather than
maintain a misleading API surface.

Addresses M-06 from PR #39 code review."
```

---

## Task 5: M-02 — Extract to_json_or_warn / from_json_or_warn helpers in conversions.rs

**Files:**
- Modify: `crates/amplifier-core/src/generated/conversions.rs`

### Step 1: Add helper functions at the top of the file

In `crates/amplifier-core/src/generated/conversions.rs`, add these helpers at the very top of the file, right after the module doc comment (after line 3 `//! boundary.`):

**Find:**
```rust
//! boundary.

// ---------------------------------------------------------------------------
// ToolResult conversions
```

**Replace with:**
```rust
//! boundary.

// ---------------------------------------------------------------------------
// Shared serialization/deserialization helpers
// ---------------------------------------------------------------------------

/// Serialize a value to JSON, logging a warning and returning an empty string on failure.
fn to_json_or_warn(value: &impl serde::Serialize, label: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| {
        log::warn!("Failed to serialize {label} to JSON: {e}");
        String::new()
    })
}

/// Deserialize a JSON string, logging a warning and returning the type's `Default` on failure.
fn from_json_or_default<T: serde::de::DeserializeOwned + Default>(json: &str, label: &str) -> T {
    serde_json::from_str(json).unwrap_or_else(|e| {
        log::warn!("Failed to deserialize {label}: {e}");
        T::default()
    })
}

// ---------------------------------------------------------------------------
// ToolResult conversions
```

### Step 2: Replace all `serde_json::to_string(...).unwrap_or_else(...)` patterns

This is a mechanical replacement. There are 26 occurrences to replace. Work through the file systematically. For each occurrence, replace the multi-line `serde_json::to_string(&value).unwrap_or_else(|e| { log::warn!(...); String::new() })` pattern with a call to `to_json_or_warn(&value, "label")`.

Here are all 26 replacements grouped by section. The `label` should match the original warn message's description:

**Lines 16-19 — ToolResult output:**
Find:
```rust
                    serde_json::to_string(&v).unwrap_or_else(|e| {
                        log::warn!("Failed to serialize ToolResult output to JSON: {e}");
                        String::new()
                    })
```
Replace with:
```rust
                    to_json_or_warn(&v, "ToolResult output")
```

**Lines 25-28 — ToolResult error:**
Find:
```rust
                    serde_json::to_string(&e).unwrap_or_else(|ser_err| {
                        log::warn!("Failed to serialize ToolResult error to JSON: {ser_err}");
                        String::new()
                    })
```
Replace with:
```rust
                    to_json_or_warn(&e, "ToolResult error")
```

**Line 87-90 — ModelInfo defaults:**
Find:
```rust
            defaults_json: serde_json::to_string(&native.defaults).unwrap_or_else(|e| {
                log::warn!("Failed to serialize ModelInfo defaults to JSON: {e}");
                String::new()
            }),
```
Replace with:
```rust
            defaults_json: to_json_or_warn(&native.defaults, "ModelInfo defaults"),
```

**Lines 281-284 — Thinking content:**
Find:
```rust
                        serde_json::to_string(&v).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize Thinking content to JSON: {e}");
                            String::new()
                        })
```
Replace with:
```rust
                        to_json_or_warn(&v, "Thinking content")
```

**Lines 306-309 — ToolCall input:**
Find:
```rust
                input_json: serde_json::to_string(&input).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolCall input to JSON: {e}");
                    String::new()
                }),
```
Replace with:
```rust
                input_json: to_json_or_warn(&input, "ToolCall input"),
```

**Lines 321-324 — ToolResult output (block):**
Find:
```rust
                output_json: serde_json::to_string(&output).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolResult output to JSON: {e}");
                    String::new()
                }),
```
Replace with:
```rust
                output_json: to_json_or_warn(&output, "ToolResult output"),
```

**Lines 343-346 — Image source:**
Find:
```rust
                source_json: serde_json::to_string(&source).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize Image source to JSON: {e}");
                    String::new()
                }),
```
Replace with:
```rust
                source_json: to_json_or_warn(&source, "Image source"),
```

**Lines 360-363 — Reasoning content item:**
Find:
```rust
                        serde_json::to_string(&v).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize Reasoning content item to JSON: {e}");
                            String::new()
                        })
```
Replace with:
```rust
                        to_json_or_warn(&v, "Reasoning content item")
```

**Lines 369-372 — Reasoning summary item:**
Find:
```rust
                        serde_json::to_string(&v).unwrap_or_else(|e| {
                            log::warn!("Failed to serialize Reasoning summary item to JSON: {e}");
                            String::new()
                        })
```
Replace with:
```rust
                        to_json_or_warn(&v, "Reasoning summary item")
```

**Lines 526-529 — Message metadata:**
Find:
```rust
                serde_json::to_string(&m).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize Message metadata to JSON: {e}");
                    String::new()
                })
```
Replace with:
```rust
                to_json_or_warn(&m, "Message metadata")
```

**Lines 637-640 — HookResult data:**
Find:
```rust
            serde_json::to_string(d).unwrap_or_else(|e| {
                log::warn!("Failed to serialize HookResult data to JSON: {e}");
                String::new()
            })
```
Replace with:
```rust
            to_json_or_warn(d, "HookResult data")
```

**Lines 704-707 — ToolSpec parameters:**
Find:
```rust
                parameters_json: serde_json::to_string(&t.parameters).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolSpec parameters to JSON: {e}");
                    String::new()
                }),
```
Replace with:
```rust
                parameters_json: to_json_or_warn(&t.parameters, "ToolSpec parameters"),
```

**Lines 719-722 — JsonSchema schema:**
Find:
```rust
                    schema_json: serde_json::to_string(schema).unwrap_or_else(|e| {
                        log::warn!("Failed to serialize JsonSchema schema to JSON: {e}");
                        String::new()
                    }),
```
Replace with:
```rust
                    schema_json: to_json_or_warn(schema, "JsonSchema schema"),
```

**Lines 747-750 — ChatRequest metadata:**
Find:
```rust
                serde_json::to_string(m).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ChatRequest metadata to JSON: {e}");
                    String::new()
                })
```
Replace with:
```rust
                to_json_or_warn(m, "ChatRequest metadata")
```

**Lines 759 — ToolChoice object:**
Find:
```rust
                ToolChoice::Object(obj) => serde_json::to_string(obj).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolChoice object to JSON: {e}");
                    String::new()
                }),
```
Replace with:
```rust
                ToolChoice::Object(obj) => to_json_or_warn(obj, "ToolChoice object"),
```

**Lines 937-940 — ChatResponse content:**
Find:
```rust
        content: serde_json::to_string(&response.content).unwrap_or_else(|e| {
            log::warn!("Failed to serialize ChatResponse content to JSON: {e}");
            String::new()
        }),
```
Replace with:
```rust
        content: to_json_or_warn(&response.content, "ChatResponse content"),
```

**Lines 949-952 — ToolCall arguments:**
Find:
```rust
                arguments_json: serde_json::to_string(&tc.arguments).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ToolCall arguments to JSON: {e}");
                    String::new()
                }),
```
Replace with:
```rust
                arguments_json: to_json_or_warn(&tc.arguments, "ToolCall arguments"),
```

**Lines 969-972 — ChatResponse metadata:**
Find:
```rust
                serde_json::to_string(m).unwrap_or_else(|e| {
                    log::warn!("Failed to serialize ChatResponse metadata to JSON: {e}");
                    String::new()
                })
```
Replace with:
```rust
                to_json_or_warn(m, "ChatResponse metadata")
```

### Step 3: Replace all `serde_json::from_str(...).unwrap_or_else(...)` patterns that return Default

There are 8 `from_str` patterns that follow the same shape and return `Default::default()` (or a type-specific default). Replace them with `from_json_or_default`:

**Line 106-109 — ModelInfo defaults_json:**
Find:
```rust
                serde_json::from_str(&proto.defaults_json).unwrap_or_else(|e| {
                    log::warn!("Failed to deserialize ModelInfo defaults_json: {e}");
                    Default::default()
                })
```
Replace with:
```rust
                from_json_or_default(&proto.defaults_json, "ModelInfo defaults_json")
```

**Line 428-431 — ToolCallBlock input_json:**
Find:
```rust
            input: serde_json::from_str(&tc.input_json).unwrap_or_else(|e| {
                log::warn!("Failed to deserialize ToolCallBlock input_json: {e}");
                Default::default()
            }),
```
Replace with:
```rust
            input: from_json_or_default(&tc.input_json, "ToolCallBlock input_json"),
```

**Line 448-451 — ImageBlock source_json:**
Find:
```rust
                serde_json::from_str(&ib.source_json).unwrap_or_else(|e| {
                    log::warn!("Failed to deserialize ImageBlock source_json: {e}");
                    Default::default()
                })
```
Replace with:
```rust
                from_json_or_default(&ib.source_json, "ImageBlock source_json")
```

**Line 818-821 — ToolSpec parameters_json:**
Find:
```rust
                            serde_json::from_str(&t.parameters_json).unwrap_or_else(|e| {
                                log::warn!("Failed to deserialize ToolSpec parameters_json: {e}");
                                Default::default()
                            })
```
Replace with:
```rust
                            from_json_or_default(&t.parameters_json, "ToolSpec parameters_json")
```

**Line 835-838 — JsonSchemaFormat schema_json:**
Find:
```rust
                    serde_json::from_str(&js.schema_json).unwrap_or_else(|e| {
                        log::warn!("Failed to deserialize JsonSchemaFormat schema_json: {e}");
                        Default::default()
                    })
```
Replace with:
```rust
                    from_json_or_default(&js.schema_json, "JsonSchemaFormat schema_json")
```

**Line 993-996 — ChatResponse content:**
Find:
```rust
            serde_json::from_str(&response.content).unwrap_or_else(|e| {
                log::warn!("Failed to deserialize ChatResponse content: {e}");
                Vec::new()
            })
```
Replace with:
```rust
            from_json_or_default(&response.content, "ChatResponse content")
```

**Line 1011-1014 — ToolCall arguments_json:**
Find:
```rust
                            serde_json::from_str(&tc.arguments_json).unwrap_or_else(|e| {
                                log::warn!("Failed to deserialize ToolCall arguments_json: {e}");
                                Default::default()
                            })
```
Replace with:
```rust
                            from_json_or_default(&tc.arguments_json, "ToolCall arguments_json")
```

**Note on special cases:** Two `from_str` patterns do NOT fit `from_json_or_default`:
- Line 437: `output: serde_json::from_str(&tr.output_json).unwrap_or_else(|e| { ... serde_json::Value::Null })` — returns `Value::Null`, not `Default`. Leave this one as-is OR note that `Value::default()` is `Value::Null`, so `from_json_or_default` works here too since `serde_json::Value` implements `Default` as `Value::Null`.
- Line 574-579: The metadata deserialization uses `.map_err(|e| { log::warn!(...); e }).ok()` — different pattern (maps to `Option`). Leave this one as-is.

So replace the line 437 one too:

**Line 437-440 — ToolResultBlock output_json:**
Find:
```rust
            output: serde_json::from_str(&tr.output_json).unwrap_or_else(|e| {
                log::warn!("Failed to deserialize ToolResultBlock output_json: {e}");
                serde_json::Value::Null
            }),
```
Replace with:
```rust
            output: from_json_or_default(&tr.output_json, "ToolResultBlock output_json"),
```

(This works because `serde_json::Value::default()` is `Value::Null`.)

### Step 4: Run full verification suite

```bash
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

Expected: All tests pass, no clippy warnings. The helpers may trigger an `unused` warning if any occurrence was missed — fix by finding remaining instances.

### Step 5: Commit

```bash
git add crates/amplifier-core/src/generated/conversions.rs
git commit -m "refactor(conversions): extract to_json_or_warn and from_json_or_default helpers

Replace 26 repeated serde_json::to_string().unwrap_or_else() patterns
with to_json_or_warn() and 8 from_str().unwrap_or_else() patterns with
from_json_or_default(). Reduces ~120 lines of boilerplate to ~34.

Addresses M-02 from PR #39 code review."
```

---

## Task 6: M-01 — Extract shared get_typed_func to bridges/mod.rs

**Files:**
- Modify: `crates/amplifier-core/src/bridges/mod.rs`
- Modify: `crates/amplifier-core/src/bridges/wasm_tool.rs`
- Modify: `crates/amplifier-core/src/bridges/wasm_provider.rs`
- Modify: `crates/amplifier-core/src/bridges/wasm_context.rs`
- Modify: `crates/amplifier-core/src/bridges/wasm_hook.rs`
- Modify: `crates/amplifier-core/src/bridges/wasm_orchestrator.rs`
- Modify: `crates/amplifier-core/src/bridges/wasm_approval.rs`

### Step 1: Add the shared function to bridges/mod.rs

In `crates/amplifier-core/src/bridges/mod.rs`, add the shared function at the end of the file (after line 67):

```rust

/// Look up a typed function export from a WASM component instance.
///
/// Component Model exports may be at the root level or nested inside an
/// exported interface instance. This helper tries:
/// 1. Direct root-level export by `func_name`
/// 2. Nested inside the `interface_name` exported instance
#[cfg(feature = "wasm")]
pub(crate) fn get_typed_func<Params, Results>(
    instance: &wasmtime::component::Instance,
    store: &mut wasmtime::Store<wasm_tool::WasmState>,
    func_name: &str,
    interface_name: &str,
) -> Result<wasmtime::component::TypedFunc<Params, Results>, Box<dyn std::error::Error + Send + Sync>>
where
    Params: wasmtime::component::Lower + wasmtime::component::ComponentNamedList,
    Results: wasmtime::component::Lift + wasmtime::component::ComponentNamedList,
{
    // Try direct root-level export first.
    if let Ok(f) = instance.get_typed_func::<Params, Results>(&mut *store, func_name) {
        return Ok(f);
    }

    // Try nested inside the interface-exported instance.
    let iface_idx = instance
        .get_export_index(&mut *store, None, interface_name)
        .ok_or_else(|| format!("export instance '{interface_name}' not found"))?;
    let func_idx = instance
        .get_export_index(&mut *store, Some(&iface_idx), func_name)
        .ok_or_else(|| {
            format!("export function '{func_name}' not found in '{interface_name}'")
        })?;
    let func = instance
        .get_typed_func::<Params, Results>(&mut *store, &func_idx)
        .map_err(|e| format!("typed func lookup failed for '{func_name}': {e}"))?;
    Ok(func)
}
```

### Step 2: Replace get_typed_func_from_instance in wasm_tool.rs

In `crates/amplifier-core/src/bridges/wasm_tool.rs`, **delete** the entire `get_typed_func_from_instance` function (lines 100-131, from the doc comment `/// Look up a typed function export...` through `Ok(func)\n}`).

Then update all call sites in the same file. Find each call to `get_typed_func_from_instance` and replace with `super::get_typed_func`, adding the `INTERFACE_NAME` argument.

**Example — line 141 (call_get_spec):**
Find:
```rust
    let func = get_typed_func_from_instance::<(), (Vec<u8>,)>(&instance, &mut store, "get-spec")?;
```
Replace with:
```rust
    let func = super::get_typed_func::<(), (Vec<u8>,)>(&instance, &mut store, "get-spec", INTERFACE_NAME)?;
```

Search for ALL other calls to `get_typed_func_from_instance` in this file and replace similarly. Each call adds `INTERFACE_NAME` as the 4th argument.

### Step 3: Replace get_provider_func in wasm_provider.rs

In `crates/amplifier-core/src/bridges/wasm_provider.rs`, **delete** the entire `get_provider_func` function (lines 44-74, from `/// Look up a typed function export...` through `Ok(func)\n}`).

Then update all call sites. Find each `get_provider_func` call and replace:

**Example — line 84 (call_get_info):**
Find:
```rust
    let func = get_provider_func::<(), (Vec<u8>,)>(&instance, &mut store, "get-info")?;
```
Replace with:
```rust
    let func = super::get_typed_func::<(), (Vec<u8>,)>(&instance, &mut store, "get-info", INTERFACE_NAME)?;
```

Do the same for all other `get_provider_func` calls in the file (list-models, complete, parse-tool-calls).

### Step 4: Replace get_context_func in wasm_context.rs

In `crates/amplifier-core/src/bridges/wasm_context.rs`, **delete** the entire `get_context_func` function (lines 32-62).

Update all call sites, replacing `get_context_func` with `super::get_typed_func` and adding `INTERFACE_NAME`.

### Step 5: Replace get_handle_func in wasm_hook.rs

In `crates/amplifier-core/src/bridges/wasm_hook.rs`, **delete** the entire `get_handle_func` function (lines 32-59).

This function is NOT generic — it hardcodes `(Vec<u8>,)` and `(Result<Vec<u8>, String>,)`. Replace calls with explicit type params:

**Example — line 73 (call_handle):**
Find:
```rust
    let func = get_handle_func(&instance, &mut store)?;
```
Replace with:
```rust
    let func = super::get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
        &instance, &mut store, "handle", INTERFACE_NAME,
    )?;
```

Also update the `HandleFunc` type alias usage if it's used only in the deleted function. If `HandleFunc` is still used elsewhere (as a stored type), keep it.

### Step 6: Replace get_execute_func in wasm_orchestrator.rs

In `crates/amplifier-core/src/bridges/wasm_orchestrator.rs`, **delete** the entire `get_execute_func` function (lines 327-354).

Replace calls — note this file uses `ORCHESTRATOR_INTERFACE` (not `INTERFACE_NAME`):

**Example — in call_execute_sync:**
Find:
```rust
    let func = get_execute_func(&instance, &mut store)?;
```
Replace with:
```rust
    let func = super::get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
        &instance, &mut store, "execute", ORCHESTRATOR_INTERFACE,
    )?;
```

### Step 7: Replace get_request_approval_func in wasm_approval.rs

In `crates/amplifier-core/src/bridges/wasm_approval.rs`, **delete** the entire `get_request_approval_func` function (lines 33-62).

Replace calls:

**Example — line 75 (call_request_approval):**
Find:
```rust
    let func = get_request_approval_func(&instance, &mut store)?;
```
Replace with:
```rust
    let func = super::get_typed_func::<(Vec<u8>,), (Result<Vec<u8>, String>,)>(
        &instance, &mut store, "request-approval", INTERFACE_NAME,
    )?;
```

### Step 8: Run full verification suite

```bash
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

If clippy warns about unused imports (e.g., the `HandleFunc` type alias that was only used internally), remove them.

### Step 9: Commit

```bash
git add crates/amplifier-core/src/bridges/
git commit -m "refactor(wasm): extract shared get_typed_func to bridges/mod.rs

All 6 WASM bridge files had near-identical functions for resolving a
typed export (try root-level, then try interface-nested). Extract to a
single generic get_typed_func in bridges/mod.rs. Each bridge now passes
its INTERFACE_NAME constant as a parameter.

Addresses M-01 from PR #39 code review."
```

---

## Task 7: M-07 — Remove dead API surface from Node bindings

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Modify: `bindings/node/index.d.ts`
- Modify: `bindings/node/__tests__/types.test.ts`

### Step 1: Remove dead types from Rust bindings

In `bindings/node/src/lib.rs`:

**Delete the `Role` enum** (lines 80-88):
```rust
#[napi(string_enum)]
pub enum Role {
    System,
    Developer,
    User,
    Assistant,
    Function,
    Tool,
}
```

**Delete the `JsToolResult` struct** (lines 218-223):
```rust
#[napi(object)]
pub struct JsToolResult {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}
```

**Delete the `JsToolSpec` struct** (lines 225-230):
```rust
#[napi(object)]
pub struct JsToolSpec {
    pub name: String,
    pub description: Option<String>,
    pub parameters_json: String,
}
```

**Delete the `JsSessionConfig` struct** (lines 248-251):
```rust
#[napi(object)]
pub struct JsSessionConfig {
    pub config_json: String,
}
```

### Step 2: Update TypeScript declarations

In `bindings/node/index.d.ts`:

**Delete the `Role` enum** (lines 34-41):
```typescript
export const enum Role {
  System = 'System',
  Developer = 'Developer',
  User = 'User',
  Assistant = 'Assistant',
  Function = 'Function',
  Tool = 'Tool'
}
```

**Delete the `JsToolResult` interface** (lines 42-46):
```typescript
export interface JsToolResult {
  success: boolean
  output?: string
  error?: string
}
```

**Delete the `JsToolSpec` interface** (lines 47-51):
```typescript
export interface JsToolSpec {
  name: string
  description?: string
  parametersJson: string
}
```

**Delete the `JsSessionConfig` interface** (lines 66-68):
```typescript
export interface JsSessionConfig {
  configJson: string
}
```

### Step 3: Update tests

In `bindings/node/__tests__/types.test.ts`:

**Remove `Role` from the import** (line 8):
Find:
```typescript
import {
  HookAction,
  SessionState,
  ContextInjectionRole,
  ApprovalDefault,
  UserMessageLevel,
  Role,
} from '../index.js'
```
Replace with:
```typescript
import {
  HookAction,
  SessionState,
  ContextInjectionRole,
  ApprovalDefault,
  UserMessageLevel,
} from '../index.js'
```

**Delete the entire `Role` describe block** (lines 54-63):
```typescript
  describe('Role', () => {
    it('has all expected variants with correct string values', () => {
      expect(Role.System).toBe('System')
      expect(Role.Developer).toBe('Developer')
      expect(Role.User).toBe('User')
      expect(Role.Assistant).toBe('Assistant')
      expect(Role.Function).toBe('Function')
      expect(Role.Tool).toBe('Tool')
    })
  })
```

### Step 4: Build and test Node bindings

```bash
cd bindings/node && npm run build && npx vitest run && cd ../..
```

### Step 5: Run Rust verification

```bash
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

### Step 6: Commit

```bash
git add bindings/node/src/lib.rs bindings/node/index.d.ts bindings/node/__tests__/types.test.ts
git commit -m "fix(node): remove unused JsToolResult, JsToolSpec, JsSessionConfig, Role exports

These types were exported but never consumed by any JS/TS code:
- JsToolResult/JsToolSpec: execute() and getSpec() return raw JSON strings
- JsSessionConfig: wraps config_json but no API accepts it
- Role: duplicates the proto role enum but nothing references it

Addresses M-07 from PR #39 code review."
```

---

## Task 8: M-04 — Introduce WasmPath(PathBuf) variant in ModuleArtifact

**Files:**
- Modify: `crates/amplifier-core/src/module_resolver.rs`

### Step 1: Write a failing test

In `crates/amplifier-core/src/module_resolver.rs`, find the test module (`#[cfg(test)] mod tests` at line 549). Add a new test near the existing `parse_toml_wasm_transport` test (around line 662):

```rust
    #[test]
    fn parse_toml_wasm_returns_wasm_path_not_wasm_bytes() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "my-tool.wasm"
"#;
        let path = Path::new("/modules/my-tool");
        let manifest = parse_amplifier_toml(toml_content, path).unwrap();
        match &manifest.artifact {
            ModuleArtifact::WasmPath(wasm_path) => {
                assert_eq!(wasm_path, &PathBuf::from("/modules/my-tool/my-tool.wasm"));
            }
            other => panic!("expected WasmPath, got {other:?}"),
        }
    }
```

### Step 2: Run test to verify it fails

```bash
cargo test -p amplifier-core --features wasm -- parse_toml_wasm_returns_wasm_path -v
```

Expected: FAIL — `WasmPath` variant does not exist yet.

### Step 3: Add the WasmPath variant to ModuleArtifact

In `crates/amplifier-core/src/module_resolver.rs`, find the `ModuleArtifact` enum (line 236):

**Find:**
```rust
/// The loadable artifact for a resolved module.
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleArtifact {
    /// Raw WASM component bytes, plus the path they were read from.
    WasmBytes { bytes: Vec<u8>, path: PathBuf },
    /// A gRPC endpoint URL (e.g., "http://localhost:50051").
    GrpcEndpoint(String),
    /// A Python package name (e.g., "amplifier_module_tool_bash").
    PythonModule(String),
}
```

**Replace with:**
```rust
/// The loadable artifact for a resolved module.
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleArtifact {
    /// A WASM component path that has NOT yet been loaded into memory.
    /// Returned by `parse_amplifier_toml` — bytes will be read lazily by
    /// the transport layer or `load_module`.
    WasmPath(PathBuf),
    /// Raw WASM component bytes, plus the path they were read from.
    /// Returned by `resolve_module` when it reads the bytes eagerly.
    WasmBytes { bytes: Vec<u8>, path: PathBuf },
    /// A gRPC endpoint URL (e.g., "http://localhost:50051").
    GrpcEndpoint(String),
    /// A Python package name (e.g., "amplifier_module_tool_bash").
    PythonModule(String),
}
```

### Step 4: Update parse_amplifier_toml to return WasmPath

Find lines 203-206:

**Find:**
```rust
            ModuleArtifact::WasmBytes {
                bytes: Vec::new(), // bytes loaded later by the transport layer
                path: wasm_path,
            }
```

**Replace with:**
```rust
            ModuleArtifact::WasmPath(wasm_path)
```

### Step 5: Update load_module to handle WasmPath

Find the WASM transport match arm in `load_module` (around line 473):

**Find:**
```rust
        Transport::Wasm => {
            let bytes = match &manifest.artifact {
                ModuleArtifact::WasmBytes { bytes, .. } => bytes,
                other => {
                    return Err(format!(
                        "expected WasmBytes artifact for WASM transport, got {:?}",
                        other
                    )
                    .into())
                }
            };
```

**Replace with:**
```rust
        Transport::Wasm => {
            let bytes = match &manifest.artifact {
                ModuleArtifact::WasmPath(path) => {
                    std::fs::read(path).map_err(|e| {
                        format!("failed to read WASM bytes from {}: {e}", path.display())
                    })?
                }
                ModuleArtifact::WasmBytes { bytes, .. } => bytes.clone(),
                other => {
                    return Err(format!(
                        "expected WasmPath or WasmBytes artifact for WASM transport, got {:?}",
                        other
                    )
                    .into())
                }
            };
```

Then update the references below this match arm. The local `bytes` variable was previously `&Vec<u8>` (a reference) but is now `Vec<u8>` (owned). Find all usages of `bytes` in the rest of the `Transport::Wasm` arm and change `bytes` to `&bytes` where it's passed to functions expecting `&[u8]`:

```rust
                ModuleType::Tool => {
                    let tool = crate::transport::load_wasm_tool(&bytes, engine)?;
```

Similarly for Hook, Context, Approval, Provider, Orchestrator arms — each `bytes` reference needs to become `&bytes`.

### Step 6: Update existing tests that construct WasmBytes with empty bytes

Find the test `parse_toml_wasm_transport` (line 662) that currently expects `WasmBytes`:

**Find:**
```rust
        match &manifest.artifact {
            ModuleArtifact::WasmBytes {
                path: wasm_path, ..
            } => {
                assert_eq!(wasm_path, &PathBuf::from("/modules/my-hook/my-hook.wasm"));
            }
            other => panic!("expected WasmBytes, got {other:?}"),
        }
```

**Replace with:**
```rust
        match &manifest.artifact {
            ModuleArtifact::WasmPath(wasm_path) => {
                assert_eq!(wasm_path, &PathBuf::from("/modules/my-hook/my-hook.wasm"));
            }
            other => panic!("expected WasmPath, got {other:?}"),
        }
```

Also update the `module_manifest_can_be_constructed` test (line 554) which constructs `WasmBytes` directly — this test can keep using `WasmBytes` since it's testing that variant directly.

Search for ALL other match arms on `ModuleArtifact` throughout the **entire crate** (not just this file) to ensure exhaustive matching still compiles:

```bash
cd /home/bkrabach/dev/rust-devrust-core/amplifier-core
grep -rn "ModuleArtifact::" crates/amplifier-core/src/ --include="*.rs"
```

Fix any non-exhaustive match warnings by adding `ModuleArtifact::WasmPath(_)` arms. Check:
- `bindings/node/src/lib.rs` — if it matches on `ModuleArtifact`, add the new arm.

### Step 7: Run full verification suite

```bash
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

### Step 8: Commit

```bash
git add crates/amplifier-core/src/module_resolver.rs
# Also add any other files that needed match arm updates
git commit -m "refactor(resolver): add WasmPath variant to distinguish pre-load from loaded WASM artifacts

ModuleArtifact now has WasmPath(PathBuf) for the deferred-load case
(returned by parse_amplifier_toml) and WasmBytes for the eagerly-loaded
case (returned by resolve_module). Previously, parse_amplifier_toml
returned WasmBytes with empty bytes — a confusing sentinel value.

load_module now handles WasmPath by reading bytes from disk.

Addresses M-04 from PR #39 code review."
```

---

## Task 9: M-08 — Add optional sha256 field for WASM module integrity verification (DEFERRABLE)

> **This task is deferrable.** If time is running out, skip it and note the deferral in the PR description.

**Files:**
- Modify: `crates/amplifier-core/Cargo.toml`
- Modify: `crates/amplifier-core/src/module_resolver.rs`

### Step 1: Add sha2 dependency

In `crates/amplifier-core/Cargo.toml`, add `sha2` as a WASM-feature-gated dependency:

**Find:**
```toml
wasmtime = { version = "42", optional = true, features = ["component-model"] }
wasmtime-wasi = { version = "42", optional = true }
```

**Replace with:**
```toml
wasmtime = { version = "42", optional = true, features = ["component-model"] }
wasmtime-wasi = { version = "42", optional = true }
sha2 = { version = "0.10", optional = true }
```

**Find:**
```toml
wasm = ["wasmtime", "wasmtime-wasi"]
```

**Replace with:**
```toml
wasm = ["wasmtime", "wasmtime-wasi", "sha2"]
```

### Step 2: Write failing tests

In `crates/amplifier-core/src/module_resolver.rs`, add to the test module:

```rust
    #[cfg(feature = "wasm")]
    #[test]
    fn verify_wasm_integrity_matching_hash_passes() {
        use sha2::{Digest, Sha256};
        let bytes = b"fake wasm bytes for testing";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let result = verify_wasm_integrity(bytes, &hash);
        assert!(result.is_ok(), "Matching hash should pass: {result:?}");
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn verify_wasm_integrity_mismatched_hash_fails() {
        let bytes = b"fake wasm bytes for testing";
        let result = verify_wasm_integrity(bytes, "0000000000000000000000000000000000000000000000000000000000000000");
        assert!(result.is_err(), "Mismatched hash should fail");
        let err = result.unwrap_err();
        assert!(
            matches!(err, ModuleResolverError::IntegrityMismatch { .. }),
            "Expected IntegrityMismatch error, got: {err:?}"
        );
    }

    #[test]
    fn parse_toml_wasm_with_sha256_field() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "my-tool.wasm"
sha256 = "abc123"
"#;
        let path = Path::new("/modules/my-tool");
        let manifest = parse_amplifier_toml(toml_content, path).unwrap();
        assert_eq!(manifest.sha256, Some("abc123".to_string()));
    }

    #[test]
    fn parse_toml_wasm_without_sha256_field() {
        let toml_content = r#"
[module]
transport = "wasm"
type = "tool"
artifact = "my-tool.wasm"
"#;
        let path = Path::new("/modules/my-tool");
        let manifest = parse_amplifier_toml(toml_content, path).unwrap();
        assert_eq!(manifest.sha256, None);
    }
```

### Step 3: Run tests to verify they fail

```bash
cargo test -p amplifier-core --features wasm -- verify_wasm_integrity -v
cargo test -p amplifier-core --features wasm -- parse_toml_wasm_with_sha256 -v
```

Expected: FAIL — functions and fields don't exist yet.

### Step 4: Add sha256 field to ModuleManifest

In `crates/amplifier-core/src/module_resolver.rs`, find `ModuleManifest` (line 226):

**Find:**
```rust
pub struct ModuleManifest {
    /// Transport to use for loading (Python, WASM, gRPC).
    pub transport: Transport,
    /// Module type (Tool, Provider, Orchestrator, etc.).
    pub module_type: ModuleType,
    /// Where the loadable artifact lives.
    pub artifact: ModuleArtifact,
}
```

**Replace with:**
```rust
pub struct ModuleManifest {
    /// Transport to use for loading (Python, WASM, gRPC).
    pub transport: Transport,
    /// Module type (Tool, Provider, Orchestrator, etc.).
    pub module_type: ModuleType,
    /// Where the loadable artifact lives.
    pub artifact: ModuleArtifact,
    /// Optional SHA-256 hash for WASM module integrity verification.
    /// If present, the loaded bytes must match this hex-encoded hash.
    pub sha256: Option<String>,
}
```

### Step 5: Add IntegrityMismatch error variant

In the `ModuleResolverError` enum (line 356), add a new variant before the closing `}`:

**Find:**
```rust
    /// I/O error reading files.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}
```

**Replace with:**
```rust
    /// I/O error reading files.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// WASM module bytes do not match the expected sha256 hash.
    #[error("integrity check failed for {path}: expected sha256 {expected}, got {actual}")]
    IntegrityMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
}
```

### Step 6: Add verify_wasm_integrity function

Add this function near the other public functions (e.g., before `load_module`):

```rust
/// Verify WASM module bytes against an expected SHA-256 hash.
///
/// The `expected` hash must be a lowercase hex-encoded string (64 chars).
/// Returns `Ok(())` if the hash matches, or `Err(IntegrityMismatch)` if not.
#[cfg(feature = "wasm")]
pub fn verify_wasm_integrity(
    bytes: &[u8],
    expected: &str,
) -> Result<(), ModuleResolverError> {
    use sha2::{Digest, Sha256};
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected {
        Ok(())
    } else {
        Err(ModuleResolverError::IntegrityMismatch {
            path: PathBuf::from("<in-memory>"),
            expected: expected.to_string(),
            actual,
        })
    }
}
```

### Step 7: Parse sha256 from amplifier.toml

In `parse_amplifier_toml`, find where `ModuleManifest` is constructed (around line 217):

**Find:**
```rust
    Ok(ModuleManifest {
        transport,
        module_type,
        artifact,
    })
```

**Replace with:**
```rust
    let sha256 = module_section
        .get("sha256")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ModuleManifest {
        transport,
        module_type,
        artifact,
        sha256,
    })
```

### Step 8: Wire verification into load_module

In `load_module`, after reading bytes (the `let bytes = match ...` block), add verification before using the bytes. Find the line after the bytes match block and before the `match &manifest.module_type` dispatch:

Add after the bytes extraction:
```rust
            // Verify integrity if a sha256 hash was specified
            if let Some(expected_hash) = &manifest.sha256 {
                let path = match &manifest.artifact {
                    ModuleArtifact::WasmPath(p) => p.clone(),
                    ModuleArtifact::WasmBytes { path, .. } => path.clone(),
                    _ => PathBuf::from("<unknown>"),
                };
                let actual = {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(&bytes))
                };
                if actual != *expected_hash {
                    return Err(Box::new(ModuleResolverError::IntegrityMismatch {
                        path,
                        expected: expected_hash.clone(),
                        actual,
                    }));
                }
            }
```

### Step 9: Fix all ModuleManifest construction sites

Every place that constructs a `ModuleManifest` now needs the `sha256` field. Search:

```bash
grep -rn "ModuleManifest {" crates/amplifier-core/src/ --include="*.rs"
```

Add `sha256: None,` to every construction site in:
- `resolve_module` (line 322) — the `.wasm` auto-detect path
- `resolve_module` — any other construction paths
- Test code — every `ModuleManifest { ... }` in tests

For the `resolve_module` `.wasm` auto-detect path, `sha256` is always `None` since there's no toml to read it from:
```rust
            return Ok(ModuleManifest {
                transport: Transport::Wasm,
                module_type,
                artifact: ModuleArtifact::WasmBytes {
                    bytes,
                    path: wasm_path,
                },
                sha256: None,
            });
```

### Step 10: Run full verification suite

```bash
cargo test -p amplifier-core --features wasm
cargo clippy -p amplifier-core --features wasm -- -D warnings
cargo clippy -p amplifier-core -- -D warnings
```

### Step 11: Commit

```bash
git add crates/amplifier-core/Cargo.toml crates/amplifier-core/src/module_resolver.rs
git commit -m "feat(resolver): add optional sha256 integrity verification for WASM modules

amplifier.toml [module] section now supports an optional 'sha256' field
containing a hex-encoded SHA-256 hash. When present, load_module verifies
the WASM bytes match before passing them to wasmtime.

New error variant: ModuleResolverError::IntegrityMismatch.
New function: verify_wasm_integrity(bytes, expected_hash).
New dependency: sha2 (feature-gated under 'wasm').

Addresses M-08 from PR #39 code review."
```

---

## Final Steps

### Push and create PR

```bash
git log --oneline -10
# Verify all 8-9 commits are present and correctly ordered

git push origin fix/pr39-medium-priority-items
```

Then create a PR:
```bash
gh pr create \
  --base main \
  --title "fix: PR #39 medium-priority code quality improvements (M-01 through M-09)" \
  --body "## Summary

Implements 9 medium-priority code quality items from the 4-agent code review of PR #39 (Cross-Language SDK, v1.1.0).

## Changes (one commit each)

1. **M-09**: Clamp hook-collect timeout to 300s max (fixes NaN/Infinity panic)
2. **M-03**: Replace unwrap() with swap_remove(0) in module resolver
3. **M-05**: Add debug_assert! guards before 5 block_on calls in orchestrator bridge
4. **M-06**: Remove unused \`_reason\` parameter from Node cancellation methods
5. **M-02**: Extract \`to_json_or_warn\`/\`from_json_or_default\` helpers (replaces 26+ repeated patterns)
6. **M-01**: Extract shared \`get_typed_func\` to bridges/mod.rs (deduplicates 6 files)
7. **M-07**: Remove dead API surface (JsToolResult, JsToolSpec, JsSessionConfig, Role)
8. **M-04**: Introduce \`WasmPath(PathBuf)\` variant in ModuleArtifact
9. **M-08**: Add optional sha256 integrity verification for WASM modules *(if completed)*

## Testing

All commits gated by:
- \`cargo test -p amplifier-core --features wasm\`
- \`cargo clippy -p amplifier-core --features wasm -- -D warnings\`
- \`cargo clippy -p amplifier-core -- -D warnings\`
- \`cd bindings/node && npm run build && npx vitest run\` (for Node tasks)
"
```