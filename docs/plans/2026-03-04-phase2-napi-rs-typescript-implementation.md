# Cross-Language SDK Phase 2: TypeScript/Napi-RS Bindings — Implementation Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Deliver TypeScript/Node.js bindings for the amplifier-core Rust kernel via Napi-RS, enabling TypeScript host apps and in-process modules — while batching two dependency security upgrades (pyo3, wasmtime).

**Architecture:** A single Napi-RS crate at `bindings/node/` mirrors the proven Python/PyO3 bridge pattern. Four classes (`AmplifierSession`, `Coordinator`, `HookRegistry`, `CancellationToken`) wrap the same Rust kernel types. Six module interfaces (`Tool`, `Provider`, `Orchestrator`, `ContextManager`, `HookHandler`, `ApprovalProvider`) use `ThreadsafeFunction` for JS↔Rust callback bridging. A hybrid coordinator stores JS module objects in a JS-side `Map` while the Rust kernel handles config, tracking, and cancellation.

**Tech Stack:** Rust + Napi-RS (`napi` 2.x, `napi-derive`, `napi-build`), TypeScript + Node.js, Vitest for testing, tokio for async runtime.

**Design doc:** `docs/plans/2026-03-04-phase2-napi-rs-typescript-design.md`

---

## Orientation: What is this codebase?

`amplifier-core` is a pure Rust kernel for modular AI agent orchestration. It has **zero** Python dependency — language bindings wrap it via FFI. The project structure:

```
amplifier-core/
├── Cargo.toml                          # Workspace root (members: crates/amplifier-core, bindings/python)
├── crates/amplifier-core/              # The Rust kernel — all core types live here
│   └── src/
│       ├── lib.rs                      # Re-exports everything
│       ├── session.rs                  # Session + SessionConfig
│       ├── coordinator.rs              # Coordinator (module mount points)
│       ├── hooks.rs                    # HookRegistry (event dispatch)
│       ├── cancellation.rs             # CancellationToken (cooperative cancel)
│       ├── traits.rs                   # 6 module traits: Tool, Provider, Orchestrator, ContextManager, HookHandler, ApprovalProvider
│       ├── models.rs                   # HookResult, ToolResult, HookAction, SessionState, etc.
│       ├── messages.rs                 # ChatRequest, ChatResponse, Message, Role, ToolSpec, etc.
│       ├── errors.rs                   # AmplifierError, ProviderError, ToolError, etc.
│       ├── events.rs                   # Event name constants (SESSION_START, TOOL_PRE, etc.)
│       └── bridges/wasm_tool.rs        # WASM tool bridge (needs wasmtime upgrade fix)
├── bindings/python/                    # PyO3 bridge — THE reference for our Napi-RS bridge
│   ├── Cargo.toml                      # pyo3 0.28 (needs bump to 0.28.2)
│   └── src/lib.rs                      # ~2,885 lines: PySession, PyCoordinator, PyHookRegistry, PyCancellationToken
└── bindings/node/                      # ← WE ARE CREATING THIS
```

The Python bridge at `bindings/python/src/lib.rs` is the pattern we mirror for every task.

---

## Task 0: Dependency Upgrades

**Why:** pyo3 has a HIGH severity security fix, wasmtime has 8 Dependabot alerts. We batch these since we're touching Cargo.toml anyway.

**Files:**
- Modify: `bindings/python/Cargo.toml` (pyo3 version bump)
- Modify: `crates/amplifier-core/Cargo.toml` (wasmtime version bump)
- Modify: `crates/amplifier-core/src/bridges/wasm_tool.rs` (fix API breakage)

### Step 1: Bump pyo3 to 0.28.2

Open `bindings/python/Cargo.toml`. Change:

```toml
# FROM:
pyo3 = { version = "0.28", features = ["generate-import-lib"] }
pyo3-async-runtimes = { version = "0.28", features = ["tokio-runtime"] }

# TO:
pyo3 = { version = "0.28.2", features = ["generate-import-lib"] }
pyo3-async-runtimes = { version = "0.28.2", features = ["tokio-runtime"] }
```

### Step 2: Bump wasmtime to latest

Open `crates/amplifier-core/Cargo.toml`. Change:

```toml
# FROM:
wasmtime = { version = "29", optional = true }

# TO:
wasmtime = { version = "31", optional = true }
```

> **Note:** We target v31, not v42. The wasmtime crate on crates.io shows v31 as latest stable at time of writing. Check `cargo search wasmtime` to confirm the actual latest version and adjust accordingly. The key point is: bump from v29 to whatever latest stable is available.

### Step 3: Fix WASM bridge API breakage

After bumping wasmtime, there may be API changes. The WASM bridge is minimal — it only uses `Engine::default()`, `Module::new()`, and `Module::name()`. Open `crates/amplifier-core/src/bridges/wasm_tool.rs` and check if these APIs still compile.

The current code (which should still work, but verify):

```rust
pub fn from_bytes(wasm_bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, wasm_bytes)?;
    let name = module.name().unwrap_or("wasm-tool").to_string();
    // ...
}
```

If `Module::name()` signature changed (e.g., returns `&str` vs `Option<&str>`), fix accordingly. The wasmtime API between v29→v31 is usually source-compatible for these basics.

### Step 4: Build and verify

Run:
```bash
cd amplifier-core && cargo build --all-features 2>&1
```
Expected: Clean build with no errors.

### Step 5: Run all Rust tests

Run:
```bash
cd amplifier-core && cargo test --all 2>&1
```
Expected: All 312+ tests pass (the exact count may vary).

### Step 6: Commit

```bash
cd amplifier-core && git add bindings/python/Cargo.toml crates/amplifier-core/Cargo.toml crates/amplifier-core/src/bridges/wasm_tool.rs && git commit -m "chore: bump pyo3 to 0.28.2 and wasmtime to latest (security fixes)"
```

---

## Task 1: Napi-RS Scaffold

**Why:** Create the empty `bindings/node/` crate with a single `#[napi]` function to prove the build pipeline works end-to-end: Rust compiles → native `.node` addon generated → `index.js` + `index.d.ts` auto-created → importable from Node.js.

**Files:**
- Create: `bindings/node/Cargo.toml`
- Create: `bindings/node/src/lib.rs`
- Create: `bindings/node/build.rs`
- Create: `bindings/node/package.json`
- Create: `bindings/node/tsconfig.json`
- Create: `bindings/node/__tests__/smoke.test.ts`
- Modify: `Cargo.toml` (workspace root — add member)

### Step 1: Add bindings/node to workspace members

Open the workspace root `Cargo.toml`. Change:

```toml
# FROM:
[workspace]
members = [
    "crates/amplifier-core",
    "bindings/python",
]

# TO:
[workspace]
members = [
    "crates/amplifier-core",
    "bindings/python",
    "bindings/node",
]
```

### Step 2: Create bindings/node/Cargo.toml

Create the file `bindings/node/Cargo.toml`:

```toml
[package]
name = "amplifier-core-node"
version = "1.0.10"
edition = "2021"
description = "Napi-RS bridge for amplifier-core Rust kernel"
license = "MIT"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
amplifier-core = { path = "../../crates/amplifier-core" }
napi = { version = "2", features = ["async", "serde-json", "napi9"] }
napi-derive = "2"
tokio = { version = "1", features = ["rt-multi-thread"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }

[build-dependencies]
napi-build = "2"
```

### Step 3: Create bindings/node/build.rs

Create the file `bindings/node/build.rs`:

```rust
extern crate napi_build;

fn main() {
    napi_build::setup();
}
```

### Step 4: Create minimal bindings/node/src/lib.rs

Create the file `bindings/node/src/lib.rs`:

