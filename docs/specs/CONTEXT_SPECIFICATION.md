# Context & Orchestrator Specification

**Purpose**: Specification for context managers and orchestrator modules - message handling and turn execution.

---

## Context Manager Contracts

### Core Principle: Preserve Fidelity

**Store what you receive**. Context managers MUST preserve messages exactly as received - no flattening, no stripping, no modification.

### Message Storage Rules

**1. Preserve Content Blocks**

Store message content as block lists, not flattened text:

```python
# Correct
{
  "role": "assistant",
  "content": [
    {"type": "text", "text": "Let me search for that."},
    {"type": "tool_call", "id": "call_1", "name": "search", "input": {...}}
  ]
}

# Wrong - flattening loses structure
{
  "role": "assistant",
  "content": "Let me search for that."
  # tool_call information lost!
}
```

**2. Preserve Thinking Continuity (Anthropic)**

When tools are used, preserve and re-send the original `thinking` block with `signature`:

```python
# Message N (assistant with tool call)
{
  "role": "assistant",
  "content": [
    {"type": "thinking", "thinking": "...", "signature": "sig_abc"},
    {"type": "tool_call", ...}
  ]
}

# Message N+1 (tool result)
{
  "role": "function",
  "content": [{"type": "tool_result", ...}]
}

# Message N+2 (assistant continues) - MUST include thinking with signature
{
  "role": "assistant",
  "content": [
    {"type": "thinking", "thinking": "...", "signature": "sig_abc"},  # REQUIRED
    {"type": "text", "text": "Based on search results..."}
  ]
}
```

**Why**: Anthropic requires thinking blocks with signatures for conversation continuity.

**3. Tool Pair Atomicity**

Tool_use and tool_result messages are atomic pairs that MUST be preserved together during compaction:

```python
# If compaction keeps this message:
{
  "role": "assistant",
  "content": [{"type": "tool_call", "id": "call_1", ...}]
}

# It MUST keep the immediately following message:
{
  "role": "function",
  "tool_call_id": "call_1",
  "content": [{"type": "tool_result", ...}]
}
```

**Why**: Preserves conversation state integrity. Providers (especially Anthropic) require every tool_use in message N to have corresponding tool_result in message N+1.

**Compaction strategy**: When removing messages, check for tool_use blocks. If found, keep both the tool_use message and its paired tool_result message (or remove both).

### Context Protocol

```python
class ContextManager(Protocol):
    async def add_message(self, message: Message) -> None:
        """Add message to context."""

    async def get_messages(self) -> List[Message]:
        """Get all messages in context."""

    async def compact(self) -> CompactionResult:
        """Reduce context size while preserving critical messages."""

    async def get_token_count(self) -> int:
        """Get approximate token count."""
```

### Compaction Guidelines

**What to preserve** (in priority order):
1. System messages (always keep)
2. Recent messages (last 3-5 turns)
3. Tool pairs (atomic, keep together or remove together)
4. Messages with critical context

**What can be summarized**:
- Middle conversation history (beyond recent window)
- Older user questions and assistant responses
- Non-tool-related exchanges

**Compaction events**:
```python
# Before compaction
self.emit("context:pre_compact", {
    "before_tokens": 150000,
    "before_messages": 50,
    "strategy": "summarize_middle"
})

# After compaction
self.emit("context:post_compact", {
    "after_tokens": 100000,
    "after_messages": 35,
    "removed_tokens": 50000,
    "removed_messages": 15
})
```

---

## Orchestrator Contracts

### Turn & Iteration Semantics

**Turn**: One complete request/response cycle
- Begins when app hands `ChatRequest` with new `turn_id`
- Ends at `message.end` event (or terminal `error`)

**Iteration**: One provider/tool cycle within turn
- Increment `iteration` counter for each cycle
- Used in planning loops, tool execution loops

**Span**: Nested operation within turn
- Provider call: new `span_id` (parent = turn root or surrounding span)
- Tool call: new `span_id` (parent = provider span)
- Planning step: new `span_id`

