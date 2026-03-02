# Cross-Language AmplifierSession SDK Design

> Design for making the AmplifierSession developer experience available in every language with the same mental model.

**Status:** Approved  
**Date:** 2026-03-02  
**Prerequisite:** Polyglot contracts (Tasks 1-29) — COMPLETE  
**Philosophy:** See `foundation:context/LANGUAGE_PHILOSOPHY.md` and `core:docs/CORE_DEVELOPMENT_PRINCIPLES.md`

---

## 1. Goal

A developer who knows Amplifier in one language can use it in any other. Same class names, same lifecycle, same config shape. Only language idioms (casing, async syntax, error handling) change.

```python
# Python
async with AmplifierSession(config) as session:
    result = await session.execute("Hello")
```

```typescript
// TypeScript
const session = new AmplifierSession(config);
await session.initialize();
const result = await session.execute("Hello");
await session.cleanup();
```

```rust
// Rust
let session = AmplifierSession::new(config)?;
session.initialize().await?;
let result = session.execute("Hello").await?;
session.cleanup().await?;
```

```go
// Go
session, err := amplifier.NewSession(config)
defer session.Cleanup(ctx)
err = session.Initialize(ctx)
result, err := session.Execute(ctx, "Hello")
```

```csharp
// C#
var session = new AmplifierSession(config);
await session.InitializeAsync();
var result = await session.ExecuteAsync("Hello");
await session.CleanupAsync();
```

---

## 2. Architecture

### The Binding Model

Each language gets the Rust kernel in-process via native bindings. No gRPC between app and kernel.

```
┌─────────────────────────────────────────┐
│  Language-idiomatic SDK                  │
│  (AmplifierSession, Coordinator, Tool)   │
│  Same nouns, same verbs, same config     │
├─────────────────────────────────────────┤
│  Thin binding layer                      │
│  (PyO3 / Napi-RS / CGo / P/Invoke)      │
├─────────────────────────────────────────┤
│  Rust kernel                             │
│  (Session, Coordinator, HookRegistry)    │
│  One implementation, many faces          │
└─────────────────────────────────────────┘
```

| Language | Binding tech | Status |
|----------|-------------|--------|
| Python | PyO3 | **Done** (v1.0.7 on PyPI) |
| TypeScript | Napi-RS | Phase 2 |
| Go | CGo + C ABI | Phase 5 |
| C# | P/Invoke + C ABI | Phase 5 |
| WASM/Browser | wasm-bindgen | Future |

### Transport Model (for modules)

Modules from any language work in any app. Transport is invisible.

| Module language | Same-language app | Different-language app |
|----------------|-------------------|----------------------|
| Rust | Native (zero overhead) | Native (compiled into kernel) |
| Python | PyO3 in-process | WASM or gRPC |
| TypeScript | Napi-RS in-process | WASM or gRPC |
| Go, C#, etc. | CGo/P/Invoke in-process | WASM (universal) |
| Any → .wasm | WASM in-process | WASM in-process |

gRPC is opt-in for microservice deployments. WASM is the default cross-language path. Developers never choose transport — the framework resolves it.

### KernelService Is Internal

Python developers don't know KernelService exists. Neither does anyone else. The SDK provides proxy objects that look local but route through KernelService under the hood when needed.

---

## 3. Cross-Language Type Map

### Behavioral types (classes with methods)

| Concept | Universal name | Python | TS | Rust | Go | C# |
|---------|---------------|--------|-----|------|-----|-----|
| Session | `AmplifierSession` | `AmplifierSession` | `AmplifierSession` | `AmplifierSession` | `amplifier.Session` | `AmplifierSession` |
| Coordinator | `Coordinator` | `Coordinator` (alias: `ModuleCoordinator`) | `Coordinator` | `Coordinator` | `amplifier.Coordinator` | `Coordinator` |
| Hook registry | `HookRegistry` | `HookRegistry` | `HookRegistry` | `HookRegistry` | `amplifier.HookRegistry` | `HookRegistry` |
| Cancellation | `CancellationToken` | `CancellationToken` | `CancellationToken` | `CancellationToken` | `amplifier.CancellationToken` | `CancellationToken` |

### Module interfaces (implemented by module authors)

Same 6 interfaces in every language: `Tool`, `Provider`, `Orchestrator`, `ContextManager`, `HookHandler`, `ApprovalProvider`.

- Python: `Protocol` (structural typing)
- TypeScript: `interface`
- Rust: `trait`
- Go: `interface`
- C#: `interface`

### Data types (proto-generated)

All data types come from proto and are generated per-language: `ChatRequest`, `ChatResponse`, `Message`, `ContentBlock`, `ToolSpec`, `ToolCall`, `ToolResult`, `HookResult`, `Usage`, `ModelInfo`, `ProviderInfo`, `ApprovalRequest`, `ApprovalResponse`, all error types.

Same structure, same field names, same semantics. Proto is the source of truth.

---

## 4. Universal Session Lifecycle

```
new(config)              →  Session exists, not initialized
  │
initialize()             →  Modules loaded from config, mounted on coordinator
  │                         Emits: session:start (or session:resume, session:fork)
  │
execute(prompt) → string →  Orchestrator runs the agent loop
  │                         Can be called multiple times
  │
cleanup()                →  Cleanup functions run in reverse order
                            Emits: session:end
```

### AmplifierSession methods