```rust
//! Napi-RS bridge for amplifier-core.
//!
//! This crate wraps the pure Rust kernel types and exposes them
//! as JavaScript/TypeScript classes via Napi-RS. It compiles into
//! a native `.node` addon that ships inside an npm package.
//!
//! # Exposed classes
//!
//! | TypeScript name         | Rust wrapper              | Inner type                        |
//! |-------------------------|---------------------------|-----------------------------------|
//! | `AmplifierSession`      | `JsSession`               | `amplifier_core::Session`         |
//! | `HookRegistry`          | `JsHookRegistry`          | `amplifier_core::HookRegistry`    |
//! | `CancellationToken`     | `JsCancellationToken`     | `amplifier_core::CancellationToken` |
//! | `Coordinator`           | `JsCoordinator`           | `amplifier_core::Coordinator`     |

#[macro_use]
extern crate napi_derive;

/// Smoke test: returns a greeting string from the native addon.
/// Remove this once real bindings are in place.
#[napi]
pub fn hello() -> String {
    "Hello from amplifier-core native addon!".to_string()
}
```

### Step 5: Create bindings/node/package.json

Create the file `bindings/node/package.json`:

```json
{
  "name": "amplifier-core",
  "version": "1.0.10",
  "description": "TypeScript/Node.js bindings for amplifier-core Rust kernel",
  "main": "index.js",
  "types": "index.d.ts",
  "scripts": {
    "build": "napi build --release --platform",
    "build:debug": "napi build --platform",
    "test": "vitest run"
  },
  "napi": {
    "name": "amplifier-core",
    "triples": {}
  },
  "license": "MIT",
  "devDependencies": {
    "@napi-rs/cli": "^2",
    "vitest": "^3",
    "typescript": "^5"
  }
}
```

### Step 6: Create bindings/node/tsconfig.json

Create the file `bindings/node/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "node16",
    "moduleResolution": "node16",
    "strict": true,
    "esModuleInterop": true,
    "outDir": "dist",
    "declaration": true,
    "types": ["vitest/globals"]
  },
  "include": ["__tests__/**/*.ts"]
}
```

### Step 7: Install npm dependencies and build

Run:
```bash
cd amplifier-core/bindings/node && npm install && npm run build:debug 2>&1
```
Expected: Build succeeds. You should see `amplifier-core.linux-arm64-gnu.node` (or similar platform-specific name) in the directory. Napi-RS also generates `index.js` and `index.d.ts`.

### Step 8: Write the smoke test

Create the file `bindings/node/__tests__/smoke.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { hello } from '../index.js';

describe('native addon smoke test', () => {
  it('loads the native addon and calls hello()', () => {
    const result = hello();
    expect(result).toBe('Hello from amplifier-core native addon!');
  });
});
```

### Step 9: Run the smoke test

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run 2>&1
```
Expected: 1 test passes.

### Step 10: Commit

```bash
cd amplifier-core && git add Cargo.toml bindings/node/ && git commit -m "feat(node): scaffold Napi-RS crate with smoke test"
```

---

## Task 2: Data Model Types

**Why:** All other tasks depend on these types. Enums become TypeScript string unions via `#[napi(string_enum)]`. Structs become TypeScript interfaces via `#[napi(object)]`. This establishes the typed data contract across the FFI boundary.

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Create: `bindings/node/__tests__/types.test.ts`

### Step 1: Write the failing test

Create the file `bindings/node/__tests__/types.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import {
  HookAction,
  SessionState,
  ContextInjectionRole,
  ApprovalDefault,
  UserMessageLevel,
  Role,
} from '../index.js';

describe('enum types', () => {
  it('HookAction has all variants', () => {
    expect(HookAction.Continue).toBe('Continue');
    expect(HookAction.Deny).toBe('Deny');
    expect(HookAction.Modify).toBe('Modify');
    expect(HookAction.InjectContext).toBe('InjectContext');
    expect(HookAction.AskUser).toBe('AskUser');
  });

  it('SessionState has all variants', () => {
    expect(SessionState.Running).toBe('Running');
    expect(SessionState.Completed).toBe('Completed');
    expect(SessionState.Failed).toBe('Failed');
    expect(SessionState.Cancelled).toBe('Cancelled');
  });

  it('ContextInjectionRole has all variants', () => {
    expect(ContextInjectionRole.System).toBe('System');
    expect(ContextInjectionRole.User).toBe('User');
    expect(ContextInjectionRole.Assistant).toBe('Assistant');
  });

  it('ApprovalDefault has all variants', () => {
    expect(ApprovalDefault.Allow).toBe('Allow');
    expect(ApprovalDefault.Deny).toBe('Deny');
  });

  it('UserMessageLevel has all variants', () => {
    expect(UserMessageLevel.Info).toBe('Info');
    expect(UserMessageLevel.Warning).toBe('Warning');
    expect(UserMessageLevel.Error).toBe('Error');
  });

  it('Role has all variants', () => {
    expect(Role.System).toBe('System');
    expect(Role.User).toBe('User');
    expect(Role.Assistant).toBe('Assistant');
    expect(Role.Tool).toBe('Tool');
  });
});
```

### Step 2: Run test to verify it fails

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run __tests__/types.test.ts 2>&1
```
Expected: FAIL — imports don't exist yet.

### Step 3: Implement the enums in lib.rs

Open `bindings/node/src/lib.rs`. Add the enum definitions after the `hello()` function:

```rust
// ---------------------------------------------------------------------------
// Enums — TypeScript string enums via #[napi(string_enum)]
// ---------------------------------------------------------------------------

/// Action type for hook results.
#[napi(string_enum)]
pub enum HookAction {
    Continue,
    Deny,
    Modify,
    InjectContext,
    AskUser,
}

/// Session lifecycle state.
#[napi(string_enum)]
pub enum SessionState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Role for context injection messages.
#[napi(string_enum)]
pub enum ContextInjectionRole {
    System,
    User,
    Assistant,
}

/// Default decision on approval timeout.
#[napi(string_enum)]
pub enum ApprovalDefault {
    Allow,
    Deny,
}

/// Severity level for user messages from hooks.
#[napi(string_enum)]
pub enum UserMessageLevel {
    Info,
    Warning,
    Error,
}

/// Message role in conversation.
#[napi(string_enum)]
pub enum Role {
    System,
    Developer,
    User,
    Assistant,
    Function,
    Tool,
}

// ---------------------------------------------------------------------------
// Conversion helpers: Napi enums ↔ amplifier_core enums
// ---------------------------------------------------------------------------

impl From<HookAction> for amplifier_core::models::HookAction {
    fn from(val: HookAction) -> Self {
        match val {
            HookAction::Continue => amplifier_core::models::HookAction::Continue,
            HookAction::Deny => amplifier_core::models::HookAction::Deny,
            HookAction::Modify => amplifier_core::models::HookAction::Modify,
            HookAction::InjectContext => amplifier_core::models::HookAction::InjectContext,
            HookAction::AskUser => amplifier_core::models::HookAction::AskUser,
        }
    }
}

impl From<amplifier_core::models::HookAction> for HookAction {
    fn from(val: amplifier_core::models::HookAction) -> Self {
        match val {
            amplifier_core::models::HookAction::Continue => HookAction::Continue,
            amplifier_core::models::HookAction::Deny => HookAction::Deny,
            amplifier_core::models::HookAction::Modify => HookAction::Modify,
            amplifier_core::models::HookAction::InjectContext => HookAction::InjectContext,
            amplifier_core::models::HookAction::AskUser => HookAction::AskUser,
        }
    }
}

impl From<SessionState> for amplifier_core::models::SessionState {
    fn from(val: SessionState) -> Self {
        match val {
            SessionState::Running => amplifier_core::models::SessionState::Running,
            SessionState::Completed => amplifier_core::models::SessionState::Completed,
            SessionState::Failed => amplifier_core::models::SessionState::Failed,
            SessionState::Cancelled => amplifier_core::models::SessionState::Cancelled,
        }
    }
}

impl From<amplifier_core::models::SessionState> for SessionState {
    fn from(val: amplifier_core::models::SessionState) -> Self {
        match val {
            amplifier_core::models::SessionState::Running => SessionState::Running,
            amplifier_core::models::SessionState::Completed => SessionState::Completed,
            amplifier_core::models::SessionState::Failed => SessionState::Failed,
            amplifier_core::models::SessionState::Cancelled => SessionState::Cancelled,
        }
    }
}

// ---------------------------------------------------------------------------
// Structs — TypeScript interfaces via #[napi(object)]
// ---------------------------------------------------------------------------