### Orchestrator Protocol

```python
class OrchestratorModule(Protocol):
    async def execute(
        self,
        prompt: str,
        context: ContextManager,
        providers: Dict[str, ProviderModule],
        tools: Dict[str, ToolModule],
        hooks: List[HookModule]
    ) -> Response:
        """Execute turn with given context and capabilities."""
```

### Event Emission Requirements

Orchestrators MUST emit:

**Turn lifecycle**:
- `prompt:submit` - When turn starts
- `prompt:complete` - When turn ends

**Planning** (if applicable):
- `plan:start` - Planning phase begins
- `plan:end` - Planning phase complete

**Provider calls**:
- `provider:request` - Before API call
- `provider:response` - After success
- `provider:error` - On failure

**Tool execution**:
- `tool:pre` - Before tool execution
- `tool:post` - After success
- `tool:error` - On failure

**Context operations**:
- `context:pre_compact` - Before compaction
- `context:post_compact` - After compaction

**All events include**:
- `session_id` - Session identifier
- `turn_id` - Turn identifier
- `span_id` - Operation span (where applicable)
- `parent_span_id` - Parent span (where applicable)
- `iteration` - Iteration number (where applicable)

### Orchestrator Types

**basic** (loop-basic):
- Simple request/response
- No planning, no multi-turn loops
- Single provider call per turn

**streaming** (loop-streaming):
- Real-time response streaming
- SSE event emission
- Single provider call with streaming

**events** (loop-events):
- Event-driven orchestration
- Multi-turn conversations
- Tool execution loops
- Planning capabilities

**Custom orchestrators** (your own):
- Implement `OrchestratorModule` protocol
- Emit required events
- Respect context preservation rules

---

## Visibility Policy (App/UI Layer)

**Mechanism**: Kernel preserves all content (thinking, reasoning, etc.)

**Policy**: Apps decide what to display to users

```python
# Content blocks may have visibility hints
{
  "type": "thinking",
  "thinking": "...",
  "visibility": "internal"  # or "developer", "user"
}
```

**Not kernel responsibility**: Whether to show reasoning/thinking to end users

**App layer decides**:
- Internal tools: Show all content
- Developer UIs: Show thinking/reasoning with toggle
- End user UIs: Show only user-facing content

---

## Implementation Checklist

### Context Managers

- [ ] Preserve messages exactly (no flattening)
- [ ] Preserve content blocks (store as arrays)
- [ ] Preserve thinking blocks with signatures
- [ ] Keep tool pairs atomic during compaction
- [ ] Emit compaction events
- [ ] Provide token count estimates

### Orchestrators

- [ ] Implement `OrchestratorModule` protocol
- [ ] Generate turn_id for each request
- [ ] Generate span_id for nested operations
- [ ] Increment iteration for loops
- [ ] Emit all required events
- [ ] Propagate IDs to providers/tools
- [ ] Handle tool execution loops correctly

---

## Reference Implementations

**Context Managers**:
- `amplifier-module-context-simple` - In-memory, no persistence
- `amplifier-module-context-persistent` - File-based persistence

**Orchestrators**:
- `amplifier-module-loop-basic` - Simple request/response
- `amplifier-module-loop-streaming` - Real-time streaming
- `amplifier-module-loop-events` - Event-driven with tools

---

## Summary

**Context managers**: Preserve message fidelity, handle compaction, emit events

**Key rules**:
- Store messages exactly as received
- Preserve content blocks (no flattening)
- Keep tool pairs atomic
- Preserve thinking with signatures

**Orchestrators**: Execute turns, emit events, manage IDs

**Key rules**:
- Generate turn_id, span_id correctly
- Emit all required lifecycle events
- Propagate IDs to child operations
- Respect context preservation

**Philosophy**: Kernel provides mechanisms (protocols, events). Context/orchestrator modules implement policies (compaction strategy, execution flow).
