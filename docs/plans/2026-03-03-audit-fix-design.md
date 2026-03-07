# Amplifier-Core Audit Fix Design

## Goal

Fix all 30+ findings from the amplifier-core cross-boundary audit report by restoring the observability that the original Python amplifier-core had via `logging.getLogger(__name__)`, which was lost in the Rust rewrite.

## Background

The amplifier-core Rust rewrite introduced systematic silent error swallowing across three boundary layers: the PyO3 bridge (Python ↔ Rust), the Rust kernel, and the gRPC bridge (Rust ↔ external services). The audit cataloged 30+ sites where errors are silently dropped via `.ok()`, `.unwrap_or_default()`, `unwrap_or_else(|_| ...)`, bare `continue` in error arms, and `eprintln!()` calls that bypass structured logging.

The original Python amplifier-core used `logging.getLogger(__name__)` consistently at every error boundary. The Rust rewrite replaced that with silence. This design restores observability without changing any runtime behavior — every fallback/continue/default still happens, but now it's visible.

## Approach

Add the Rust `log` crate as the kernel's logging facade, bridged to Python's `logging` module via `pyo3-log`. Then systematically fix all 30+ audit findings by adding `log::warn!()` / `log::error!()` / `log::debug!()` at every silent fallback site, matching the Python version's severity levels.

This matches the original Python amplifier-core's logging strategy: no hook events for internal errors (hooks are for lifecycle events), just standard structured logging.

**Key constraint:** No behavioral changes. Every site still continues, falls back, or returns defaults exactly as before. This PR adds visibility only.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Python Application                                      │
│  ┌────────────────────────────────────────────────────┐  │
│  │  logging module (stdlib)                           │  │
│  │  └─ amplifier_core logger hierarchy                │  │
│  │     ├─ amplifier_core.coordinator                  │  │
│  │     ├─ amplifier_core.hooks                        │  │
│  │     ├─ amplifier_core.grpc_hook                    │  │
│  │     └─ ...                                         │  │
│  └────────────────▲───────────────────────────────────┘  │
│                   │ pyo3-log routes automatically         │
│  ┌────────────────┴───────────────────────────────────┐  │
│  │  PyO3 Bridge (bindings/python/src/lib.rs)          │  │
│  │  └─ pyo3_log::init() at module load                │  │
│  └────────────────▲───────────────────────────────────┘  │
│                   │                                       │
│  ┌────────────────┴───────────────────────────────────┐  │
│  │  Rust Kernel (crates/amplifier-core/src/)          │  │
│  │  └─ log::warn!(), log::error!(), log::debug!()     │  │
│  └────────────────▲───────────────────────────────────┘  │
│                   │                                       │
│  ┌────────────────┴───────────────────────────────────┐  │
│  │  gRPC Bridge (crates/amplifier-core/src/grpc_*)    │  │
│  │  └─ log::warn!(), log::debug!()                    │  │
│  └────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘

Non-Python consumers (pure Rust, gRPC services):
  log calls → no-op by default
  App wires up env_logger / tracing-log / etc. as needed
