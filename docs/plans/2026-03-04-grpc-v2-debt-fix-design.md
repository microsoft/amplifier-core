# gRPC Phase 2 Debt Fix Design

## Goal

Fix all gRPC Phase 2 debt in amplifier-core — 15 code `TODO(grpc-v2)` markers across 4 bridge files, implement 8 stubbed KernelService RPCs, and make remote cross-language orchestrators fully functional.

## Background

The gRPC bridge layer was built during the Rust kernel migration with known data loss documented via `TODO(grpc-v2)` markers and `log::debug!()` calls. The audit design doc (`docs/plans/2026-03-03-audit-fix-design.md`) prescribed "document, don't fix" as the initial strategy. This design addresses the actual fixes.

15 code TODOs across 4 files:

- `conversions.rs`: 3 (Usage optional fields)
- `grpc_context.rs`: 8 (message fields + content + provider_name)
- `grpc_approval.rs`: 2 (optional timeout)
- `grpc_orchestrator.rs`: 2 (session_id + discarded params)

Plus 8 of 9 KernelService RPCs stubbed as `Status::unimplemented` in `grpc_server.rs`. Only `ExecuteTool` is implemented.

## Approach

Single PR, 4 layered commits working bottom-up through the dependency chain:

1. Proto schema fixes
2. Bidirectional conversions (bulk of the work)
3. Session routing and bridge fixes
4. KernelService RPC implementation

Changes are tightly coupled — proto schema changes flow into bridge fixes which flow into KernelService. Splitting across PRs would mean intermediate states where the proto is updated but bridges aren't. Layered commits within one PR give clean git history while shipping atomically.

## Architecture

The fix touches four layers of the gRPC subsystem, each building on the one below:

```
┌─────────────────────────────────────────────────┐
│  Layer 4: KernelService RPCs (grpc_server.rs)   │  ← Remote modules call back
├─────────────────────────────────────────────────┤
│  Layer 3: Bridge Fixes (orchestrator, context,  │  ← Session routing, params
│           approval, provider)                   │
├─────────────────────────────────────────────────┤
│  Layer 2: Conversions (Message, ChatRequest,    │  ← ~60% of total effort
│           ChatResponse, HookResult)             │
├─────────────────────────────────────────────────┤
│  Layer 1: Proto Schema (amplifier_module.proto) │  ← optional fields
└─────────────────────────────────────────────────┘
```

## Components

### Layer 1: Proto Schema Fixes

Add `optional` keyword to 5 fields in `proto/amplifier_module.proto`:

```protobuf
// Usage message — 3 token count fields
optional int32 reasoning_tokens = 4;
optional int32 cache_read_tokens = 5;
optional int32 cache_creation_tokens = 6;

// ApprovalRequest — 1 timeout field
optional double timeout = 5;

// HookResult — 1 timeout field (same None/0.0 ambiguity)
optional double approval_timeout = 9;
```

**Why:** Proto3 bare scalars default to `0`/`0.0` on the wire, making `None` (not reported / wait forever) and `Some(0)` (zero tokens / expire immediately) indistinguishable. The `optional` keyword generates `Option<T>` in Rust.

**Wire compatibility:** Adding `optional` to an existing proto3 field is backward-compatible — old readers treat the field the same way, new readers get `Option<T>`.

**After proto change:** Regenerate Rust code via `cargo build` (with protoc installed — `build.rs` auto-regenerates `src/generated/amplifier.module.rs`). Commit both the proto change AND the regenerated Rust code together. Update `conversions.rs` to map `None ↔ None` instead of `unwrap_or(0)`, and update `grpc_approval.rs` to send `None` instead of `0.0`.

### Layer 2: Bidirectional Conversions

This is the foundation for everything else and the bulk of the work (~60% of total effort). Build complete bidirectional conversions between native Rust types and proto types.

**New conversions to write:**

1. **`Message ↔ proto::Message`** (with ContentBlock, Role mapping):
   - `value_to_proto_message()`: Use `serde_json::from_value::<Message>(value)` for type-safe parsing (not hand-parsing JSON keys). Map `Role` enum to proto `Role`, extract `name`, `tool_call_id`, `metadata` (serialize to JSON string), handle both `MessageContent::Text` (→ TextContent) and `MessageContent::Blocks` (→ BlockContent with all 7 ContentBlock variants: text, thinking, redacted_thinking, tool_call, tool_result, image, reasoning)
   - `proto_message_to_value()`: Full fidelity reverse — map proto Role back to string, populate name/tool_call_id/metadata, handle BlockContent by iterating proto ContentBlock entries