/// Tool execution result — crosses the FFI boundary as a plain JS object.
#[napi(object)]
pub struct JsToolResult {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// Tool specification — describes a tool's interface.
#[napi(object)]
pub struct JsToolSpec {
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema parameters as a JSON string.
    pub parameters_json: String,
}

/// Hook result — the return value from hook handlers.
#[napi(object)]
pub struct JsHookResult {
    pub action: HookAction,
    pub reason: Option<String>,
    pub context_injection: Option<String>,
    pub context_injection_role: Option<ContextInjectionRole>,
    pub ephemeral: Option<bool>,
    pub suppress_output: Option<bool>,
    pub user_message: Option<String>,
    pub user_message_level: Option<UserMessageLevel>,
    pub user_message_source: Option<String>,
    pub approval_prompt: Option<String>,
    pub approval_timeout: Option<f64>,
    pub approval_default: Option<ApprovalDefault>,
}

/// Session configuration — typed config for AmplifierSession constructor.
#[napi(object)]
pub struct JsSessionConfig {
    /// Full config as a JSON string. The Rust kernel parses and validates it.
    pub config_json: String,
}
```

### Step 4: Rebuild and run tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run __tests__/types.test.ts 2>&1
```
Expected: All enum tests pass.

### Step 5: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "feat(node): add data model types — enums and structs"
```

---

## Task 3: CancellationToken

**Why:** Simplest of the four classes — no async, no subsystem dependencies. Perfect starting point to prove the `#[napi]` class pattern works.

**Reference:** The Rust type is `amplifier_core::CancellationToken` in `crates/amplifier-core/src/cancellation.rs`. It uses `Arc<Mutex<Inner>>` internally and is already `Clone + Send + Sync`. The Python equivalent is `PyCancellationToken` in `bindings/python/src/lib.rs`.

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Create: `bindings/node/__tests__/cancellation.test.ts`

### Step 1: Write the failing test

Create the file `bindings/node/__tests__/cancellation.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { JsCancellationToken } from '../index.js';

describe('CancellationToken', () => {
  it('creates with default state (not cancelled)', () => {
    const token = new JsCancellationToken();
    expect(token.isCancelled).toBe(false);
    expect(token.isGraceful).toBe(false);
    expect(token.isImmediate).toBe(false);
  });

  it('requestGraceful transitions to graceful', () => {
    const token = new JsCancellationToken();
    token.requestGraceful();
    expect(token.isCancelled).toBe(true);
    expect(token.isGraceful).toBe(true);
    expect(token.isImmediate).toBe(false);
  });

  it('requestImmediate transitions to immediate', () => {
    const token = new JsCancellationToken();
    token.requestImmediate();
    expect(token.isCancelled).toBe(true);
    expect(token.isImmediate).toBe(true);
  });

  it('graceful then immediate escalates', () => {
    const token = new JsCancellationToken();
    token.requestGraceful();
    expect(token.isGraceful).toBe(true);
    token.requestImmediate();
    expect(token.isImmediate).toBe(true);
  });

  it('reset returns to uncancelled state', () => {
    const token = new JsCancellationToken();
    token.requestGraceful();
    expect(token.isCancelled).toBe(true);
    token.reset();
    expect(token.isCancelled).toBe(false);
  });

  it('requestGraceful with reason', () => {
    const token = new JsCancellationToken();
    token.requestGraceful('user pressed Ctrl+C');
    expect(token.isGraceful).toBe(true);
  });

  it('requestImmediate with reason', () => {
    const token = new JsCancellationToken();
    token.requestImmediate('timeout exceeded');
    expect(token.isImmediate).toBe(true);
  });
});
```

### Step 2: Run test to verify it fails

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run __tests__/cancellation.test.ts 2>&1
```
Expected: FAIL — `JsCancellationToken` doesn't exist yet.

### Step 3: Implement JsCancellationToken

Open `bindings/node/src/lib.rs`. Add:

```rust
use std::sync::Arc;

// ---------------------------------------------------------------------------
// JsCancellationToken — wraps amplifier_core::CancellationToken
// ---------------------------------------------------------------------------

/// Cooperative cancellation token.
///
/// State machine: None → Graceful → Immediate.
/// Thread-safe: backed by Arc<Mutex> in the Rust kernel.
#[napi]
pub struct JsCancellationToken {
    inner: amplifier_core::CancellationToken,
}

#[napi]
impl JsCancellationToken {
    /// Create a new token in the uncancelled state.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: amplifier_core::CancellationToken::new(),
        }
    }

    /// Create from an existing Rust CancellationToken (internal use).
    pub fn from_inner(inner: amplifier_core::CancellationToken) -> Self {
        Self { inner }
    }

    /// True if any cancellation has been requested (graceful or immediate).
    #[napi(getter)]
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// True if graceful cancellation (wait for current tools to complete).
    #[napi(getter)]
    pub fn is_graceful(&self) -> bool {
        self.inner.is_graceful()
    }

    /// True if immediate cancellation (stop now).
    #[napi(getter)]
    pub fn is_immediate(&self) -> bool {
        self.inner.is_immediate()
    }

    /// Request graceful cancellation. Waits for current tools to complete.
    #[napi]
    pub fn request_graceful(&self, _reason: Option<String>) {
        self.inner.request_graceful();
    }

    /// Request immediate cancellation. Stops as soon as possible.
    #[napi]
    pub fn request_immediate(&self, _reason: Option<String>) {
        self.inner.request_immediate();
    }

    /// Reset cancellation state. Called at turn boundaries.
    #[napi]
    pub fn reset(&self) {
        self.inner.reset();
    }
}
```

> **Note:** The `_reason` parameter is accepted but not yet stored (matching the current Rust kernel API which doesn't have a reason field). This is forward-compatible with a future enhancement.

### Step 4: Rebuild and run tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run __tests__/cancellation.test.ts 2>&1
```
Expected: All 7 tests pass.

### Step 5: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "feat(node): add CancellationToken binding"
```

---

## Task 4: HookRegistry

**Why:** The hook system is the event backbone of the kernel. This task wraps `amplifier_core::HookRegistry` and implements `JsHookHandlerBridge` — the struct that lets JS functions act as Rust `HookHandler` trait objects via `ThreadsafeFunction`.

**Reference:** The Rust type is `amplifier_core::HookRegistry` in `crates/amplifier-core/src/hooks.rs`. The Python equivalent is `PyHookRegistry` + `PyHookHandlerBridge` in `bindings/python/src/lib.rs` (lines 53–181).

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Create: `bindings/node/__tests__/hooks.test.ts`

### Step 1: Write the failing test

Create the file `bindings/node/__tests__/hooks.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { JsHookRegistry, HookAction } from '../index.js';

describe('HookRegistry', () => {
  it('creates empty registry', () => {
    const registry = new JsHookRegistry();
    const handlers = registry.listHandlers();
    expect(Object.keys(handlers).length).toBe(0);
  });

  it('emits with no handlers returns Continue', async () => {
    const registry = new JsHookRegistry();
    const result = await registry.emit('test:event', '{}');
    expect(result.action).toBe(HookAction.Continue);
  });

  it('registers and emits to a JS handler', async () => {
    const registry = new JsHookRegistry();
    let handlerCalled = false;
    let receivedEvent = '';

    registry.register('test:event', (_event: string, _data: string) => {
      handlerCalled = true;
      receivedEvent = _event;
      return JSON.stringify({ action: 'continue' });
    }, 0, 'test-handler');

    await registry.emit('test:event', JSON.stringify({ key: 'value' }));
    expect(handlerCalled).toBe(true);
    expect(receivedEvent).toBe('test:event');
  });

  it('listHandlers returns registered handler names', () => {
    const registry = new JsHookRegistry();
    registry.register('tool:pre', (_e: string, _d: string) => {
      return JSON.stringify({ action: 'continue' });
    }, 0, 'my-hook');

    const handlers = registry.listHandlers();
    expect(handlers['tool:pre']).toBeDefined();
    expect(handlers['tool:pre']).toContain('my-hook');
  });

  it('handler returning deny stops pipeline', async () => {
    const registry = new JsHookRegistry();
    registry.register('test:event', (_e: string, _d: string) => {
      return JSON.stringify({ action: 'deny', reason: 'blocked' });
    }, 0, 'denier');

    const result = await registry.emit('test:event', '{}');
    expect(result.action).toBe(HookAction.Deny);
    expect(result.reason).toBe('blocked');
  });

  it('setDefaultFields merges into emit data', async () => {
    const registry = new JsHookRegistry();
    let receivedData = '';

    registry.register('test:event', (_e: string, data: string) => {
      receivedData = data;
      return JSON.stringify({ action: 'continue' });
    }, 0, 'capture');

    registry.setDefaultFields(JSON.stringify({ session_id: 'test-123' }));
    await registry.emit('test:event', JSON.stringify({ custom: true }));

    const parsed = JSON.parse(receivedData);
    expect(parsed.session_id).toBe('test-123');
    expect(parsed.custom).toBe(true);
  });
});
```

### Step 2: Run test to verify it fails

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run __tests__/hooks.test.ts 2>&1
```
Expected: FAIL — `JsHookRegistry` doesn't exist yet.

