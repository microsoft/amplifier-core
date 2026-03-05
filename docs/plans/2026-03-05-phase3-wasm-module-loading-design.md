# Phase 3: WASM Module Loading Design

> Full WebAssembly Component Model integration for amplifier-core — all 6 module types loadable as `.wasm` components via wasmtime.

**Status:** Approved
**Date:** 2026-03-05
**Phase:** 3 of 5 (Cross-Language SDK)
**Parent design:** `docs/plans/2026-03-02-cross-language-session-sdk-design.md`
**Prerequisites:** PR #35 (Phase 2 — wasmtime 42 upgrade), PR #36 (gRPC v2 debt fix)

---

## 1. Goal

Implement full WASM module loading for amplifier-core via the WebAssembly Component Model and wasmtime. All 6 module types (Tool, Provider, Orchestrator, ContextManager, HookHandler, ApprovalProvider) get WASM bridges, WIT interface definitions, and a Rust guest SDK. This enables cross-language module authoring — compile a module to `.wasm` once, load it into any host (Python, TypeScript, Rust, future Go/C#).

---

## 2. Background

This is Phase 3 of the 5-phase Cross-Language SDK plan. Phase 3 depends on:

- **PR #35 (Phase 2)** — wasmtime 29→42 upgrade. Wasmtime 42 provides mature Component Model support with the `bindgen!` macro and `wasmtime::component::*` APIs.
- **PR #36 (gRPC debt)** — bidirectional proto conversions (Message, ChatRequest, ChatResponse, HookResult), `Arc<Coordinator>` on Session, all 9 KernelService RPCs implemented.

Both PRs must merge before Phase 3 work begins.

**Current state:** A `WasmToolBridge` stub exists (compiles WASM bytes, satisfies `Arc<dyn Tool>`, but `execute()` returns a hard error). A `Transport::Wasm` variant exists in dispatch. Zero `.wasm` test fixtures, zero `.wit` files, zero component model code.

---

## 3. Key Design Decisions

1. **Thin WIT + proto bytes** — WIT functions accept/return `list<u8>` (proto-serialized bytes), not rich WIT records. Same wire format as gRPC. Proto remains the single source of truth (CORE_DEVELOPMENT_PRINCIPLES §6). A module compiled for gRPC can be recompiled for WASM without code changes.

2. **All 6 module types** — WIT definitions, bridge implementations, and tests for all 6. Tiered delivery within one PR: Tier 1 (pure compute: Tool, HookHandler, ContextManager, ApprovalProvider) first, then Tier 2 (needs host capabilities: Provider with WASI HTTP, Orchestrator with kernel-service host imports).

3. **Developer experience first** — Module authors never see WIT or proto bytes directly. The guest SDK (`amplifier-guest` crate) provides familiar Amplifier types (`ToolSpec`, `ToolResult`, `ChatRequest`, etc.) and a single `export!` macro. Writing a WASM module looks nearly identical to writing a native Rust module.

4. **Shared wasmtime Engine** — Single `Engine` instance reused across all WASM modules (engine creation is expensive, module instantiation is cheap).

5. **Async via spawn_blocking** — WASM execution is synchronous CPU work. Bridges wrap calls in `tokio::task::spawn_blocking()` to avoid blocking the async runtime.

---

## 4. Developer Experience

The goal: a Rust developer writing a WASM Tool module writes code that looks almost identical to writing a native Rust Tool module. The WIT + proto bytes are hidden behind a guest SDK crate.

**Native Rust module today:**
```rust
impl Tool for MyTool {
    fn name(&self) -> &str { "my-tool" }
    fn get_spec(&self) -> ToolSpec { ToolSpec { name: "my-tool".into(), ... } }
    async fn execute(&self, input: Value) -> Result<ToolResult, ToolError> {
        Ok(ToolResult { success: true, output: "done".into(), .. })
    }
}
```

**WASM module with guest SDK:**
```rust
use amplifier_guest::Tool;

struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &str { "my-tool" }
    fn get_spec(&self) -> ToolSpec { ToolSpec { name: "my-tool".into(), ... } }
    fn execute(&self, input: Value) -> Result<ToolResult, ToolError> {
        // same logic, sync (WASM is sync from guest perspective)
        Ok(ToolResult { success: true, output: "done".into(), .. })
    }
}

amplifier_guest::export!(MyTool);  // macro handles WIT binding glue
```

**What the guest SDK hides:**
- WIT interface binding generation (via `wit-bindgen`)
- Proto serialization/deserialization of inputs and outputs
- The `list<u8>` boundary — module authors work with typed structs
- The `export!` macro wires the struct to the WIT exports

**Same types, same names:** `ToolSpec`, `ToolResult`, `ChatRequest`, `ChatResponse`, `HookResult`, `Message` — all the same structs, re-exported through the guest SDK. A developer moving from native Rust to WASM changes their `Cargo.toml` dependency and adds the `export!` macro. The logic stays identical.

For future non-Rust guests (Go, C#, C++ compiled to WASM via TinyGo, NativeAOT, Emscripten): the guest SDK would be a package in that language providing the same interface names. Phase 3 targets Rust guest modules only.

---

## 5. WIT Interface Definitions

All 6 module types defined as WIT interfaces using the thin proto bytes pattern:

```wit
package amplifier:modules@1.0.0;

// === Tier 1: Pure compute (no WASI, no host imports) ===

interface tool {
    get-spec: func() -> list<u8>;
    execute: func(request: list<u8>) -> result<list<u8>, string>;
}

interface hook-handler {
    handle: func(event: string, data: list<u8>) -> result<list<u8>, string>;
}

interface context-manager {
    add-message: func(message: list<u8>) -> result<_, string>;
    get-messages: func() -> result<list<u8>, string>;
    get-messages-for-request: func(request: list<u8>) -> result<list<u8>, string>;
    set-messages: func(messages: list<u8>) -> result<_, string>;
    clear: func() -> result<_, string>;
}

interface approval-provider {
    request-approval: func(request: list<u8>) -> result<list<u8>, string>;
}

// === Tier 2: Needs host capabilities ===

interface provider {
    get-info: func() -> list<u8>;
    list-models: func() -> result<list<u8>, string>;
    complete: func(request: list<u8>) -> result<list<u8>, string>;
    parse-tool-calls: func(response: list<u8>) -> list<u8>;
}

interface orchestrator {
    execute: func(request: list<u8>) -> result<list<u8>, string>;
}
```

**Host-provided imports for Tier 2 modules:**

```wit
// Kernel callbacks — WASM equivalent of gRPC KernelService
interface kernel-service {
    execute-tool: func(name: string, input: list<u8>) -> result<list<u8>, string>;
    complete-with-provider: func(name: string, request: list<u8>) -> result<list<u8>, string>;
    emit-hook: func(event: string, data: list<u8>) -> result<list<u8>, string>;
    get-messages: func() -> result<list<u8>, string>;
    add-message: func(message: list<u8>) -> result<_, string>;
    get-capability: func(name: string) -> result<list<u8>, string>;
    register-capability: func(name: string, value: list<u8>) -> result<_, string>;
}
```

Provider gets WASI HTTP imports (via `wasi:http/outgoing-handler`) for making LLM API calls. Orchestrator gets `kernel-service` host imports for calling back into the kernel.

All complex types are `list<u8>` (proto-serialized bytes). The WIT interfaces are thin wrappers. The proto schema remains the single source of truth.

---

## 6. Component Model Host Infrastructure

**Shared engine:** The current stub creates a new `wasmtime::Engine` per bridge. Phase 3 shares a single `Engine` across all WASM modules. The engine is stored on the Coordinator or passed through the transport layer.

**Module lifecycle:**
1. **Compile time** (once): `cargo component build` produces a `.wasm` component binary
2. **Load time** (once per module): `Component::new()` validates and AOT-compiles the WASM
3. **Instantiate** (per call or pooled): `Linker::instantiate()` creates a `Store` + instance with imports wired

**Bridge pattern** (same as gRPC):
1. Host code calls `bridge.execute(input)`
2. Bridge serializes input to proto bytes
3. Bridge calls WASM export via wasmtime
4. WASM guest deserializes, runs logic, serializes result
5. Bridge deserializes proto bytes back to native type (e.g. `ToolResult`)
6. Returns `Arc<dyn Tool>` result

The key difference from gRPC: no network, no process management. The `.wasm` binary is loaded in-process. The bridge holds a `wasmtime::component::Instance` instead of a `tonic::Channel`.

**Async handling:** WASM execution is synchronous CPU work. The bridge wraps calls in `tokio::task::spawn_blocking()` to avoid blocking the async runtime, then awaits the result.

---

## 7. Guest SDK (`amplifier-guest`)

A Rust crate that module authors depend on. It hides all WIT/proto plumbing behind familiar Amplifier types and a single `export!` macro.

**Crate structure:**
```
amplifier-guest/
├── Cargo.toml          # depends on wit-bindgen, prost, amplifier-core (types only)
├── src/
│   ├── lib.rs          # re-exports types + export! macro
│   ├── types.rs        # ToolSpec, ToolResult, ChatRequest, ChatResponse, etc.
│   └── bindings.rs     # generated from WIT via wit-bindgen (build.rs)
└── wit/
    └── amplifier-modules.wit   # the WIT definitions from Section 5
```

**What it provides to module authors:**
- `amplifier_guest::Tool` trait (same method signatures as `amplifier_core::Tool`, minus the async)
- `amplifier_guest::Provider`, `HookHandler`, `ContextManager`, `Orchestrator`, `ApprovalProvider` traits
- All data types: `ToolSpec`, `ToolResult`, `ChatRequest`, `ChatResponse`, `HookResult`, `Message`, etc.
- `amplifier_guest::export!(MyTool)` macro that generates the WIT binding glue
- For Tier 2 modules: `amplifier_guest::kernel::execute_tool()`, `kernel::complete_with_provider()`, etc. — typed wrappers around the host `kernel-service` imports

**Location:** New crate at `crates/amplifier-guest/`. It is a compile-time dependency for WASM module authors, not a runtime dependency of the kernel.

**Build workflow for module authors:**
```bash
cargo component build --release
# Produces: target/wasm32-wasip2/release/my_tool.wasm
```

---

## 8. Bridge Implementations

6 WASM bridge structs, mirroring the 6 gRPC bridges. Each follows the identical pattern: hold a wasmtime `Instance`, serialize inputs to proto bytes, call the WASM export, deserialize proto bytes back to native types, implement the corresponding Rust trait.

### Tier 1 Bridges (Pure Compute)

| Bridge | Trait | WASM Exports Called | Host Imports |
|---|---|---|---|
| `WasmToolBridge` | `Tool` | `get-spec`, `execute` | None |
| `WasmHookBridge` | `HookHandler` | `handle` | None |
| `WasmContextBridge` | `ContextManager` | `add-message`, `get-messages`, `get-messages-for-request`, `set-messages`, `clear` | None |
| `WasmApprovalBridge` | `ApprovalProvider` | `request-approval` | None |

### Tier 2 Bridges (Needs Host Capabilities)

| Bridge | Trait | WASM Exports Called | Host Imports |
|---|---|---|---|
| `WasmProviderBridge` | `Provider` | `get-info`, `list-models`, `complete`, `parse-tool-calls` | WASI HTTP (`wasi:http/outgoing-handler`) |
| `WasmOrchestratorBridge` | `Orchestrator` | `execute` | `kernel-service` (custom host imports) |

**Each bridge struct holds:**
```rust
pub struct WasmToolBridge {
    engine: Arc<Engine>,                    // shared across all WASM modules
    component: Component,                   // AOT-compiled WASM component
    linker: Linker<WasmState>,             // pre-configured with imports
    name: String,
}
```

**Async wrapping:** All bridge trait methods use `tokio::task::spawn_blocking()` since WASM execution is synchronous CPU work.

**Transport dispatch:** `transport.rs` gets `load_wasm_*` functions for all 6 module types (currently only `load_wasm_tool` exists). Each accepts `&[u8]` or `&Path` and returns `Arc<dyn Trait>`.

---

## 9. Test Fixtures & E2E Testing

### Test Fixtures

All fixtures compiled from Rust guest code using the `amplifier-guest` crate. They live in `tests/fixtures/wasm/` as pre-compiled `.wasm` binaries committed to the repo. A `build-fixtures.sh` script recompiles them from source in `tests/fixtures/wasm/src/`.

| Fixture | Module Type | What it does | Validates |
|---|---|---|---|
| `echo-tool.wasm` | Tool | Returns input as output | Basic WIT + proto roundtrip |
| `deny-hook.wasm` | HookHandler | Returns `HookAction::Deny` | Hook bridge + HookResult serialization |
| `memory-context.wasm` | ContextManager | In-memory message store | Stateful WASM module (multi-call state) |
| `auto-approve.wasm` | ApprovalProvider | Always approves | Approval bridge + proto roundtrip |
| `echo-provider.wasm` | Provider | Returns canned ChatResponse | WASI HTTP imports (mocked in test) |
| `passthrough-orchestrator.wasm` | Orchestrator | Calls one tool via kernel-service import, returns result | Host kernel-service imports |

### E2E Tests

All behind `#[cfg(feature = "wasm")]`:

```rust
#[test] fn load_echo_tool_from_bytes()           // load .wasm, verify name/spec
#[tokio::test] async fn echo_tool_execute()      // full execute roundtrip
#[tokio::test] async fn hook_handler_deny()      // deny hook fires correctly
#[tokio::test] async fn context_manager_roundtrip()   // add + get messages
#[tokio::test] async fn approval_auto_approve()       // approval request → approved
#[tokio::test] async fn provider_complete()            // ChatRequest → ChatResponse
#[tokio::test] async fn orchestrator_calls_kernel()    // orchestrator → host import → tool
```

The **cross-language validation** test loads the same `echo-tool.wasm` from a Python host (via PyO3 bridge) and a TypeScript host (via Napi-RS bridge), proving the `.wasm` binary is truly portable across host languages.

---

## 10. Transport Matrix (Complete Picture)

How all languages connect to the Amplifier kernel:

### Host App Bindings (In-Process)

Run the kernel in your language:

| Language | Binding | Mechanism | Status |
|---|---|---|---|
| Rust | Native | Direct Rust | Complete |
| Python | PyO3 | Rust ↔ CPython FFI | Complete (Phase 1) |
| TypeScript | Napi-RS | Rust ↔ V8 FFI | PR #35 (Phase 2) |
| Go | CGo | Rust ↔ Go FFI via C ABI | Future (TODO #4) |
| C# | P/Invoke | Rust ↔ .NET FFI via C ABI | Future (TODO #4) |
| C/C++ | C header | Direct C ABI | Future (TODO #4) |

### Module Authoring (Cross-Language)

Write a module in any language, plug into any host:

| Transport | Mechanism | Overhead | Use case |
|---|---|---|---|
| Native | Direct Rust traits | Zero | Rust modules in Rust host |
| PyO3 | In-process FFI | Minimal | Python modules in Python host |
| Napi-RS | In-process FFI | Minimal | TS modules in TS host |
| WASM | wasmtime in-process | ~10-70μs/call | Cross-language portable modules (**Phase 3, this work**) |
| gRPC | Out-of-process RPC | ~1-5ms/call | Sidecar/microservice modules |

Developers don't choose transport — Phase 4 (module resolver) auto-detects. WASM is the default cross-language path; gRPC is opt-in for microservice deployments.

---

## 11. Deliverables

1. **`wit/amplifier-modules.wit`** — WIT interface definitions for all 6 module types + `kernel-service` host imports
2. **`crates/amplifier-guest/`** — Rust guest SDK crate with traits, types, `export!` macro, and kernel-service wrappers
3. **6 WASM bridge implementations** in `crates/amplifier-core/src/bridges/` — `WasmToolBridge` (rewritten from stub), `WasmHookBridge`, `WasmContextBridge`, `WasmApprovalBridge`, `WasmProviderBridge`, `WasmOrchestratorBridge`
4. **Shared `Engine` management** — single wasmtime engine reused across all WASM modules
5. **`transport.rs`** — `load_wasm_*` functions for all 6 module types (file path + bytes variants)
6. **6 test fixture `.wasm` binaries** compiled from Rust guest code using `amplifier-guest`
7. **E2E tests** for all 6 module types behind `#[cfg(feature = "wasm")]`
8. **WASI HTTP integration** for Provider bridge, **kernel-service host imports** for Orchestrator bridge

### Tiered Delivery (Within One PR)

- **Tier 1 commits:** WIT definitions + guest SDK + Tier 1 bridges (Tool, HookHandler, ContextManager, ApprovalProvider) + Tier 1 test fixtures and E2E tests. Validates the WIT + Component Model foundation on simpler modules.
- **Tier 2 commits:** Tier 2 bridges (Provider with WASI HTTP, Orchestrator with kernel-service host imports) + Tier 2 test fixtures and E2E tests. Adds host capability complexity on top of the proven foundation.

---

## 12. Dependencies

**Must merge first:**
- **PR #35** (Phase 2) — contains wasmtime 29→42 upgrade. Phase 3 needs wasmtime 42 for mature Component Model APIs (`bindgen!`, `wasmtime::component::*`).
- **PR #36** (gRPC debt) — contains the bidirectional proto conversions (Message, ChatRequest, ChatResponse, HookResult) that the WASM bridges reuse for serialization, plus `Arc<Coordinator>` on Session and all KernelService RPCs implemented.

---

## 13. Not In Scope

- Non-Rust guest SDKs (Go, C#, C++ guest SDKs are Phase 5)
- Module resolver auto-detection of `.wasm` files (Phase 4)
- Browser WASM host (webruntime concern, not kernel)
- Hot-reload of WASM modules
- WASM module marketplace
- Go/C#/C++ native host bindings (in-process, like PyO3/Napi-RS)

---

## 14. Tracked Future Work

Adding to the list from prior phases:

- **Future TODO #4:** Go/C#/C++ native host bindings (in-process, like PyO3/Napi-RS) — CGo, P/Invoke, C ABI
- **Future TODO #5:** Non-Rust WASM guest SDKs (TinyGo, NativeAOT, Emscripten) — so non-Rust authors can compile to `.wasm` targeting the same WIT interfaces
- **Future TODO #6:** WASM module hot-reload