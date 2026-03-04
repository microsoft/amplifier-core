# Cross-Language SDK Phase 2: TypeScript/Napi-RS Bindings Design

## Goal

Deliver TypeScript/Node.js bindings for the amplifier-core Rust kernel via Napi-RS, enabling three consumer types: TypeScript host apps (full agent loop), TypeScript in-process modules (Tool/Provider/etc. implementations), and TypeScript gRPC module authoring helpers — while batching two dependency security upgrades (pyo3, wasmtime).

## Background

This is Phase 2 of the 5-phase Cross-Language SDK plan documented in [`2026-03-02-cross-language-session-sdk-design.md`](./2026-03-02-cross-language-session-sdk-design.md). Phase 1 (complete) delivered the Python/PyO3 bridge — 4 classes wrapping the Rust kernel in ~2,885 lines of `bindings/python/src/lib.rs`. Phase 2 mirrors this for TypeScript.

The existing Python bridge uses a "hybrid coordinator" pattern: Python Protocol objects are stored in a Python-side `mount_points` dict, while the Rust kernel handles config, turn tracking, and cancellation. The TypeScript binding follows the same pattern with a JS-side `Map` for module storage.

### Three Consumer Types

1. **TypeScript host apps** — full agent loop in Node.js (`new AmplifierSession(config) → execute() → cleanup()`)
2. **TypeScript in-process modules** — implement `Tool`/`Provider`/etc. interfaces, mount directly in a TS host
3. **TypeScript gRPC modules** — implement proto services, plug into any host (Python, Rust, future Go) via transport-invisible bridge

## Approach

Single-crate Napi-RS bridge mirroring the proven PyO3 structure. The Python bridge is a working, battle-tested pattern. The TypeScript binding mirrors it structurally: same 4 classes, same hybrid coordinator, same async bridging strategy adapted for Node's event loop.