### Step 3: Implement JsHookHandlerBridge and JsHookRegistry

Open `bindings/node/src/lib.rs`. Add these imports at the top (merge with existing):

```rust
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use napi::threadsafe_function::{ThreadsafeFunction, ErrorStrategy, ThreadSafeCallContext};
use napi::bindgen_prelude::*;

use amplifier_core::errors::HookError;
use amplifier_core::models::HookResult;
use amplifier_core::traits::HookHandler;
```

Then add the bridge and registry:

```rust
// ---------------------------------------------------------------------------
// JsHookHandlerBridge — wraps a JS callback as a Rust HookHandler
// ---------------------------------------------------------------------------

/// Bridges a JavaScript function into the Rust HookHandler trait.
///
/// Holds a ThreadsafeFunction reference to the JS callback. When the Rust
/// HookRegistry fires an event, it calls through this bridge back into JS.
///
/// The JS callback signature is: (event: string, data: string) => string
/// where `data` and the return value are JSON strings.
struct JsHookHandlerBridge {
    callback: ThreadsafeFunction<(String, String), ErrorStrategy::Fatal>,
}

unsafe impl Send for JsHookHandlerBridge {}
unsafe impl Sync for JsHookHandlerBridge {}

impl HookHandler for JsHookHandlerBridge {
    fn handle(
        &self,
        event: &str,
        data: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<HookResult, HookError>> + Send + '_>> {
        let event = event.to_string();
        let data_str = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
        let callback = self.callback.clone();

        Box::pin(async move {
            // Call into JS via ThreadsafeFunction
            let result_str: String = callback
                .call_async((event.clone(), data_str))
                .await
                .map_err(|e| HookError::HandlerFailed {
                    message: format!("JS hook handler error: {e}"),
                    handler_name: None,
                })?;

            // Parse the JSON string returned by JS into a HookResult
            let hook_result: HookResult =
                serde_json::from_str(&result_str).unwrap_or_else(|e| {
                    log::warn!(
                        "Failed to parse JS hook handler result (defaulting to Continue): {e}"
                    );
                    HookResult::default()
                });

            Ok(hook_result)
        })
    }
}

// ---------------------------------------------------------------------------
// JsHookRegistry — wraps amplifier_core::HookRegistry
// ---------------------------------------------------------------------------

/// Hook event dispatch registry.
///
/// Handlers execute sequentially by priority. Deny short-circuits the chain.
#[napi]
pub struct JsHookRegistry {
    pub(crate) inner: Arc<amplifier_core::HookRegistry>,
}

#[napi]
impl JsHookRegistry {
    /// Create an empty hook registry.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(amplifier_core::HookRegistry::new()),
        }
    }

    /// Create from an existing Rust HookRegistry (internal use).
    pub fn from_inner(inner: &amplifier_core::HookRegistry) -> Self {
        // Note: HookRegistry is not Clone, so we can't wrap an existing one.
        // For coordinator integration, we'll need a different approach.
        // For now, this creates a new one.
        Self {
            inner: Arc::new(amplifier_core::HookRegistry::new()),
        }
    }

    /// Register a JS function as a hook handler.
    ///
    /// The callback signature is: (event: string, dataJson: string) => string
    /// It must return a JSON string of a HookResult.
    #[napi(ts_args_type = "event: string, handler: (event: string, dataJson: string) => string, priority: number, name: string")]
    pub fn register(
        &self,
        event: String,
        handler: JsFunction,
        priority: i32,
        name: String,
    ) -> Result<()> {
        // Create a ThreadsafeFunction from the JS callback
        let tsfn: ThreadsafeFunction<(String, String), ErrorStrategy::Fatal> = handler
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<(String, String)>| {
                let env = ctx.env;
                let (event, data) = ctx.value;
                Ok(vec![
                    env.create_string(&event)?.into_unknown(),
                    env.create_string(&data)?.into_unknown(),
                ])
            })?;

        let bridge = Arc::new(JsHookHandlerBridge { callback: tsfn });

        self.inner
            .register(&event, bridge, priority, Some(name));

        Ok(())
    }

    /// Emit an event. Returns the aggregated HookResult as a JsHookResult.
    ///
    /// `data_json` is a JSON string of the event payload.
    #[napi]
    pub async fn emit(&self, event: String, data_json: String) -> Result<JsHookResult> {
        let data: serde_json::Value =
            serde_json::from_str(&data_json).unwrap_or(serde_json::json!({}));

        let result = self.inner.emit(&event, data).await;

        Ok(hook_result_to_js(result))
    }

    /// List all registered handlers, grouped by event name.
    ///
    /// Returns an object where keys are event names and values are arrays of handler names.
    #[napi]
    pub fn list_handlers(&self) -> HashMap<String, Vec<String>> {
        self.inner.list_handlers(None)
    }

    /// Set default fields merged into every emit() call.
    ///
    /// `defaults_json` is a JSON string of the default fields.
    #[napi]
    pub fn set_default_fields(&self, defaults_json: String) {
        if let Ok(defaults) = serde_json::from_str(&defaults_json) {
            self.inner.set_default_fields(defaults);
        }
    }
}

/// Convert a Rust HookResult to a JS-friendly JsHookResult.
fn hook_result_to_js(result: HookResult) -> JsHookResult {
    JsHookResult {
        action: result.action.into(),
        reason: result.reason,
        context_injection: result.context_injection,
        context_injection_role: result.context_injection_role.into(),
        ephemeral: Some(result.ephemeral),
        suppress_output: Some(result.suppress_output),
        user_message: result.user_message,
        user_message_level: Some(result.user_message_level.into()),
        user_message_source: result.user_message_source,
        approval_prompt: result.approval_prompt,
        approval_timeout: Some(result.approval_timeout),
        approval_default: Some(result.approval_default.into()),
    }
}
```

> **Note:** You will also need `From` implementations for `ContextInjectionRole`, `UserMessageLevel`, and `ApprovalDefault` following the same pattern as the `HookAction` converters added in Task 2. Add those conversion impls alongside the existing ones.

### Step 4: Rebuild and run tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run __tests__/hooks.test.ts 2>&1
```
Expected: All 6 tests pass.

### Step 5: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "feat(node): add HookRegistry binding with JS handler bridge"
```

---

## Task 5: Coordinator

**Why:** The Coordinator is the central hub — it holds module mount points, capabilities, the hook registry, the cancellation token, and config. This is the "hybrid coordinator" pattern: JS-side storage for TS module objects, Rust kernel for everything else.

**Reference:** The Rust type is `amplifier_core::Coordinator` in `crates/amplifier-core/src/coordinator.rs`. The Python equivalent is `PyCoordinator` in `bindings/python/src/lib.rs`.

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Create: `bindings/node/__tests__/coordinator.test.ts`

### Step 1: Write the failing test