| Method | Signature | Notes |
|--------|-----------|-------|
| constructor | `(config, session_id?, parent_id?)` | Config is a dict/map. session_id auto-generated if omitted. |
| initialize | `() → void` | Loads modules from config, mounts on coordinator. Idempotent. |
| execute | `(prompt: string) → string` | Runs the orchestrator. Requires initialized. |
| cleanup | `() → void` | Runs cleanup fns in reverse order. Emits session:end. |
| coordinator | property → `Coordinator` | Access to mount points, hooks, capabilities. |
| session_id | property → `string` | UUID. |
| parent_id | property → `string?` | Set for forked/child sessions. |
| config | property → `map` | The original config dict. |
| is_initialized | property → `bool` | True after initialize(). |
| is_resumed | property → `bool` | True if resumed session. |

### Coordinator methods

| Method | Signature | Notes |
|--------|-----------|-------|
| mount_tool | `(name, tool) → void` | |
| mount_provider | `(name, provider) → void` | |
| set_orchestrator | `(orchestrator) → void` | |
| set_context | `(context_manager) → void` | |
| get_tool | `(name) → Tool?` | |
| get_provider | `(name) → Provider?` | |
| tools | `() → map<string, Tool>` | All mounted tools |
| providers | `() → map<string, Provider>` | All mounted providers |
| hooks | property → `HookRegistry` | |
| cancellation | property → `CancellationToken` | |
| register_capability | `(name, value) → void` | |
| get_capability | `(name) → any?` | |
| register_cleanup | `(fn) → void` | |
| cleanup | `() → void` | Reverse-order cleanup |
| to_dict | `() → map` | Introspection |

### Config shape (universal)

```json
{
  "session": {
    "orchestrator": "loop-basic",
    "context": "context-simple"
  },
  "providers": [
    {"module": "provider-anthropic", "config": {"model": "claude-sonnet-4-5"}}
  ],
  "tools": [
    {"module": "tool-bash"}
  ],
  "hooks": [
    {"module": "hooks-logging"}
  ]
}
```

Same JSON shape in every language. No language-specific config format.

---

## 5. Language-Specific Conveniences (Additive)

Each language MAY add idiomatic sugar on top of the universal API. These are extras, not replacements.

| Language | Convenience | How |
|----------|------------|-----|
| Python | `async with` context manager | `__aenter__` / `__aexit__` (existing) |
| TypeScript | `Disposable` protocol | `Symbol.asyncDispose` |
| Rust | Builder pattern | `AmplifierSession::builder().orchestrator("loop-basic").build()` |
| Go | Functional options | `amplifier.NewSession(config, amplifier.WithTool("bash", &BashTool{}))` |
| Go | `defer` cleanup | `defer session.Cleanup(ctx)` |
| C# | `IAsyncDisposable` | `await using var session = ...` |

---

## 6. Implementation Phases

### Phase 1: Clean up the Rust public API

Make the Rust crate usable as a standalone library with the universal API surface.

- Rename/alias to match universal names (`AmplifierSession`, `Coordinator`)
- Ensure all universal surface methods exist on the Rust types
- Add `initialize()` flow (config-driven module loading) to Rust
- Add `AmplifierSession::builder()` convenience
- Tests: Rust-native app creates session, mounts EchoTool, executes

### Phase 2: Napi-RS TypeScript bindings

Same approach as PyO3 — thin binding layer translating between JS and Rust.

- Set up Napi-RS build infrastructure (`bindings/node/`)
- Expose `AmplifierSession`, `Coordinator`, `HookRegistry`, `CancellationToken`
- Expose 6 module interfaces as TypeScript interfaces
- TypeScript type definitions (`.d.ts`) from bindings
- Config-driven `initialize()` and imperative `mountTool()` both work
- Tests: TypeScript app mirroring the Python test suite

### Phase 3: WASM module loading (full implementation)

Make the `WasmToolBridge` stub real.

- Implement WASM component model host↔guest interface
- Define WASM ABI for each module type
- Module resolver auto-detects `.wasm` and loads via wasmtime
- Tests: compile EchoTool to WASM, load from both Python and TypeScript

### Phase 4: Cross-language module resolver

The piece that makes transport invisible.

- Module source inspection (detect `Cargo.toml`, `package.json`, `go.mod`, `.wasm`)
- Transport selection logic (native if same language, WASM if cross-language, gRPC if explicit)
- Pre-built `.wasm` artifact resolution
- Integration with existing bundle YAML (no config format changes)

### Phase 5: Go and C# SDKs

Same pattern via C ABI.

- Expose Rust kernel via stable C ABI (`amplifier_ffi.h`)
- Go SDK wrapping CGo calls in idiomatic Go types
- C# SDK wrapping P/Invoke in idiomatic C# types
- Same universal surface, same tests

---

## 7. Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Binding approach | Native per language (PyO3, Napi-RS, CGo) | Library feel, not service. Same as Python. |
| Cross-language module transport | WASM (default), gRPC (opt-in) | In-process, sandboxed, portable. No services to run. |
| Coordinator naming | `Coordinator` everywhere, `ModuleCoordinator` as Python alias | Clean universal name, backward compat preserved. |
| API parity | Same names + language casing. Conveniences are additive. | Maximum knowledge transfer across languages. |
| Config format | Same JSON shape everywhere | Learn once, use anywhere. |
| KernelService visibility | Internal only — hidden behind SDK proxy objects | Developers never write proto or gRPC. |
| Proto role | Source of truth for data types and module contracts | Generated per-language, not hand-written. |
| Rust role | Source of truth for logic, types, and validation | Language layers are thin translators (CORE_DEVELOPMENT_PRINCIPLES §5). |

---

## 8. Not In Scope (Future Work)

- `process_hook_result()` cross-language routing (injection, approval, display)
- Module marketplace / pre-built `.wasm` registry
- Hot-reload of WASM modules at runtime
- Browser WASM host (different constraints from server-side wasmtime)
- Utils consolidation (migrate Python utils to Rust — tracked in CORE_DEVELOPMENT_PRINCIPLES §5)