```

## Components

### 1. Logging Infrastructure

**Dependencies added:**

| Crate | Location | Purpose |
|-------|----------|---------|
| `log` | `crates/amplifier-core/Cargo.toml` | Logging facade, used by all Rust kernel code |
| `pyo3-log` | `bindings/python/Cargo.toml` | Bridges Rust `log` → Python `logging` at module init |

**Initialization:**

`pyo3_log::init()` is called as the first line of `fn _engine(...)` in `bindings/python/src/lib.rs`. This routes all Rust `log::*!()` calls through Python's `logging` module.

**Logger name mapping:**

`pyo3-log` uses Rust's `module_path!()` which produces names like `amplifier_core::hooks`. It automatically converts `::` to `.` for the Python logger hierarchy. This matches what the original Python code did with `logging.getLogger(__name__)`.

**Existing code migration:**

- 2 existing `eprintln!()` calls (lines 164, 1306 of `lib.rs`) → `log::warn!()` / `log::error!()`
- 8 manual `py.import("logging")` / `getLogger()` calls in cleanup paths → simple `log::error!()` calls (`pyo3-log` handles routing)

**Non-Python consumers:** When there's no Python runtime (pure Rust apps, future native CLI), `log` calls are no-ops by default. The application wires up any `log` subscriber it wants (`env_logger`, `tracing-log`, etc.). This is the standard Rust library pattern — the library emits, the application decides.

### 2. Silent Error Fix — Kernel Layer

**Scope:** P0-8, P0-9, P1-10 in `crates/amplifier-core/src/` (coordinator.rs, hooks.rs)

**P0-8 — `coordinator.rs` contribution collection (~line 324):**

- Current: `Err(_e) => { continue; }` with comment "Log and skip" but no actual log
- Fix: `log::warn!("Contributor {name} failed: {e}")` + continue
- Python equivalent: `logger.warning()` at this site

**P0-9 — `hooks.rs` `emit_and_collect()` (~lines 306-313), two arms:**

- Handler error arm `Ok(Err(_e))`: `log::error!()` + continue (Python used `logger.error()`)
- Timeout arm `Err(_)`: `log::warn!()` + continue (Python used `logger.warning()`)
- Both include handler name and event name in messages, matching the pattern in `emit()` at line 214

**P1-10 — `hooks.rs` `emit()` Modify serialization (~line 232):**

- Current: `serde_json::to_value(modified).unwrap_or(current_data)` — silently drops modification
- Fix: `log::warn!()` in error case before falling back
- Rationale: handler asked to modify data and the modification was silently dropped

### 3. Silent Error Fix — PyO3 Bridge Layer

**Scope:** P0-1, P0-2, P1-3, P1-4, P2-5, P2-6, P2-7 in `bindings/python/src/lib.rs`

**3A — Pydantic `model_dump()` guard (P0-1, P0-2, P1-3):**

Three sites call `json.dumps(data)` on unconstrained `PyAny` without first trying `model_dump()`. Fix:

```rust
let serializable = match data.call_method0("model_dump") {
    Ok(dict) => dict,
    Err(_) => data,  // not a Pydantic model, use as-is
};
let json_str = json_mod.call_method1("dumps", (serializable,))?;
```

The `?` on `dumps` still propagates any real serialization error as a `PyErr`. We're not silencing anything — just handling Pydantic objects correctly.

**3B — Config parse error propagation (P1-4, line 1410):**

- Current: `serde_json::from_str(&json_str).unwrap_or_default()` — silently produces empty `HashMap` when valid JSON isn't a JSON object
- Fix: `unwrap_or_else` + `log::warn!()` before returning default

**3C — Serialization fallback logging (P2-5, P2-6, P2-7):**

- Current: three sites use `serde_json::to_string(...).unwrap_or_else(|_| "{}".to_string())`
- Fix: change `|_|` to `|e|` and add `log::warn!()` before returning fallback
- No behavioral change — still falls back to `"{}"`, but now it's visible

### 4. Silent Error Fix — gRPC Bridge Layer

**Scope:** P1-11 through P2-18 (8 sites across 6 files)

All use `.ok()`, `.unwrap_or_default()`, or `unwrap_or_else(|_| ...)` silently dropping parse/serialization errors at gRPC wire boundaries. Fix: add `log::warn!()` before every silent fallback.

| Finding | File | Current | Fix |
|---------|------|---------|-----|
| P1-11 | `grpc_hook.rs:64` | `.ok()` on hook result data | `map_err(\|e\| log::warn!(...)).ok()` |
| P1-12 | `grpc_tool.rs:126` | `.ok()` on tool output | Same pattern |
| P1-13 | `grpc_tool.rs:62` | `.unwrap_or_default()` on tool schema | `unwrap_or_else(\|e\| { log::warn!(...); default })` |
| P1-14 | `grpc_tool.rs:121-127` | `content_type` ignored | Log warning if present and not `application/json`; don't change parsing behavior |
| P2-15 | `grpc_provider.rs:61,114` | `.unwrap_or_default()` on provider defaults (2 sites) | `unwrap_or_else` + `log::warn!()` |
| P2-16 | `grpc_context.rs:57` | `.unwrap_or_default()` on message serialization | `unwrap_or_else` + `log::warn!()` |
| P2-17 | `conversions.rs:13-38` | Silent loss cycle both directions | `unwrap_or_else` + `log::warn!()` on serialization; `map_err` + `log::warn!()` + `.ok()` on deserialization |
| P2-18 | `grpc_server.rs:73-80` | `.unwrap_or_default()` on server-side tool result | `unwrap_or_else` + `log::warn!()` |

All `log::warn!()` — data integrity issues at wire boundaries. No behavioral changes.

### 5. Enum & Type Safety — gRPC Wire Boundaries

**E-1 — Raw integer enum matching in `grpc_hook.rs` (4 fields):**

- Current: `match proto.action { 1 => Continue, 2 => Modify, ... _ => Continue }` with raw integers
- Fix: match on generated proto enum variants (e.g., `amplifier_module::hook_result::Action::Continue`); add `log::warn!("Unknown hook action variant {}, defaulting to Continue", proto.action)` for the unknown/default arm
- Same treatment for all 4 fields: `action`, `context_injection_role`, `approval_default`, `user_message_level`

**E-2 — `None` timeout conflated with `0.0` in `grpc_approval.rs`:**

- Current: `request.timeout.unwrap_or(0.0)` makes "no timeout" and "expire immediately" indistinguishable on the wire
- Fix: use the proto's `optional float` if available, or use `-1.0` sentinel for "no timeout" with `log::warn!()` for unexpected values
- Requires checking proto definition to determine correct approach

**E-3 — Unchecked `i64 → i32` truncation in `conversions.rs`:**

- Current: `context_window: native.context_window as i32` silently wraps on overflow
- Fix: `i32::try_from(native.context_window).unwrap_or_else(|_| { log::warn!(...); i32::MAX })`
- Clamps to `i32::MAX` instead of wrapping. Real fix requires updating proto to `int64` (separate protocol change)

**E-4 — `0` sentinel for optional token counts in `conversions.rs`:**

- Current: `Some(0)` (provider reported zero tokens) round-trips to `None` (provider didn't report)
- Proto design limitation — fields aren't `optional` in the proto
- Fix: `log::debug!()` noting ambiguity + code comment documenting limitation
- Real fix requires proto schema change to `optional int32` (out of scope)

### 6. Structural Gaps (Document Only)

These are acknowledged incomplete gRPC protocol implementations, not bugs with silent fallbacks. They require proto schema changes and are Phase 2/4 work items per the cross-language SDK roadmap. **Document, don't fix.**

> **✅ RESOLVED (2026-03-05):** All S-1 through S-4 structural gaps were fully fixed in the gRPC Phase 2 debt fix work
> (`docs/plans/2026-03-04-grpc-v2-debt-fix-design.md`). All `TODO(grpc-v2)` markers have been removed from source code.
> The table below reflects the original action taken; actual fixes are described in the debt fix design and implementation docs.

| Finding | File | Gap | Resolution |
|---------|------|-----|------------|
| S-1 | `grpc_context.rs` | Message fields (role, name, tool_call_id, metadata) zeroed | Fixed: full bidirectional conversion implemented |
| S-2 | `grpc_context.rs` | BlockContent variants → Null | Fixed: all BlockContent variants converted |
| S-3 | `grpc_context.rs` | `provider_name` not transmitted | Fixed: provider_name transmitted via proto field |
| S-4 | `grpc_orchestrator.rs` | 5 orchestrator parameters discarded | Fixed: remote orchestrators access these via KernelService RPCs using session_id |

**Log level: `debug`**, not `warn`. These are known limitations, not unexpected failures. An operator running at `debug` level sees them; normal operation stays quiet.

**Why not just comments?** Because `log::debug!()` makes them visible at runtime to anyone investigating gRPC bridge behavior, not just to developers reading source code. Comments only help if you already know where to look.

## Data Flow

### Log message routing (Python context)

```
Rust code: log::warn!("Contributor {} failed: {}", name, e)
  → log crate dispatches to registered logger
  → pyo3-log receives it
  → maps module_path!() "amplifier_core::coordinator" → "amplifier_core.coordinator"
  → calls Python logging.getLogger("amplifier_core.coordinator").warning(...)
  → Python logging infrastructure handles it (console, file, etc.)