A single `lib.rs` file (matching Python's approach) keeps things simple and greppable. Splitting into modules is tracked as future work when the file outgrows maintainability.

## Architecture

```
bindings/node/
├── Cargo.toml          # napi-rs crate
├── src/lib.rs          # All Napi-RS bindings (mirrors bindings/python/src/lib.rs)
├── package.json        # npm package config
├── index.js            # Generated Napi-RS entry
├── index.d.ts          # Generated TypeScript definitions
└── __tests__/          # Vitest test suite
```

The crate lives at `bindings/node/` in the workspace, parallel to `bindings/python/`. Both depend on `amplifier-core` as a path dependency and wrap the same Rust kernel types.

## Components

### Build Infrastructure & Dependencies

**Workspace setup:**
- New crate `bindings/node/` added to workspace `Cargo.toml` members list
- Napi-RS framework: `napi` + `napi-derive` crates, `napi-build` as build dependency
- npm package (name TBD — likely `@amplifier/core` or `amplifier-core`)

**Dependency upgrades (batched with this phase):**
- `pyo3` → `0.28.2` in `bindings/python/Cargo.toml` (and `pyo3-async-runtimes` to match) — HIGH severity type confusion fix (Dependabot alert #1)
- `wasmtime` → latest (currently 42) in `crates/amplifier-core/Cargo.toml` — covers all 8 Dependabot alerts (6 medium, 2 low). WASM bridge API breakage must be fixed since wasmtime jumps from v29 to v42.

**Generated outputs:**
- Napi-RS auto-generates `index.js` (native binding loader) and `index.d.ts` (TypeScript definitions) from `#[napi]` annotations
- Platform-specific `.node` binary

**Crate dependencies:**
- `amplifier-core` (path dependency, same as Python binding)
- `napi` + `napi-derive` (Napi-RS framework)
- `tokio` (async runtime)
- `serde_json` (JSON bridging)
- `uuid` (session IDs)

### TypeScript API Surface

**Four classes exposed via `#[napi]`:**

#### AmplifierSession — Primary Entry Point

```typescript
interface SessionConfig {
  providers?: Record<string, ProviderConfig>;
  tools?: Record<string, ToolConfig>;
  orchestrator?: OrchestratorConfig;
  context?: ContextConfig;
  hooks?: HookConfig[];
  system_prompt?: string;
  metadata?: Record<string, unknown>;
}

class AmplifierSession {
  constructor(config: SessionConfig);
  get sessionId(): string;
  get parentId(): string | null;
  get status(): SessionStatus;
  get isInitialized(): boolean;
  get coordinator(): Coordinator;

  async initialize(): Promise<void>;
  async execute(prompt: string): Promise<ExecuteResult>;
  async cleanup(): Promise<void>;

  // Symbol.asyncDispose support
  async [Symbol.asyncDispose](): Promise<void>;
}
```

#### Coordinator — Module Mounting and Lifecycle

```typescript
class Coordinator {
  mountTool(name: string, tool: Tool): void;
  mountProvider(name: string, provider: Provider): void;
  setOrchestrator(orchestrator: Orchestrator): void;
  setContext(context: ContextManager): void;

  getTool(name: string): Tool | null;
  getProvider(name: string): Provider | null;
  get tools(): string[];
  get providers(): string[];

  get hooks(): HookRegistry;
  get cancellation(): CancellationToken;
  get config(): SessionConfig;

  registerCapability<T>(name: string, value: T): void;
  getCapability<T>(name: string): T | null;

  async cleanup(): Promise<void>;
  resetTurn(): void;
  toDict(): CoordinatorState;
}
```

#### HookRegistry — Event System

```typescript
class HookRegistry {
  register(event: string, handler: HookHandler): string;
  unregister(handlerId: string): void;
  async emit(event: string, data: HookEventData): Promise<HookResult>;
  async emitAndCollect(event: string, data: HookEventData): Promise<HookResult[]>;
  listHandlers(event?: string): string[];
  setDefaultFields(fields: Record<string, unknown>): void;
}
```

#### CancellationToken — Cooperative Cancellation

```typescript
class CancellationToken {
  get isCancelled(): boolean;
  get isGraceful(): boolean;
  get isImmediate(): boolean;
  requestGraceful(reason?: string): void;
  requestImmediate(reason?: string): void;
  reset(): void;
  onCancel(callback: () => void): void;
}
```

**Six module interfaces (for module authors):**

```typescript
interface Tool {
  name: string;
  description: string;
  getSpec(): ToolSpec;
  execute(params: Record<string, any>): Promise<ToolResult>;
}

interface Provider { /* matching Rust Provider trait */ }
interface Orchestrator { /* matching Rust Orchestrator trait */ }
interface ContextManager { /* matching Rust ContextManager trait */ }
interface HookHandler { /* matching Rust HookHandler trait */ }
interface ApprovalProvider { /* matching Rust ApprovalProvider trait */ }
```

**Data model types — all typed, generated from Rust structs via `#[napi(object)]`:**

```typescript
interface ToolSpec {
  name: string;
  description: string;
  parameters: Record<string, unknown>;  // JSON Schema — intentionally loose
}

interface ToolResult {
  success: boolean;
  output: string;
  error?: string;
  metadata?: Record<string, unknown>;
}

interface HookResult {
  action: HookAction;
  reason?: string;
  contextInjection?: string;
  contextInjectionRole?: ContextInjectionRole;
  ephemeral?: boolean;
  suppressOutput?: boolean;
  userMessage?: string;
  userMessageLevel?: UserMessageLevel;
  userMessageSource?: string;
  approvalPrompt?: string;
  approvalOptions?: string[];
  approvalTimeout?: number;
  approvalDefault?: ApprovalDefault;
}

// Enums as string unions (TypeScript idiom)
type HookAction = 'continue' | 'inject_context' | 'ask_user' | 'deny';
type Role = 'system' | 'user' | 'assistant' | 'tool';
type SessionState = 'created' | 'initialized' | 'running' | 'completed' | 'failed';
```

**Naming convention:** camelCase methods per TypeScript idiom. Napi-RS `#[napi]` handles Rust snake_case → JS camelCase automatically.

**Typing rule:** Typed interfaces everywhere except where the schema is genuinely dynamic (JSON Schema for tool parameters, arbitrary metadata bags). Those use `Record<string, unknown>` — still better than `any` because it signals "this is a dictionary, not a class."

### Async Bridging & Runtime

**Core challenge:** Rust tokio ↔ Node.js libuv event loop.

**Approach (mirrors Python bridge strategy):**
- Napi-RS `AsyncTask` and `Task` traits bridge async Rust → JS Promises
- Each async Rust method becomes a `#[napi]` async method that spawns a tokio future and returns `Promise<T>` to JS
- Tokio runtime initialized lazily on first use, shared across all calls (same pattern as `pyo3-async-runtimes`)
- JS callback bridging uses Napi-RS `ThreadsafeFunction` — equivalent of PyO3's `Py<PyAny>` callback pattern

**Hook handler bridging:**
- JS functions registered as hook handlers get wrapped in `JsHookHandlerBridge` (Rust struct, mirrors `PyHookHandlerBridge`)
- Bridge holds a `ThreadsafeFunction` reference to the JS callback
- When Rust `HookRegistry` fires, it calls through the bridge back into JS
- Both sync and async JS handlers supported (detect via Promise return type)

**Error bridging:**
- Rust `AmplifierError` variants → JS `Error` subclasses with typed `code` properties
- JS exceptions in module callbacks → caught at Napi-RS boundary, converted to `Result::Err`
- Same error taxonomy as Python: `ProviderError`, `ToolError`, `SessionError`, etc.

## Data Flow

The data flow mirrors the Python bridge exactly:

1. **Session creation:** TS `new AmplifierSession(config)` → Napi-RS boundary → Rust `Session::new()`
2. **Module mounting:** TS `coordinator.mountTool(name, tool)` → JS-side `Map` stores the TS object (not sent to Rust)
3. **Execution:** TS `session.execute(prompt)` → Rust kernel orchestrates → calls back into JS via `ThreadsafeFunction` when it needs Tool/Provider execution → JS module runs → result crosses back through Napi-RS → Rust continues
4. **Hook emission:** Rust kernel fires hook → `JsHookHandlerBridge` calls JS handler via `ThreadsafeFunction` → JS handler returns `HookResult` → Rust processes result
5. **Cancellation:** TS `cancellation.requestGraceful()` → Rust `AtomicBool` set → checked cooperatively during execution loops

## Error Handling

- **Rust errors** cross the FFI boundary as typed JS `Error` subclasses with a `code` property matching the Rust variant name (`ProviderError`, `ToolError`, `SessionError`, etc.)
- **JS exceptions** thrown inside module callbacks (Tool.execute, Provider.generate, etc.) are caught at the Napi-RS boundary and converted to Rust `Result::Err` — they do not crash the process
- **Async errors** in Promises are propagated correctly — a rejected Promise in a JS hook handler becomes an `Err` in the Rust `HookRegistry` emission
- **Type mismatches** at the boundary (wrong config shape, missing required fields) are caught by Napi-RS's automatic deserialization and reported as clear `TypeError`s with field paths

## Testing Strategy

**Test parity target:** Prove the Napi-RS bindings work correctly, not retest the Rust kernel (which has its own 312 tests).

| Layer | What | Framework | Count (est.) |
|---|---|---|---|
| Binding smoke tests | Each class instantiates, properties return correct types, async methods return Promises | Vitest | ~20 |
| Session lifecycle tests | new → initialize → execute → cleanup with mock modules | Vitest | ~10 |
| Module interface tests | TS objects implementing Tool/Provider/etc. mount correctly, get called, return typed results | Vitest | ~15 |
| Async bridging tests | Concurrent operations, cancellation mid-execution, error propagation across FFI | Vitest | ~10 |
| Type fidelity tests | Config types, HookResult fields, error codes serialize/deserialize correctly across boundary | Vitest | ~10 |

**~65 tests total** focused on the bridge layer.

**Framework:** Vitest (modern, fast, native TS support, good async testing).

**NOT tested at the TS layer:** Kernel correctness (Rust tests), orchestrator loop behavior (Python orchestrator module tests), gRPC transport (deferred).

## Deliverables

1. `bindings/node/` — Napi-RS crate with 4 typed classes, 6 module interfaces, full data model types
2. `.d.ts` type definitions — auto-generated from `#[napi]` annotations
3. `package.json` — publishable npm package
4. ~65 Vitest tests covering the binding layer
5. Dependency upgrades — pyo3 → 0.28.2, wasmtime → latest (with WASM bridge API fixes)

## Explicitly Not In Scope

- gRPC bridge fidelity fixes (27 `TODO(grpc-v2)` markers — separate effort)
- `process_hook_result()` ported to Rust (deferred, tracked below)
- Cross-language module resolver (Phase 4)
- npm publishing pipeline / CI/CD for npm (follow-up)

## Tracked Future Debt

| # | Item | Description | Trigger |
|---|------|-------------|---------|
| Future TODO #1 | Unified Rust Module Storage | Consolidate per-language module dicts (Python `mount_points`, TS `Map`) into Rust `Arc<dyn Trait>` slots on the Coordinator. Reduces N×M maintenance cost as languages × trait changes grow. Currently each language independently stores module objects in its own runtime. | Third language binding (Go/C#) added, or trait surface starts evolving again |
| Future TODO #2 | Rust-native `process_hook_result()` | Port hook result routing logic (context injection, approval gates, user messages, output suppression) from Python `_rust_wrappers.py:ModuleCoordinator` into the Rust kernel. Currently ~185 lines of Python that every orchestrator calls after every `hooks.emit()`. Requires `DisplaySystem` trait in Rust, wiring approval/context through Rust typed slots. | First TypeScript orchestrator written, or after TODO #1 lands (which solves the subsystem access problem) |
| Future TODO #3 | Split `bindings/node/src/lib.rs` | Split single-file Napi-RS binding into `src/session.rs`, `src/coordinator.rs`, `src/hooks.rs`, `src/cancellation.rs`, `src/types.rs` for navigability. Single-file pattern is proven from Python bridge but may outgrow maintainability. | File exceeds ~3,000 lines |

## Key Design Decisions

1. **Napi-RS in-process bindings** (not gRPC) — zero-overhead FFI, same pattern as PyO3
2. **Hybrid coordinator pattern** — JS-side `Map` for module storage, Rust kernel for config/tracking/cancellation (mirrors Python, pragmatic for ship speed)
3. **Deferred gRPC bridge fidelity fixes** — TS in-process modules don't hit the wire, so no data loss; gRPC fixes are a separate effort
4. **Deferred `process_hook_result()` to Rust** — callable from Python only today; TS orchestrators are future use case; tracked as debt
5. **Single `lib.rs`** — YAGNI, split later when needed (Future TODO #3)
6. **Fully typed API surface** — typed interfaces for configs, results, events (not `object`/`any`) to maximize AI-assist and IDE value
7. **Dependency upgrades batched** — pyo3 + wasmtime security fixes in the first task since we're touching Cargo.toml anyway

## Relationship to Other Phases

- **Phase 1 (complete):** Python/PyO3 bridge — the pattern we're mirroring
- **Phase 2 (this design):** TypeScript/Napi-RS bridge
- **Phase 3 (future):** Full WASM module loading via wasmtime component model
- **Phase 4 (future):** Cross-language module resolver — auto-detect language, pick transport
- **Phase 5 (future):** Go (CGo) and C# (P/Invoke) SDKs