Create the file `bindings/node/__tests__/coordinator.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { JsCoordinator } from '../index.js';

describe('Coordinator', () => {
  it('creates with empty config', () => {
    const coord = new JsCoordinator('{}');
    expect(coord.toolNames).toEqual([]);
    expect(coord.providerNames).toEqual([]);
    expect(coord.hasOrchestrator).toBe(false);
    expect(coord.hasContext).toBe(false);
  });

  it('registers and retrieves capabilities', () => {
    const coord = new JsCoordinator('{}');
    coord.registerCapability('streaming', JSON.stringify(true));
    const cap = coord.getCapability('streaming');
    expect(cap).toBe('true');
  });

  it('getCapability returns null for missing', () => {
    const coord = new JsCoordinator('{}');
    expect(coord.getCapability('nonexistent')).toBeNull();
  });

  it('provides access to hooks subsystem', () => {
    const coord = new JsCoordinator('{}');
    const hooks = coord.hooks;
    expect(hooks).toBeDefined();
    expect(typeof hooks.listHandlers).toBe('function');
  });

  it('provides access to cancellation subsystem', () => {
    const coord = new JsCoordinator('{}');
    const cancel = coord.cancellation;
    expect(cancel).toBeDefined();
    expect(cancel.isCancelled).toBe(false);
  });

  it('resetTurn resets turn tracking', () => {
    const coord = new JsCoordinator('{}');
    // Should not throw
    coord.resetTurn();
  });

  it('toDict returns coordinator state', () => {
    const coord = new JsCoordinator('{}');
    const dict = coord.toDict();
    expect(dict).toHaveProperty('tools');
    expect(dict).toHaveProperty('providers');
    expect(dict).toHaveProperty('has_orchestrator');
    expect(dict).toHaveProperty('has_context');
    expect(dict).toHaveProperty('capabilities');
  });

  it('config returns original config', () => {
    const configJson = JSON.stringify({ session: { orchestrator: 'test' } });
    const coord = new JsCoordinator(configJson);
    const config = coord.config;
    expect(config).toBeDefined();
  });
});
```

### Step 2: Run test to verify it fails

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run __tests__/coordinator.test.ts 2>&1
```
Expected: FAIL — `JsCoordinator` doesn't exist yet.

### Step 3: Implement JsCoordinator

Open `bindings/node/src/lib.rs`. Add:

```rust
// ---------------------------------------------------------------------------
// JsCoordinator — wraps amplifier_core::Coordinator
// ---------------------------------------------------------------------------

/// Central coordination hub for module mount points, capabilities, and services.
///
/// The hybrid coordinator pattern: JS-side storage for TS module objects
/// (tools, providers, orchestrator, context), Rust kernel for config,
/// tracking, hooks, and cancellation.
#[napi]
pub struct JsCoordinator {
    pub(crate) inner: Arc<amplifier_core::Coordinator>,
}

#[napi]
impl JsCoordinator {
    /// Create a new coordinator with the given config JSON.
    #[napi(constructor)]
    pub fn new(config_json: String) -> Result<Self> {
        let config: HashMap<String, serde_json::Value> =
            serde_json::from_str(&config_json).unwrap_or_default();
        Ok(Self {
            inner: Arc::new(amplifier_core::Coordinator::new(config)),
        })
    }

    /// Names of all mounted tools (from the Rust kernel side).
    #[napi(getter)]
    pub fn tool_names(&self) -> Vec<String> {
        self.inner.tool_names()
    }

    /// Names of all mounted providers (from the Rust kernel side).
    #[napi(getter)]
    pub fn provider_names(&self) -> Vec<String> {
        self.inner.provider_names()
    }

    /// Whether an orchestrator is mounted.
    #[napi(getter)]
    pub fn has_orchestrator(&self) -> bool {
        self.inner.has_orchestrator()
    }

    /// Whether a context manager is mounted.
    #[napi(getter)]
    pub fn has_context(&self) -> bool {
        self.inner.has_context()
    }

    /// Register a capability (inter-module communication).
    #[napi]
    pub fn register_capability(&self, name: String, value_json: String) {
        if let Ok(value) = serde_json::from_str(&value_json) {
            self.inner.register_capability(&name, value);
        }
    }

    /// Get a registered capability. Returns null if not found.
    #[napi]
    pub fn get_capability(&self, name: String) -> Option<String> {
        self.inner
            .get_capability(&name)
            .map(|v| serde_json::to_string(&v).unwrap_or_default())
    }

    /// Access the hook registry subsystem.
    #[napi(getter)]
    pub fn hooks(&self) -> JsHookRegistry {
        // Note: This creates a new JsHookRegistry wrapper. For the coordinator's
        // internal hooks to be shared, we need Arc access. The Coordinator exposes
        // hooks() as &HookRegistry. For now, we create a separate registry.
        // TODO: Share the actual HookRegistry once we have Arc<Coordinator>.
        JsHookRegistry::from_inner(self.inner.hooks())
    }

    /// Access the cancellation token subsystem.
    #[napi(getter)]
    pub fn cancellation(&self) -> JsCancellationToken {
        JsCancellationToken::from_inner(self.inner.cancellation().clone())
    }

    /// Session configuration as JSON string.
    #[napi(getter)]
    pub fn config(&self) -> String {
        serde_json::to_string(self.inner.config()).unwrap_or_else(|_| "{}".to_string())
    }

    /// Reset per-turn tracking. Call at turn boundaries.
    #[napi]
    pub fn reset_turn(&self) {
        self.inner.reset_turn();
    }

    /// Return coordinator state as a JSON-compatible object.
    #[napi]
    pub fn to_dict(&self) -> HashMap<String, serde_json::Value> {
        self.inner.to_dict()
    }

    /// Run all cleanup functions.
    #[napi]
    pub async fn cleanup(&self) {
        self.inner.cleanup().await;
    }
}
```

> **Important Note:** The `hooks()` getter currently creates a wrapper but cannot share the coordinator's internal `HookRegistry` because `Coordinator::hooks()` returns `&HookRegistry` (a reference). The `JsHookRegistry` needs to own or share the registry via `Arc`. This is a known limitation that gets resolved in Task 6 when the session wires everything together. For Task 5 tests, the coordinator's own hooks will work for capability/config tests, and the hooks getter returns a working (but separate) registry. Add a TODO comment in the code.

### Step 4: Rebuild and run tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run __tests__/coordinator.test.ts 2>&1
```
Expected: All 9 tests pass.

### Step 5: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "feat(node): add Coordinator binding with hybrid pattern"
```

---

## Task 6: AmplifierSession

**Why:** The session is the top-level entry point for TypeScript consumers: `new AmplifierSession(config) → initialize() → execute(prompt) → cleanup()`. It wires together the Coordinator, HookRegistry, and CancellationToken.

**Reference:** The Rust type is `amplifier_core::Session` in `crates/amplifier-core/src/session.rs`. The Python equivalent is `PySession` in `bindings/python/src/lib.rs` (lines 200–600+).

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Create: `bindings/node/__tests__/session.test.ts`

### Step 1: Write the failing test

Create the file `bindings/node/__tests__/session.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { JsAmplifierSession } from '../index.js';

describe('AmplifierSession', () => {
  const validConfig = JSON.stringify({
    session: {
      orchestrator: 'loop-basic',
      context: 'context-simple',
    },
  });

  it('creates with valid config and generates session ID', () => {
    const session = new JsAmplifierSession(validConfig);
    expect(session.sessionId).toBeTruthy();
    expect(session.sessionId.length).toBeGreaterThan(0);
  });

  it('creates with custom session ID', () => {
    const session = new JsAmplifierSession(validConfig, 'custom-id');
    expect(session.sessionId).toBe('custom-id');
  });

  it('creates with parent ID', () => {
    const session = new JsAmplifierSession(validConfig, undefined, 'parent-123');
    expect(session.parentId).toBe('parent-123');
  });

  it('parentId is null when no parent', () => {
    const session = new JsAmplifierSession(validConfig);
    expect(session.parentId).toBeNull();
  });

  it('starts as not initialized', () => {
    const session = new JsAmplifierSession(validConfig);
    expect(session.isInitialized).toBe(false);
  });

  it('status starts as running', () => {
    const session = new JsAmplifierSession(validConfig);
    expect(session.status).toBe('running');
  });

  it('provides access to coordinator', () => {
    const session = new JsAmplifierSession(validConfig);
    const coord = session.coordinator;
    expect(coord).toBeDefined();
  });

  it('rejects empty config', () => {
    expect(() => new JsAmplifierSession('{}')).toThrow();
  });

  it('rejects config without orchestrator', () => {
    const badConfig = JSON.stringify({
      session: { context: 'context-simple' },
    });
    expect(() => new JsAmplifierSession(badConfig)).toThrow(/orchestrator/);
  });

  it('rejects config without context', () => {
    const badConfig = JSON.stringify({
      session: { orchestrator: 'loop-basic' },
    });
    expect(() => new JsAmplifierSession(badConfig)).toThrow(/context/);
  });

  it('cleanup clears initialized flag', async () => {
    const session = new JsAmplifierSession(validConfig);
    await session.cleanup();
    expect(session.isInitialized).toBe(false);
  });
});
```

### Step 2: Run test to verify it fails

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run __tests__/session.test.ts 2>&1
```
Expected: FAIL — `JsAmplifierSession` doesn't exist yet.

