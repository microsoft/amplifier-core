# Observability Specification

**Purpose**: Complete specification for Amplifier observability - events, tracing IDs, logging schema, and hook contracts.

**Scope**: Defines what to emit, when to emit, what IDs to include, and how hooks observe.

---

## Overview

Amplifier observability follows event-first principles:

**Key concepts**:
- **Canonical events**: Standard event names for all important actions
- **Tracing IDs**: Session, turn, span IDs for flow reconstruction
- **JSONL schema**: Single unified log stream
- **Hooks**: Observe events without blocking

**Philosophy**: Kernel emits mechanisms (events); hooks decide policies (what to log, where to send).

---

## Canonical Event Taxonomy

### Event Naming Convention

`namespace:action` format

**Example**: `provider:request`, `tool:execute`, `session:start`

### Core Events

**Session lifecycle**:
- `session:start` - Session created
- `session:end` - Session ended

**Prompt handling**:
- `prompt:submit` - User/system prompt submitted
- `prompt:complete` - Response completed

**Planning** (if orchestrator supports):
- `plan:start` - Planning phase started
- `plan:end` - Planning completed

**Provider calls**:
- `provider:request` - Before LLM API call
- `provider:response` - After successful response
- `provider:error` - On API failure

**Tool execution**:
- `tool:pre` - Before tool execution
- `tool:post` - After successful execution
- `tool:error` - On tool failure

**Context management**:
- `context:pre_compact` - Before compaction
- `context:post_compact` - After compaction

**Artifacts**:
- `artifact:write` - File written
- `artifact:read` - File read

**Policy enforcement**:
- `policy:violation` - Attempted action blocked
- `approval:required` - User approval needed
- `approval:granted` - Approval given
- `approval:denied` - Approval denied

### Event Payloads

**provider:request**:
```python
{
  "event": "provider:request",
  "provider": "anthropic",
  "model": "claude-sonnet-4-5",
  "session_id": "...",
  "turn_id": "...",
  "span_id": "...",
  "parent_span_id": "...",
  "capabilities": {...},
  "response_format": {...},
  "tools": [...],
  # Optional (policy-controlled):
  "payload": {"request": {...}}  # Full native request
}
```

**provider:response**:
```python
{
  "event": "provider:response",
  "latency_ms": 1234,
  "usage": {
    "input_tokens": 100,
    "output_tokens": 50,
    "total_tokens": 150
  },
  "item_id": "msg_123",
  "degradations": [],
  "status": "ok",
  # Optional (policy-controlled):
  "payload": {"response": {...}}  # Full native response
}
```

**provider:error**:
```python
{
  "event": "provider:error",
  "kind": "rate_limit",  # or "transport", "invalid_request", "capability"
  "status": 429,
  "message": "Rate limit exceeded",
  "retry_after_ms": 60000,
  "raw_excerpt": "...",
  "status": "error"
}
```

**tool:pre/post/error**:
```python
{
  "event": "tool:pre",
  "tool": "filesystem",
  "operation": "write_file",
  "span_id": "...",
  "parent_span_id": "..."
}
```

**context:pre_compact/post_compact**:
```python
{
  "event": "context:pre_compact",
  "before_tokens": 150000,
  "before_messages": 50,
  "strategy": "summarize_middle"
}

{
  "event": "context:post_compact",
  "after_tokens": 100000,
  "after_messages": 35,
  "removed_tokens": 50000,
  "removed_messages": 15
}
```

---

## Tracing IDs

### Purpose

Enable flow reconstruction, distributed tracing, and precise debugging.

### ID Types

**session_id** (UUID):
- Generated at session creation
- Stable across all turns in session
- Represents long-running conversation or job (hours/days)

**turn_id** (UUID):
- New for each inbound request
- Represents one complete request/response cycle
- Groups all spans for that turn

**span_id** (UUID):
- New for each nested operation
- Provider call, tool execution, planning step, compaction
- Forms tree under turn_id via parent_span_id

**parent_span_id** (UUID, optional):
- Links span to parent span
- Null for top-level turn span
- Enables tree reconstruction

**iteration** (int, optional):
- Counter within a turn
- Relevant for planning loops, retries
- Monotonic increment per turn

**seq** (int):
- Monotonic sequence number per session
- Enables total ordering of all events
- Never resets during session lifetime

### ID Usage

**All events include**:
```python
{
  "ts": "2025-10-31T12:34:56.789Z",
  "session_id": "sess_abc123",
  "turn_id": "turn_xyz789",
  "span_id": "span_def456",  # Optional for session-level events
  "parent_span_id": "span_abc123",  # Optional
  "seq": 42
}
```

**Streaming events include**:
```python
{
  "ts": "...",
  "session_id": "...",
  "turn_id": "...",
  "span_id": "...",
  "parent_span_id": "...",
  "seq": 42,
  "visibility": "developer"  # Optional: "internal", "developer", "user"
}
```

### ID Generation Rules

1. Generate `session_id` once at session start
2. Generate new `turn_id` for each inbound request
3. Generate new `span_id` for each nested operation
4. Set `parent_span_id` to current span when creating child span
5. Increment `seq` monotonically for each event in session
6. Propagate all IDs to child operations

---

## JSONL Log Schema

### Schema Version

```python
{
  "schema": {
    "name": "amplifier.log",
    "ver": "1.0.0"
  }
}
```

### Log Entry Format