2. **`ChatRequest ↔ proto::ChatRequest`** (with ToolSpec, ResponseFormat):
   - Native `ChatRequest` (messages.rs) includes messages, model, system prompt, tools, response_format, temperature, max_tokens, etc.
   - Requires Message conversion from above, plus ToolSpec and ResponseFormat mapping

3. **`ChatResponse ↔ proto::ChatResponse`** (with ToolCall, Usage, Degradation):
   - Native `ChatResponse` includes content, tool_calls, usage, degradation, model, stop_reason
   - Requires ToolCall, Usage (updated for `optional` fields from Layer 1), and Degradation mapping

4. **`HookResult native → proto`** (reverse of existing `grpc_hook.rs` conversion):
   - `grpc_hook.rs` already has `proto_to_native_hook_result()`. Need the reverse: `native_to_proto_hook_result()`
   - Needed for KernelService `EmitHook` and `EmitHookAndCollect` RPCs

5. **Update existing `Usage` conversion** for `optional` fields from Layer 1

**Fix `GrpcContextBridge`** message conversion — now uses the proper Message ↔ proto conversion.

**Fix `GrpcProviderBridge::complete()`** — currently a stub returning `Err(ProviderError::Other)`. Now possible with ChatRequest/ChatResponse conversions.

### Layer 3: Session Routing & Bridge Fixes

**Critical fix — `session_id` routing:**

Store `session_id` on `GrpcOrchestratorBridge` struct at construction time. Cannot modify `Orchestrator` trait signature — that would be a breaking change affecting all orchestrator implementations.

```rust
pub struct GrpcOrchestratorBridge {
    client: tokio::sync::Mutex<OrchestratorServiceClient<Channel>>,
    session_id: String,  // Set at construction
}
```

Populate `session_id` in `OrchestratorExecuteRequest`. This enables KernelService to route callbacks to the correct session's Coordinator.

**5 discarded orchestrator parameters — by design:**

The `Orchestrator::execute()` trait passes `context`, `providers`, `tools`, `hooks`, `coordinator` — but these can't be serialized over gRPC. Remote orchestrators access these via KernelService callbacks instead (which Layer 4 implements). Remove `TODO(grpc-v2)` markers, replace with clear doc comment: "Remote orchestrators access these via KernelService RPCs using session_id." The `log::debug!()` calls remain as operational telemetry.

**Approval timeout fix:**

After proto Layer 1 lands (`optional double timeout`), update `map_approval_timeout()`:

- `None` → proto `None` (not `0.0`)
- `Some(0.0)` → proto `Some(0.0)` (expire immediately)
- `Some(30.0)` → proto `Some(30.0)` (30 second timeout)

**Provider name fix:**

In `get_messages_for_request()`, call `provider.name()` on the passed `Arc<dyn Provider>` and populate the `provider_name` field.

### Layer 4: KernelService Implementation

**Architecture:** Each `KernelServiceImpl` is scoped to one session's `Arc<Coordinator>`. NOT a session registry HashMap. The kernel provides the mechanism (one service instance per coordinator); the app layer manages session multiplexing.

**Prerequisite — Session Coordinator sharing:**

