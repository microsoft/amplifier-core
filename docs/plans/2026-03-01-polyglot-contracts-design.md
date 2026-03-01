# Polyglot Contracts Design

## Goal

Enable community module development in any language (Rust, Python, Go, TypeScript, C#, WASM) while maintaining a single source of truth for all contracts via Protocol Buffer definitions.

## Background

Amplifier has a Rust kernel (`amplifier-core`) with Python bindings via PyO3. Today, module contracts are defined in multiple places: hand-written Rust traits, hand-written Python Protocol classes, hand-maintained `.pyi` stubs, and hand-written validation logic. This leads to drift (we shipped a buggy stub with wrong parameter order), limits community contributions to Python only, and creates maintenance burden across boundaries.

Proto files become the single source of truth. gRPC becomes the transport for cross-language modules. The existing Python path (PyO3) remains unchanged throughout the migration.

## Existing Foundation

The repo already has Milestone 1 gRPC work (commit `cd1a093`):

- `proto/amplifier_module.proto` — ToolService with GetSpec + Execute (39 lines)
- `python/amplifier_core/loader_grpc.py` — GrpcToolBridge + `load_grpc_module()`
- `python/amplifier_core/loader_dispatch.py` — Transport router (grpc/python/native/wasm dispatch)
- `python/amplifier_core/_grpc_gen/` — Generated Python stubs (working, tested)
- 20 passing tests
- Clean architecture ready to extend

## Architecture

### Three-Layer Model

```
┌─────────────────────────────────────────────────────────┐
│  Layer 3: Modules (spokes)                              │
│  ┌──────┐ ┌────────┐ ┌────────┐ ┌──────┐ ┌──────────┐  │
│  │ Rust │ │ Python │ │   Go   │ │ WASM │ │TypeScript│  │
│  │native│ │  PyO3  │ │  gRPC  │ │ wasm │ │   gRPC   │  │
│  └──┬───┘ └───┬────┘ └───┬────┘ └──┬───┘ └────┬─────┘  │
│     │         │          │         │           │        │
├─────┼─────────┼──────────┼─────────┼───────────┼────────┤
│  Layer 2: Rust Kernel (hub)                             │
│     │         │          │         │           │        │
│     ▼         ▼          ▼         ▼           ▼        │
│  ┌─────────────────────────────────────────────────┐    │
│  │  Transport Bridges → Arc<dyn Trait>             │    │
│  │  Coordinator · Module Loader · KernelService    │    │
│  └─────────────────────────────────────────────────┘    │
│                                                         │
├─────────────────────────────────────────────────────────┤
│  Layer 1: Proto Contracts (source of truth)             │
│  ┌─────────────────────────────────────────────────┐    │
│  │  amplifier-core/proto/*.proto                   │    │
│  │  → Rust traits · Python stubs · Go interfaces   │    │
│  │  → TypeScript types · C# interfaces · JSON Schema│   │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

**Layer 1: Proto Contracts** — `.proto` files define every module type, every message, every error. All other code (Rust traits, Python interfaces, Go interfaces, TypeScript types) is generated from proto. Proto files live in `amplifier-core/proto/` and are the only hand-maintained contract definitions.

**Layer 2: Rust Kernel** — Central coordinator. Hosts a `KernelService` gRPC server that every out-of-process module calls back into. When any module (context manager, orchestrator, etc.) needs to call a provider, execute a tool, or emit a hook, it calls KernelService. The kernel routes to the appropriate module regardless of that module's language or transport. In-process modules (Rust-native, Python via PyO3) access the coordinator directly as they do today.

**Layer 3: Modules** — Each module implements one proto service. Modules can be: Rust-native (compiled in, zero overhead), Python (PyO3 bridge, unchanged from today), any language via gRPC (out-of-process), or WASM (in-process, sandboxed). Transport is declared in `amplifier.toml`.

**Key design principle:** AI writes ALL code in this ecosystem. Strong types everywhere (Rust, proto) force correct code generation. No opaque bytes, no stringly-typed interfaces.

## Proto Service Definitions

### Module Services (implemented by community modules)

Six services that modules implement, each in any supported language:

**ToolService** — `GetSpec`, `Execute`

**ProviderService** — `GetInfo`, `ListModels`, `Complete`, `CompleteStreaming` (server-side streaming), `ParseToolCalls`

**OrchestratorService** — `Execute` (receives session context, calls back to kernel for all provider/tool/hook access)

**ContextService** — `AddMessage`, `GetMessages`, `GetMessagesForRequest`, `SetMessages`, `Clear`

**HookService** — `Handle` (receives event + data, returns HookResult)

**ApprovalService** — `RequestApproval`

### Kernel Service (hosted by the Rust kernel)

One service that the kernel exposes for out-of-process modules to call back into:

**KernelService** — `CompleteWithProvider`, `CompleteWithProviderStreaming`, `ExecuteTool`, `EmitHook`, `EmitHookAndCollect`, `GetMessages`, `AddMessage`, `GetMountedModule`, `RegisterCapability`, `GetCapability`

This enables full polyglot operation: a Go orchestrator calls `KernelService.ExecuteTool("bash", args)`, the kernel routes to whichever module implements "bash" (Rust, Python, Go, whatever). The orchestrator doesn't know or care about the tool's language.

### Module Lifecycle (shared by all services)

All module services share a lifecycle interface:

- `Mount` — Initialize with config
- `Cleanup` — Graceful shutdown
- `HealthCheck` — Liveness probe
- `GetModuleInfo` — Self-description for future registry/marketplace

### Streaming

`ProviderService.CompleteStreaming` uses proto server-side streaming. `KernelService.CompleteWithProviderStreaming` mirrors it. This is the standard gRPC streaming pattern — no custom chunking protocol.

## Proto Message Types

All messages are fully typed. No opaque bytes. Strong types prevent malformed data at compile time across all languages.

### Module Identity and Lifecycle

- `ModuleInfo` — name, version, description, author, module_type, capabilities list
- `MountRequest` / `MountResponse` — config dict, health status
- `HealthCheckResponse` — status enum, message

### LLM Conversation Types

- `ChatRequest` — model, messages, tools, response_format, temperature, max_tokens, etc. (all fields from Rust ChatRequest)
- `ChatResponse` — content blocks, tool_calls, usage, degradation info
- `Message` — role enum, content blocks, name, tool_call_id
- `ContentBlock` — oneof: text, thinking, tool_call, tool_result, image, reasoning
- `ToolCall` — id, name, arguments (typed, not JSON string)
- `ToolSpec` — name, description, parameters (JSON Schema as typed proto)
- `Usage` — input_tokens, output_tokens, total_tokens

### Module-Specific Types

- `ToolResult` — success, output, error
- `HookResult` — all 15 fields (action enum, data, context_injection, approval fields, etc.)
- `ModelInfo` — id, display_name, context_window, max_output_tokens, capabilities
- `ProviderInfo` — name, display_name, models, config_fields
- `ConfigField` — name, type enum, description, required, default, choices
- `ApprovalRequest` / `ApprovalResponse` — fully typed with enums

### Error Types

Consistent error taxonomy across all languages:

- `AmplifierError` — oneof: provider_error, tool_error, hook_error, session_error, context_error
- `ProviderError` — error_type enum (RateLimit, Authentication, ContextLength, ContentFilter, InvalidRequest, Unavailable, Timeout), message, provider, model, retry_after, retryable
- `ToolError` — type enum (ExecutionFailed, NotFound), message, stdout, stderr, exit_code
- `HookError` — type enum (HandlerFailed, Timeout), message, handler_name

Error types map 1:1 to Rust error enums in `errors.rs`. A Go tool failure returns proto `ToolError` → kernel translates to Rust `ToolError` → Python orchestrator catches same `LLMError` subclass. Same taxonomy, every language, every boundary.

## Transport Abstraction

Four transports, declared per-module in `amplifier.toml`:

```toml
[module]
transport = "python"   # PyO3 in-process (existing, unchanged)
transport = "native"   # Rust linked directly, zero overhead
transport = "grpc"     # Any language, out-of-process, proto contract
transport = "wasm"     # Any language compiled to WASM, in-process, sandboxed
```

### Bridge Pattern

The kernel holds one Rust trait object per module slot (`Arc<dyn Tool>`, `Arc<dyn Provider>`, etc.). The transport layer creates the appropriate bridge wrapper:

| Transport | Bridge | Mechanism |
|---|---|---|
| `python` | `PyO3ToolBridge` | Existing — calls Python via PyO3, returns Rust types |
| `native` | Direct `Arc<dyn Tool>` | No bridge — the Rust module IS the trait impl |
| `grpc` | `GrpcToolBridge` | Calls gRPC service, deserializes proto into Rust types |
| `wasm` | `WasmToolBridge` | Calls WASM runtime via wasmtime, same proto message format |

Once wrapped, the kernel doesn't know or care about transport. The orchestrator calls `tool.execute(args)` — it might be Rust, Python, Go, or WASM behind the trait.

```rust
// Every transport implements the same Rust traits.
// The kernel only sees Arc<dyn Tool>, Arc<dyn Provider>, etc.

struct GrpcToolBridge { client: ToolServiceClient, ... }
impl Tool for GrpcToolBridge {
    fn execute(&self, input: Value) -> ... {
        // Serialize to proto, call gRPC, deserialize response
    }
}

struct WasmToolBridge { instance: WasmInstance, ... }
impl Tool for WasmToolBridge {
    fn execute(&self, input: Value) -> ... {
        // Serialize to proto bytes, call WASM export, deserialize
    }
}
```

All module types (Tool, Provider, Orchestrator, ContextManager, HookHandler, ApprovalProvider) get all four transport options through the same bridge pattern.

### Proto Format Sharing

Both gRPC and WASM use the same proto message format for serialization. A module developed with gRPC (easy debugging) can be deployed as WASM (sandboxed, in-process) without code changes — just recompile.

In-process modules (Python, Rust-native) access the coordinator directly — same as today. Out-of-process modules (gRPC) use KernelService RPCs. The access pattern is transparent to the module author.

## Code Generation and Build Pipeline

Proto generates code for every supported language. Nothing is hand-maintained except the `.proto` files themselves.

### Generated Artifacts

| Artifact | Target | Replaces |
|---|---|---|
| Rust traits + message structs | `crates/amplifier-core/src/generated/` | Hand-written `traits.rs`, `models.rs`, `messages.rs` |
| Rust gRPC server + client (tonic) | `crates/amplifier-core/src/generated/` | Nothing (new) |
| Python type stubs | `python/amplifier_core/_engine.pyi` | Hand-written stub (was buggy) |
| Python Protocol classes | `python/amplifier_core/interfaces.py` | Hand-written Protocols (280 lines) |
| Python gRPC stubs | `python/amplifier_core/_grpc_gen/` | Existing Tool-only stubs |
| Go interfaces + gRPC stubs | `amplifier-go` package | Nothing (new) |
| TypeScript types + gRPC stubs | `@amplifier/core` npm package | Nothing (new) |
| C# interfaces + gRPC stubs | `Amplifier.Core` NuGet package | Nothing (new) |
| JSON Schema | `schemas/` | Hand-written Python validation |

### Build Pipeline

Proto compilation happens in CI, not at user install time. Generated code is committed to each language's repo.

- Users in any language get pre-generated types — no protoc dependency
- CI verifies generated code is in sync with proto on every PR
- A single `make proto` regenerates everything

### Migration Path (Zero Disruption to Python)

Generated Rust traits initially wrap the existing hand-written traits as a compatibility layer. Over time, generated code becomes primary and hand-written code is removed. Python never notices — it talks to the same PyO3 bridge.

1. Proto generates code alongside existing hand-written code. Tests verify equivalence.
2. New code paths use generated types. Old code still works.
3. Old hand-written code removed. Generated code is sole source.

## Code Quality Items

Incorporated from production audit findings:

### 1. `vars()` on Rust-Backed Objects

`vars(coordinator)` returns only the Python `__dict__`, missing all Rust-managed state. As more types become Rust-backed, this gets worse.

**Fix:** Add a proto-generated `to_dict()` method on Rust types that includes all fields. The serialization code checks for `to_dict()` before falling back to `vars()`. Enables proper serialization for debugging, logging, and the future module registry.

### 2. Private Attribute Probing on Resolvers

`_bundle`, `_paths`, `_bundle_mappings` accessed via `hasattr` chains in `session_spawner.py`. Renames break silently.

**Fix:** Define public `get_module_paths()` / `get_mention_mappings()` methods on resolver classes. The session spawner uses the public API. Also sets up resolvers for potential future remote gRPC resolution.

### 3. Generated `_engine.pyi` Eliminates Stub Drift

Today's bug (wrong parameter order, wrong return type in the hand-written stub) becomes structurally impossible. The stub is generated from proto, not hand-maintained.

### 4. Validation Moves to Proto Schema

The 3,287 lines of hand-written Python validation (`validation/`) gradually shrinks. Structural checks (method presence, parameter types, return types) are handled by the proto compiler for compiled languages and by proto-based JSON Schema validation for Python. Only Python-introspection checks (import, async, module discovery) remain in Python — and those get smaller as more modules move to gRPC where the proto contract IS the validation.

## Migration Phases

### Phase 1: Proto Expansion (No Runtime Changes)

Expand `proto/amplifier_module.proto` from 1 service to all 7 (6 module services + KernelService). Define all typed messages. This is purely additive — no existing code changes. Generate stubs for all languages but don't wire them in yet. Existing Python path continues to work unchanged.

**Deliverable:** Complete proto definitions, generated stubs for Python/Rust/Go/TypeScript, CI pipeline for proto generation.

### Phase 2: Rust gRPC Infrastructure

Add tonic/prost to `crates/amplifier-core`. Implement `KernelService` gRPC server in the Rust kernel. Implement the transport bridge traits (`GrpcToolBridge`, `GrpcProviderBridge`, etc.) that translate between proto messages and Rust trait objects. Add transport dispatch to the module loader.

**Deliverable:** A Go or TypeScript tool can be loaded and executed via gRPC. Python path still unchanged.

### Phase 3: Rust-Native Module Support

Enable `transport = "native"` — Rust modules that implement traits directly without any bridge. This is the simplest transport (no serialization) but requires the generated Rust traits from proto to be in place.

**Deliverable:** A Rust tool module can be compiled and loaded without Python or gRPC.

### Phase 4: Generated Code Replaces Hand-Written

The generated Python stubs, interfaces, and type stubs replace the hand-written versions. The validation framework switches from Python introspection to proto schema validation where possible. `_engine.pyi`, `interfaces.py` become generated artifacts.

**Deliverable:** Hand-written contract code eliminated. Single source of truth achieved.

### Phase 5: WASM Transport

Add wasmtime to the kernel. Implement `WasmToolBridge` etc. Same proto message format, different transport. Community modules can target WASM for sandboxed, in-process execution.

**Deliverable:** WASM modules run with the same contracts as gRPC modules.

**Key principle across all phases:** The existing Python path is never broken. Each phase adds capability without removing anything that works.

## Decisions Made

1. **Proto is THE source of truth** for all contracts across all languages.
2. **Fully typed proto messages** — no opaque bytes. AI writes all code; strong types prevent bugs.
3. **Hub-and-spoke architecture** — the kernel is central; all modules talk to the kernel, not each other.
4. **Four transports** — python, native, grpc, wasm — covering all deployment models.
5. **Rust modules can choose** in-process (native) OR out-of-process (gRPC).
6. **All module types writable in any language** — tools, providers, orchestrators, context managers, hooks, approval.
7. **KernelService** enables any module to access any other module's capabilities through the kernel.
8. **Streaming via proto server-side streaming** for provider completions.
9. **Error taxonomy in proto** — consistent error types across all languages and boundaries.
10. **Module lifecycle in proto** — mount/cleanup/health/info are universal across all module types.

## Doors Left Open

These decisions are deferred. The architecture preserves room for all of them without committing prematurely.

1. **Module registry/marketplace** — `ModuleInfo` in proto is a sufficient foundation when the time comes.
2. **Sandboxing policy** — Hub-and-spoke IS the trust boundary; WASM adds in-process sandboxing later.
3. **Module-to-module communication** — KernelService already supports this pattern if needed.
4. **WASM transport details** — Phase 5; the proto message format is already compatible.

## Open Questions

1. **ToolSpec.parameters typing** — Should `ToolSpec.parameters` use proto's native typing or embed JSON Schema as a typed proto message? JSON Schema is the LLM standard but doesn't map perfectly to proto types.
2. **Mixed streaming semantics** — How should the kernel handle mixed streaming + non-streaming provider calls? Some providers support streaming, others don't — the orchestrator needs to handle both transparently.
3. **Module discovery for gRPC** — What's the discovery mechanism for gRPC modules? Currently `amplifier.toml` per-module, but a central registry may be needed as the ecosystem grows.