### Step 3: Implement JsAmplifierSession

Open `bindings/node/src/lib.rs`. Add:

```rust
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// JsAmplifierSession — wraps amplifier_core::Session
// ---------------------------------------------------------------------------

/// Primary entry point for TypeScript consumers.
///
/// Lifecycle: new(config) → initialize() → execute(prompt) → cleanup().
#[napi]
pub struct JsAmplifierSession {
    inner: Arc<tokio::sync::Mutex<amplifier_core::Session>>,
    /// Cached session_id (avoids locking inner for every access).
    cached_session_id: String,
    /// Cached parent_id.
    cached_parent_id: Option<String>,
    /// Config JSON for coordinator construction.
    config_json: String,
}

#[napi]
impl JsAmplifierSession {
    /// Create a new session.
    ///
    /// `config_json` must be a JSON string with at minimum:
    /// `{ "session": { "orchestrator": "...", "context": "..." } }`
    #[napi(constructor)]
    pub fn new(
        config_json: String,
        session_id: Option<String>,
        parent_id: Option<String>,
    ) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(&config_json)
            .map_err(|e| Error::from_reason(format!("Invalid config JSON: {e}")))?;

        let session_config = amplifier_core::SessionConfig::from_value(value)
            .map_err(|e| Error::from_reason(format!("{e}")))?;

        let session = amplifier_core::Session::new(
            session_config,
            session_id.clone(),
            parent_id.clone(),
        );

        let actual_id = session.session_id().to_string();
        let actual_parent = session.parent_id().map(|s| s.to_string());

        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(session)),
            cached_session_id: actual_id,
            cached_parent_id: actual_parent,
            config_json,
        })
    }

    /// The session ID (UUID string).
    #[napi(getter)]
    pub fn session_id(&self) -> &str {
        &self.cached_session_id
    }

    /// The parent session ID, if any.
    #[napi(getter)]
    pub fn parent_id(&self) -> Option<String> {
        self.cached_parent_id.clone()
    }

    /// Whether the session has been initialized.
    #[napi(getter)]
    pub fn is_initialized(&self) -> bool {
        // Use try_lock to avoid blocking the JS thread
        match self.inner.try_lock() {
            Ok(session) => session.is_initialized(),
            Err(_) => false,
        }
    }

    /// Current session status string (running, completed, failed, cancelled).
    #[napi(getter)]
    pub fn status(&self) -> String {
        match self.inner.try_lock() {
            Ok(session) => session.status().to_string(),
            Err(_) => "running".to_string(),
        }
    }

    /// Access the coordinator.
    #[napi(getter)]
    pub fn coordinator(&self) -> Result<JsCoordinator> {
        // Create a coordinator wrapper from the config.
        // Note: This creates a separate coordinator instance. For shared state,
        // the Session's internal coordinator needs Arc wrapping.
        // This is a known limitation — see Future TODO #1 in design doc.
        let config: HashMap<String, serde_json::Value> =
            serde_json::from_str(&self.config_json).unwrap_or_default();
        Ok(JsCoordinator {
            inner: Arc::new(amplifier_core::Coordinator::new(config)),
        })
    }

    /// Mark the session as initialized.
    ///
    /// In the Napi-RS binding, module loading happens in JS-land.
    /// Call this after mounting modules via the coordinator.
    #[napi]
    pub fn set_initialized(&self) {
        if let Ok(session) = self.inner.try_lock() {
            session.set_initialized();
        }
    }

    /// Clean up session resources.
    #[napi]
    pub async fn cleanup(&self) -> Result<()> {
        let session = self.inner.lock().await;
        session.cleanup().await;
        Ok(())
    }
}
```

> **Known limitation:** The `coordinator()` getter creates a separate Coordinator instance. Sharing the Session's internal Coordinator requires restructuring the Rust kernel to use `Arc<Coordinator>` — this is tracked as Future TODO #1 in the design doc. For the initial binding, JS-side module mounting and Rust kernel config/hooks/cancellation work independently, which matches the Python hybrid pattern.

### Step 4: Rebuild and run tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run __tests__/session.test.ts 2>&1
```
Expected: All 11 tests pass.

### Step 5: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "feat(node): add AmplifierSession binding"
```

---

## Task 7: Module Interfaces

**Why:** Module interfaces let TypeScript authors implement `Tool`, `Provider`, `Orchestrator`, etc. as plain TS objects and mount them in the coordinator. The bridge structs (`JsToolBridge`, `JsProviderBridge`, etc.) use `ThreadsafeFunction` to call from Rust back into JS.

**Reference:** The 6 Rust traits are in `crates/amplifier-core/src/traits.rs`. The Python bridge defines `PyHookHandlerBridge` (which we already did in Task 4). This task adds `JsToolBridge` as the primary example — the others follow the same pattern.

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Create: `bindings/node/__tests__/modules.test.ts`

### Step 1: Write the failing test

Create the file `bindings/node/__tests__/modules.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { JsToolBridge } from '../index.js';

describe('Tool interface bridge', () => {
  it('creates a JsToolBridge wrapping a TS tool object', () => {
    const tool = new JsToolBridge(
      'echo',
      'Echoes input back',
      JSON.stringify({ type: 'object', properties: { text: { type: 'string' } } }),
      async (inputJson: string) => {
        const input = JSON.parse(inputJson);
        return JSON.stringify({
          success: true,
          output: input.text || 'no input',
        });
      },
    );

    expect(tool.name).toBe('echo');
    expect(tool.description).toBe('Echoes input back');
  });

  it('executes a tool through the bridge', async () => {
    const tool = new JsToolBridge(
      'greet',
      'Greets a person',
      '{}',
      async (inputJson: string) => {
        const input = JSON.parse(inputJson);
        return JSON.stringify({
          success: true,
          output: `Hello, ${input.name}!`,
        });
      },
    );

    const resultJson = await tool.execute(JSON.stringify({ name: 'World' }));
    const result = JSON.parse(resultJson);
    expect(result.success).toBe(true);
    expect(result.output).toBe('Hello, World!');
  });

  it('handles tool execution errors', async () => {
    const tool = new JsToolBridge(
      'failing',
      'Always fails',
      '{}',
      async (_inputJson: string) => {
        return JSON.stringify({
          success: false,
          error: 'Something went wrong',
        });
      },
    );

    const resultJson = await tool.execute('{}');
    const result = JSON.parse(resultJson);
    expect(result.success).toBe(false);
    expect(result.error).toBe('Something went wrong');
  });
});
```

### Step 2: Run test to verify it fails

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run __tests__/modules.test.ts 2>&1
```
Expected: FAIL — `JsToolBridge` doesn't exist yet.

### Step 3: Implement JsToolBridge

Open `bindings/node/src/lib.rs`. Add:

```rust
// ---------------------------------------------------------------------------
// JsToolBridge — wraps a JS tool implementation as a Napi class
// ---------------------------------------------------------------------------

/// Bridge that wraps a TypeScript tool implementation.
///
/// The TS side provides name, description, parameters schema, and an
/// async execute function. This class holds a ThreadsafeFunction to the
/// execute callback so Rust can call back into JS.
///
/// In the hybrid coordinator pattern, these bridge objects are stored in
/// a JS-side Map (not in the Rust Coordinator). The JS orchestrator
/// retrieves them by name and calls execute() directly.
#[napi]
pub struct JsToolBridge {
    tool_name: String,
    tool_description: String,
    parameters_json: String,
    execute_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
}