```

### Log message routing (non-Python context)

```
Rust code: log::warn!("Contributor {} failed: {}", name, e)
  → log crate dispatches to registered logger
  → no logger registered → no-op (silent, zero cost)
  OR
  → app registered env_logger → prints to stderr
  → app registered tracing-log → routes to tracing subscriber
```

## Error Handling

This design **is** the error handling fix. The current state has 30+ sites where errors are handled (fallback/continue/default) but invisible. After this change:

- **P0 sites** (data corruption risk): `log::warn!()` or `log::error!()` — these should be seen in normal operation
- **P1 sites** (data loss at boundaries): `log::warn!()` — important for debugging integration issues
- **P2 sites** (minor data loss): `log::warn!()` — useful but not urgent
- **E sites** (type safety): `log::warn!()` for unexpected values, `log::debug!()` for known limitations
- **S sites** (structural gaps): `log::debug!()` — known limitations, not errors

No new error paths are introduced. No existing fallback behavior changes.

## Testing Strategy

- **Rust unit tests:** Verify `log::warn!()` / `log::error!()` / `log::debug!()` is emitted at each fixed site using the `log` crate's test capture capabilities (or a custom test subscriber)
- **Existing test suite:** Must continue to pass with no changes — this PR adds visibility only, no behavioral changes
- **Python integration tests:** Verify that Rust log output appears in Python's `logging` when `pyo3-log` is initialized
- **Migration verification:** The 8 existing manual `py.import("logging")` / `getLogger()` sites should be converted and tested to ensure identical logger names
- **Pydantic guard tests:** Verify `model_dump()` is called on Pydantic models and raw dicts pass through unchanged

## Open Questions

- **E-2 proto definition:** Need to check the proto definition for `ApprovalRequest.timeout` — is it already `optional float` or bare `float`? This determines the fix approach.
- **GitHub issue tracking:** Should the `// TODO(grpc-v2)` structural gaps (S-1 through S-4) be tracked in GitHub issues for Phase 2 of the cross-language SDK roadmap?