```python
{
  # Timestamp
  "ts": "2025-10-31T12:34:56.789Z",

  # Level
  "lvl": "info",  # or "warn", "error"

  # Schema
  "schema": {"name": "amplifier.log", "ver": "1.0.0"},

  # Tracing IDs
  "session_id": "sess_abc123",
  "request_id": "turn_xyz789",  # Same as turn_id
  "span_id": "span_def456",
  "parent_span_id": "span_abc123",

  # Event
  "event": "provider:request",

  # Component
  "component": "orchestrator",  # Who emitted this
  "module": "provider-anthropic",  # Which module (if applicable)

  # Message (optional)
  "message": "Calling Claude Sonnet 4.5",

  # Status (optional)
  "status": "success",  # or "error"

  # Duration (optional)
  "duration_ms": 1234,

  # Redaction (optional)
  "redaction": {
    "applied": true,
    "fields": ["payload.request.messages[2].content"]
  },

  # Data (optional)
  "data": {
    # Event-specific data
  },

  # Error (optional)
  "error": {
    "type": "ProviderRateLimitError",
    "message": "Rate limit exceeded",
    "stack": "..."
  }
}
```

### Critical Principles

**Single stream**:
- All observability → one JSONL file
- No multiple log files
- No stdout/stderr logging from modules

**Redaction before logging**:
- Apply redaction BEFORE writing to log
- Mark `redaction.applied: true`
- List `redaction.fields` for transparency
- Never log then redact

**Policy-controlled capture**:
- Kernel emits events (mechanism)
- Hooks decide what to capture (policy)
- Full payloads vs summaries vs metadata = policy decision

---

## Hook Contracts

### Hook Protocol

```python
class HookModule(Protocol):
    async def on_event(
        self,
        event: str,
        data: Dict[str, Any],
        context: Dict[str, Any]
    ) -> None:
        """Observe event, never block."""
```

### Hook Principles

**Non-blocking**:
- Hooks MUST NOT delay primary flow
- Errors in hooks don't propagate to kernel
- Catch own exceptions, log failures

**Non-interference**:
- Hooks observe, never modify
- No side effects on kernel state
- Failures contained to hook itself

**Policy implementation**:
- Logging destinations (file, network, database)
- Redaction rules (PII detection, masking)
- Approval gates (human-in-loop)
- Metrics collection (counters, gauges)

### Hook Registration

```python
# Register hook for specific events
kernel.register_hook("provider:*", logging_hook, priority=10)
kernel.register_hook("tool:*", approval_hook, priority=5)
kernel.register_hook("*", metrics_hook, priority=0)
```

**Priority**: Lower number = higher priority (runs first)

### Example: Logging Hook

```python
class LoggingHook:
    def __init__(self, log_file: Path, redaction_rules: List[str]):
        self.log_file = log_file
        self.redaction_rules = redaction_rules

    async def on_event(self, event: str, data: Dict, context: Dict):
        try:
            # Apply redaction
            redacted_data, redaction_info = self.redact(data)

            # Build log entry
            log_entry = {
                "ts": datetime.utcnow().isoformat() + "Z",
                "lvl": "info",
                "schema": {"name": "amplifier.log", "ver": "1.0.0"},
                "session_id": context.get("session_id"),
                "turn_id": context.get("turn_id"),
                "span_id": context.get("span_id"),
                "event": event,
                "data": redacted_data,
                "redaction": redaction_info
            }

            # Write to JSONL (append)
            with open(self.log_file, "a") as f:
                f.write(json.dumps(log_entry) + "\n")

        except Exception as e:
            # Hook errors don't propagate
            print(f"Logging hook error: {e}", file=sys.stderr)
```

---

## Implementation Checklist

### For Kernel/Orchestrators

- [ ] Emit canonical events at appropriate times
- [ ] Include all required tracing IDs in events
- [ ] Generate span_id for nested operations
- [ ] Propagate IDs to child operations
- [ ] Increment seq monotonically

### For Providers

- [ ] Emit `provider:request` before API call
- [ ] Emit `provider:response` after success
- [ ] Emit `provider:error` on failure
- [ ] Include usage, degradations in response event
- [ ] Include tracing IDs from orchestrator

### For Tools

- [ ] Emit `tool:pre` before execution
- [ ] Emit `tool:post` after success
- [ ] Emit `tool:error` on failure
- [ ] Include span_id and parent_span_id
- [ ] Emit `artifact:*` for file operations

### For Hooks

- [ ] Implement non-blocking event handling
- [ ] Catch and contain own exceptions
- [ ] Apply redaction before logging (if logging hook)
- [ ] Use unified JSONL format (if logging hook)
- [ ] Register for appropriate event patterns

---

## Reference Implementations

**Logging Hook**: See `amplifier-module-hooks-logging`
**Redaction Hook**: See `amplifier-module-hooks-redaction`
**Approval Hook**: See `amplifier-module-hooks-approval`

---

## Summary

**Observability in Amplifier**:
- Event-first: Important actions emit canonical events
- Tracing IDs: Reconstruct flows with session/turn/span IDs
- Single stream: One JSONL log for all observability
- Hook-based: Policies in hooks, mechanisms in kernel
- Non-blocking: Hooks never delay primary flow
- Redaction first: Apply before logging, never after

**Key contracts**:
- Canonical event names (defined in `amplifier_core/events.py`)
- Event payloads (data each event includes)
- Tracing IDs (session_id, turn_id, span_id, seq)
- JSONL schema (unified log format)
- Hook protocol (how to observe)

**Philosophy**:
- Kernel emits (mechanism)
- Hooks observe and decide (policy)
- Text-first, inspectable
- Non-interference always