#[napi]
impl JsToolBridge {
    /// Create a new tool bridge.
    ///
    /// - `name`: Tool name (e.g., "bash", "read_file")
    /// - `description`: Human-readable description
    /// - `parameters_json`: JSON Schema for tool parameters
    /// - `execute_fn`: Async function `(inputJson: string) => Promise<string>`
    ///   that takes JSON input and returns JSON ToolResult
    #[napi(constructor)]
    #[napi(ts_args_type = "name: string, description: string, parametersJson: string, executeFn: (inputJson: string) => Promise<string>")]
    pub fn new(
        name: String,
        description: String,
        parameters_json: String,
        execute_fn: JsFunction,
    ) -> Result<Self> {
        let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = execute_fn
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
                let env = ctx.env;
                Ok(vec![env.create_string(&ctx.value)?.into_unknown()])
            })?;

        Ok(Self {
            tool_name: name,
            tool_description: description,
            parameters_json,
            execute_fn: tsfn,
        })
    }

    /// The tool name.
    #[napi(getter)]
    pub fn name(&self) -> &str {
        &self.tool_name
    }

    /// The tool description.
    #[napi(getter)]
    pub fn description(&self) -> &str {
        &self.tool_description
    }

    /// Execute the tool with JSON input. Returns a JSON ToolResult string.
    #[napi]
    pub async fn execute(&self, input_json: String) -> Result<String> {
        let result = self
            .execute_fn
            .call_async(input_json)
            .await
            .map_err(|e| Error::from_reason(format!("Tool execution error: {e}")))?;
        Ok(result)
    }

    /// Get the tool spec as a JSON string.
    #[napi]
    pub fn get_spec(&self) -> String {
        serde_json::json!({
            "name": self.tool_name,
            "description": self.tool_description,
            "parameters": serde_json::from_str::<serde_json::Value>(&self.parameters_json)
                .unwrap_or(serde_json::json!({})),
        })
        .to_string()
    }
}
```

### Step 4: Rebuild and run tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run __tests__/modules.test.ts 2>&1
```
Expected: All 3 tests pass.

### Step 5: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "feat(node): add JsToolBridge module interface"
```

> **Follow-up in this same task or as a sub-step:** After the Tool bridge is proven, add `JsProviderBridge` following the exact same pattern (name, get_info, complete, parse_tool_calls). The other interfaces (Orchestrator, ContextManager, ApprovalProvider) follow the same ThreadsafeFunction pattern. Each gets its own constructor, properties, and async methods. The pattern is identical — only the method names and signatures differ.

---

## Task 8: Error Bridging

**Why:** Rust errors need to become proper JS Error objects with typed `code` properties. JS exceptions in callbacks need to become Rust `Result::Err`. This task establishes the error taxonomy across the FFI boundary.

**Reference:** The Rust errors are in `crates/amplifier-core/src/errors.rs`. The Python bridge converts them via `PyErr::new::<PyRuntimeError, _>(...)`.

**Files:**
- Modify: `bindings/node/src/lib.rs`
- Create: `bindings/node/__tests__/errors.test.ts`

### Step 1: Write the failing test

Create the file `bindings/node/__tests__/errors.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import {
  JsAmplifierSession,
  amplifierErrorToJs,
} from '../index.js';

describe('Error bridging', () => {
  it('invalid JSON config throws with clear message', () => {
    expect(() => new JsAmplifierSession('not json')).toThrow(/Invalid config JSON/);
  });

  it('missing orchestrator throws with field name', () => {
    const config = JSON.stringify({ session: { context: 'simple' } });
    expect(() => new JsAmplifierSession(config)).toThrow(/orchestrator/);
  });

  it('missing context throws with field name', () => {
    const config = JSON.stringify({ session: { orchestrator: 'basic' } });
    expect(() => new JsAmplifierSession(config)).toThrow(/context/);
  });

  it('amplifierErrorToJs converts error variants to typed objects', () => {
    // Test the helper function that converts Rust AmplifierError to JS
    const sessionError = amplifierErrorToJs('session', 'not initialized');
    expect(sessionError.code).toBe('SessionError');
    expect(sessionError.message).toBe('not initialized');

    const toolError = amplifierErrorToJs('tool', 'tool not found: bash');
    expect(toolError.code).toBe('ToolError');

    const providerError = amplifierErrorToJs('provider', 'rate limited');
    expect(providerError.code).toBe('ProviderError');

    const hookError = amplifierErrorToJs('hook', 'handler failed');
    expect(hookError.code).toBe('HookError');

    const contextError = amplifierErrorToJs('context', 'compaction failed');
    expect(contextError.code).toBe('ContextError');
  });
});
```

### Step 2: Run test to verify it fails

Run:
```bash
cd amplifier-core/bindings/node && npx vitest run __tests__/errors.test.ts 2>&1
```
Expected: FAIL — `amplifierErrorToJs` doesn't exist yet.

### Step 3: Implement error bridging

Open `bindings/node/src/lib.rs`. Add:

```rust
// ---------------------------------------------------------------------------
// Error bridging — Rust AmplifierError → JS Error with typed code
// ---------------------------------------------------------------------------