- Change `Session` internal storage from `coordinator: Coordinator` to `coordinator: Arc<Coordinator>`
- Add `coordinator_shared() -> Arc<Coordinator>` method
- Keep existing `coordinator() -> &Coordinator` and `coordinator_mut() -> &mut Coordinator` working via Arc derefs / `Arc::get_mut()` (safe during setup when there's one ref)
- Document lifecycle constraint: `coordinator_mut()` only callable before `Arc` is shared

**8 RPCs to implement, in priority order:**

| Priority | RPC | Depends on | Effort |
|----------|-----|-----------|--------|
| 1 | `GetCapability` | Just coordinator access | Small |
| 1 | `RegisterCapability` | Just coordinator access | Small |
| 2 | `GetMountedModule` | Just coordinator access | Small |
| 3 | `AddMessage` | Layer 2 Message conversion | Medium |
| 3 | `GetMessages` | Layer 2 Message conversion | Medium |
| 4 | `EmitHook` | Layer 2 native→proto HookResult | Medium |
| 4 | `EmitHookAndCollect` | Same + timeout + collect semantics | Medium |
| 5 | `CompleteWithProvider` | Full ChatRequest/ChatResponse conversion | Large |
| 6 | `CompleteWithProviderStreaming` | Wrap single complete() as one-shot stream | Large |

**Streaming approach:** `CompleteWithProviderStreaming` wraps a single `provider.complete()` call into one streamed chunk for now. True streaming requires a Provider trait change (`complete_stream()`) — tracked as separate future work.

**Each RPC follows the same pattern:**

1. Extract `session_id` from request
2. Use internal `Arc<Coordinator>` (already scoped to this session)
3. Call the appropriate method on Coordinator/subsystem
4. Serialize response using Layer 2 conversions
5. Return `Result`

## Data Flow

**Outbound (kernel → remote orchestrator):**

```
Session.execute()
  → GrpcOrchestratorBridge.execute(session_id, messages)
    → Message → proto::Message conversion (Layer 2)
    → OrchestratorExecuteRequest { session_id, messages, provider_name }
    → gRPC call to remote orchestrator
```

**Inbound (remote orchestrator → kernel via KernelService):**

```
Remote orchestrator calls KernelService RPC (e.g., CompleteWithProvider)
  → KernelServiceImpl receives request
  → Uses scoped Arc<Coordinator> (no session lookup needed)
  → proto::ChatRequest → native ChatRequest (Layer 2)
  → coordinator.providers.get(name).complete(request)
  → native ChatResponse → proto::ChatResponse (Layer 2)
  → gRPC response back to remote orchestrator
```

## Error Handling

- **Session not found:** Not applicable — `KernelServiceImpl` is per-session, not a registry. If the session is gone, the gRPC connection is closed.
- **Provider not found:** `CompleteWithProvider` returns `Status::not_found` with the requested provider name.
- **Conversion failures:** `Status::internal` with descriptive message (e.g., "failed to deserialize Message from proto: missing role field").
- **Coordinator method errors:** Map native error types to appropriate gRPC status codes (`InvalidArgument`, `NotFound`, `Internal`).
- **Timeout on `EmitHookAndCollect`:** Respect the timeout field from the request; return partial results if timeout expires.

## Testing Strategy

- **Proto schema:** Existing `proto-check.yml` CI workflow validates proto changes
- **Conversions:** Unit tests for each new bidirectional conversion (Message, ChatRequest, ChatResponse, HookResult) — roundtrip tests proving `native → proto → native` is lossless
- **Bridge fixes:** Update existing bridge tests that assert lossy behavior to assert full fidelity instead
- **Remove TODO-presence tests:** Tests in `grpc_orchestrator.rs:134` and `grpc_context.rs:290` that assert `TODO(grpc-v2)` markers exist — replace with fidelity tests
- **KernelService RPCs:** Integration tests per RPC — construct `KernelServiceImpl` with a test Coordinator, call RPC, verify response
- **End-to-end:** At least one test that exercises: create session → start KernelService → remote orchestrator calls back via KernelService → verify roundtrip

## Scope & Boundaries

**In scope:**

- 5 proto `optional` field additions + regeneration
- All bidirectional conversions (Message, ChatRequest, ChatResponse, HookResult)
- Fix all 15 code `TODO(grpc-v2)` markers
- Fix `GrpcProviderBridge::complete()` stub
- Session coordinator sharing (`Arc<Coordinator>`)
- All 8 KernelService RPC implementations
- Update/remove doc references to `TODO(grpc-v2)` where fixes land

**Not in scope:**

- Provider trait streaming extension (separate future PR)
- Multi-session multiplexing over single gRPC port (app-layer concern)
- `process_hook_result()` porting to Rust (tracked as Future TODO #2 from Phase 2)

## Key Design Decisions

1. **Single PR, layered commits** — changes are tightly coupled; intermediate states would be broken
2. **KernelServiceImpl stays per-session** — not a session registry; kernel provides mechanism, app provides policy
3. **Session stores `Arc<Coordinator>`** — minimal change to enable sharing; existing API preserved
4. **`session_id` stored on bridge at construction** — not passed through Orchestrator trait (would be breaking change)
5. **Streaming RPC wraps single `complete()`** — true streaming deferred to Provider trait extension
6. **Type-safe Message parsing** — use `serde_json::from_value::<Message>()`, not hand-parsing JSON keys
7. **5 discarded orchestrator params remain discarded** — by design, remote orchestrators use KernelService callbacks

## Open Questions

None — all design points validated during brainstorm with core expert review.