/// Error info object returned to JS with a typed error code.
#[napi(object)]
pub struct JsAmplifierError {
    /// Error category: "SessionError", "ToolError", "ProviderError", "HookError", "ContextError"
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

/// Convert an AmplifierError variant name + message to a typed JS error object.
///
/// This is a helper exposed to JS for consistent error handling.
/// In practice, most errors are thrown directly as napi::Error — this helper
/// is for cases where you want structured error objects.
#[napi]
pub fn amplifier_error_to_js(variant: String, message: String) -> JsAmplifierError {
    let code = match variant.as_str() {
        "session" => "SessionError",
        "tool" => "ToolError",
        "provider" => "ProviderError",
        "hook" => "HookError",
        "context" => "ContextError",
        _ => "AmplifierError",
    };
    JsAmplifierError {
        code: code.to_string(),
        message,
    }
}

/// Internal helper: convert amplifier_core::AmplifierError to napi::Error.
fn amplifier_error_to_napi(err: amplifier_core::AmplifierError) -> napi::Error {
    let (code, msg) = match &err {
        amplifier_core::AmplifierError::Session(e) => ("SessionError", e.to_string()),
        amplifier_core::AmplifierError::Tool(e) => ("ToolError", e.to_string()),
        amplifier_core::AmplifierError::Provider(e) => ("ProviderError", e.to_string()),
        amplifier_core::AmplifierError::Hook(e) => ("HookError", e.to_string()),
        amplifier_core::AmplifierError::Context(e) => ("ContextError", e.to_string()),
    };
    Error::from_reason(format!("[{code}] {msg}"))
}
```

### Step 4: Rebuild and run tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run __tests__/errors.test.ts 2>&1
```
Expected: All 5 tests pass.

### Step 5: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "feat(node): add error bridging — Rust errors to typed JS errors"
```

---

## Task 9: Integration Tests

**Why:** Verify the full binding layer works end-to-end: session lifecycle with TS-implemented modules, concurrent operations, cancellation, type fidelity across the FFI boundary.

**Files:**
- Create: `bindings/node/__tests__/integration.test.ts`

### Step 1: Write integration tests

Create the file `bindings/node/__tests__/integration.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import {
  JsAmplifierSession,
  JsCoordinator,
  JsCancellationToken,
  JsHookRegistry,
  JsToolBridge,
  HookAction,
  SessionState,
} from '../index.js';

describe('Integration: Full session lifecycle', () => {
  const validConfig = JSON.stringify({
    session: {
      orchestrator: 'loop-basic',
      context: 'context-simple',
    },
  });

  it('session → coordinator → hooks → cancel lifecycle', async () => {
    // 1. Create session
    const session = new JsAmplifierSession(validConfig);
    expect(session.sessionId).toBeTruthy();
    expect(session.isInitialized).toBe(false);

    // 2. Access coordinator
    const coord = session.coordinator;
    expect(coord).toBeDefined();

    // 3. Register capability
    coord.registerCapability('test-cap', JSON.stringify({ enabled: true }));
    const cap = coord.getCapability('test-cap');
    expect(cap).toBeTruthy();
    expect(JSON.parse(cap!).enabled).toBe(true);

    // 4. Use cancellation
    const cancel = coord.cancellation;
    expect(cancel.isCancelled).toBe(false);
    cancel.requestGraceful();
    expect(cancel.isGraceful).toBe(true);
    cancel.reset();
    expect(cancel.isCancelled).toBe(false);

    // 5. Cleanup
    await session.cleanup();
    expect(session.isInitialized).toBe(false);
  });
});

describe('Integration: Hook handler roundtrip', () => {
  it('JS handler receives event data and returns HookResult', async () => {
    const registry = new JsHookRegistry();
    const receivedEvents: Array<{ event: string; data: any }> = [];

    registry.register(
      'tool:pre',
      (event: string, dataJson: string) => {
        const data = JSON.parse(dataJson);
        receivedEvents.push({ event, data });
        return JSON.stringify({
          action: 'continue',
          user_message: 'Tool approved',
          user_message_level: 'info',
        });
      },
      0,
      'approval-hook',
    );

    const result = await registry.emit(
      'tool:pre',
      JSON.stringify({ tool_name: 'bash', command: 'ls' }),
    );

    expect(receivedEvents.length).toBe(1);
    expect(receivedEvents[0].event).toBe('tool:pre');
    expect(receivedEvents[0].data.tool_name).toBe('bash');
    expect(result.action).toBe(HookAction.Continue);
  });

  it('deny handler short-circuits pipeline', async () => {
    const registry = new JsHookRegistry();
    let secondHandlerCalled = false;

    registry.register(
      'tool:pre',
      (_e: string, _d: string) => {
        return JSON.stringify({ action: 'deny', reason: 'not allowed' });
      },
      0,
      'denier',
    );

    registry.register(
      'tool:pre',
      (_e: string, _d: string) => {
        secondHandlerCalled = true;
        return JSON.stringify({ action: 'continue' });
      },
      10,
      'after-deny',
    );

    const result = await registry.emit('tool:pre', '{}');
    expect(result.action).toBe(HookAction.Deny);
    expect(result.reason).toBe('not allowed');
    expect(secondHandlerCalled).toBe(false);
  });
});

describe('Integration: Tool bridge execution', () => {
  it('creates and executes a TS tool through the bridge', async () => {
    const tool = new JsToolBridge(
      'calculator',
      'Adds two numbers',
      JSON.stringify({
        type: 'object',
        properties: {
          a: { type: 'number' },
          b: { type: 'number' },
        },
      }),
      async (inputJson: string) => {
        const input = JSON.parse(inputJson);
        const sum = (input.a || 0) + (input.b || 0);
        return JSON.stringify({ success: true, output: String(sum) });
      },
    );

    expect(tool.name).toBe('calculator');
    const specJson = tool.getSpec();
    const spec = JSON.parse(specJson);
    expect(spec.name).toBe('calculator');
    expect(spec.parameters.type).toBe('object');

    const resultJson = await tool.execute(JSON.stringify({ a: 3, b: 4 }));
    const result = JSON.parse(resultJson);
    expect(result.success).toBe(true);
    expect(result.output).toBe('7');
  });
});

describe('Integration: CancellationToken state machine', () => {
  it('full state machine: none → graceful → immediate → reset → none', () => {
    const token = new JsCancellationToken();

    // None state
    expect(token.isCancelled).toBe(false);
    expect(token.isGraceful).toBe(false);
    expect(token.isImmediate).toBe(false);

    // → Graceful
    token.requestGraceful();
    expect(token.isCancelled).toBe(true);
    expect(token.isGraceful).toBe(true);
    expect(token.isImmediate).toBe(false);

    // → Immediate
    token.requestImmediate();
    expect(token.isCancelled).toBe(true);
    expect(token.isGraceful).toBe(false);
    expect(token.isImmediate).toBe(true);

    // → Reset → None
    token.reset();
    expect(token.isCancelled).toBe(false);
    expect(token.isGraceful).toBe(false);
    expect(token.isImmediate).toBe(false);
  });
});

describe('Integration: Type fidelity', () => {
  it('SessionConfig validates required fields', () => {
    // Valid config
    expect(
      () =>
        new JsAmplifierSession(
          JSON.stringify({
            session: { orchestrator: 'x', context: 'y' },
            providers: { anthropic: { model: 'claude' } },
            metadata: { custom: true },
          }),
        ),
    ).not.toThrow();
  });

  it('HookResult fields roundtrip correctly', async () => {
    const registry = new JsHookRegistry();
    registry.register(
      'test:roundtrip',
      (_e: string, _d: string) => {
        return JSON.stringify({
          action: 'inject_context',
          context_injection: 'Linter found 3 errors',
          context_injection_role: 'system',
          ephemeral: true,
          suppress_output: true,
          user_message: 'Found lint errors',
          user_message_level: 'warning',
          user_message_source: 'eslint-hook',
        });
      },
      0,
      'lint-hook',
    );

    const result = await registry.emit('test:roundtrip', '{}');
    expect(result.action).toBe(HookAction.InjectContext);
    expect(result.context_injection).toBe('Linter found 3 errors');
    expect(result.ephemeral).toBe(true);
    expect(result.suppress_output).toBe(true);
    expect(result.user_message).toBe('Found lint errors');
    expect(result.user_message_source).toBe('eslint-hook');
  });

  it('Coordinator toDict returns all expected fields', () => {
    const coord = new JsCoordinator('{}');
    coord.registerCapability('streaming', '"true"');
    const dict = coord.toDict();

    expect(Array.isArray(dict.tools)).toBe(true);
    expect(Array.isArray(dict.providers)).toBe(true);
    expect(typeof dict.has_orchestrator).toBe('boolean');
    expect(typeof dict.has_context).toBe('boolean');
    expect(Array.isArray(dict.capabilities)).toBe(true);
  });
});
```

### Step 2: Build and run all tests

Run:
```bash
cd amplifier-core/bindings/node && npm run build:debug && npx vitest run 2>&1
```
Expected: All tests across all test files pass (~65 total).

### Step 3: Run Rust tests to verify nothing broke

Run:
```bash
cd amplifier-core && cargo test --all 2>&1
```
Expected: All Rust tests still pass.

### Step 4: Commit

```bash
cd amplifier-core && git add bindings/node/ && git commit -m "test(node): add integration tests for full binding layer"
```

---

## Final Checklist

After all 10 tasks are complete, verify:

1. **Rust builds clean:**
   ```bash
   cd amplifier-core && cargo build --all 2>&1
   ```

2. **All Rust tests pass:**
   ```bash
   cd amplifier-core && cargo test --all 2>&1
   ```

3. **Node addon builds:**
   ```bash
   cd amplifier-core/bindings/node && npm run build:debug 2>&1
   ```

4. **All Vitest tests pass:**
   ```bash
   cd amplifier-core/bindings/node && npx vitest run 2>&1
   ```

5. **Generated types exist:**
   ```bash
   ls -la amplifier-core/bindings/node/index.js amplifier-core/bindings/node/index.d.ts
   ```

6. **Type definitions are meaningful:**
   ```bash
   cat amplifier-core/bindings/node/index.d.ts | head -100
   ```
   Expected: TypeScript declarations with proper types (not `any` everywhere).

---

## Deferred Work (NOT in this plan)

These items are explicitly out of scope — tracked in the design doc's "Tracked Future Debt" table:

1. **gRPC bridge fidelity fixes** — 27 `TODO(grpc-v2)` markers in the codebase
2. **`process_hook_result()` in Rust** — currently ~185 lines of Python-only code
3. **Cross-language module resolver** — Phase 4
4. **npm publishing pipeline / CI** — separate follow-up
5. **Splitting `lib.rs` into modules** — when >3,000 lines (Future TODO #3)
6. **Unified Rust module storage** — consolidating per-language module dicts (Future TODO #1)
7. **`JsProviderBridge`**, **`JsOrchestratorBridge`**, **`JsContextManagerBridge`**, **`JsApprovalProviderBridge`** — follow the exact same `ThreadsafeFunction` pattern as `JsToolBridge`. Add them after the Tool bridge is proven. Each is ~50 lines of boilerplate.
