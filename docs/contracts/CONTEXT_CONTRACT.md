---
contract_type: module_specification
module_type: context
contract_version: 2.1.0
last_modified: 2026-01-01
related_files:
  - path: amplifier_core/interfaces.py#ContextManager
    relationship: protocol_definition
    lines: 148-180
  - path: ../specs/MOUNT_PLAN_SPECIFICATION.md
    relationship: configuration
  - path: ../specs/CONTRIBUTION_CHANNELS.md
    relationship: observability
  - path: amplifier_core/testing.py#MockContextManager
    relationship: test_utilities
canonical_example: https://github.com/microsoft/amplifier-module-context-simple
---

# Context Contract

Context managers handle conversation memory and message storage.

---

## Purpose

Context managers control **what the agent remembers**:
- **Message storage** - Store conversation history
- **Request preparation** - Return messages that fit within token limits
- **Persistence** - Optionally persist across sessions
- **Memory strategies** - Implement various memory patterns

**Key principle**: The context manager owns **policy** for memory. The orchestrator asks for messages; the context manager decides **how** to fit them within limits. Swap context managers to change memory behavior without modifying orchestrators.

**Mechanism vs Policy**: Orchestrators provide the mechanism (request messages, make LLM calls). Context managers provide the policy (what to return, when to compact, how to fit within limits).

---

## Protocol Definition

**Source**: `amplifier_core/interfaces.py` lines 148-180

```python
@runtime_checkable
class ContextManager(Protocol):
    async def add_message(self, message: dict[str, Any]) -> None:
        """Add a message to the context."""
        ...

    async def get_messages_for_request(
        self,
        token_budget: int | None = None,
        provider: Any | None = None,
    ) -> list[dict[str, Any]]:
        """
        Get messages ready for an LLM request.

        The context manager handles any compaction needed internally.
        Returns messages that fit within the token budget.

        Args:
            token_budget: Optional explicit token limit (deprecated, prefer provider).
            provider: Optional provider instance for dynamic budget calculation.
                If provided, budget = context_window - max_output_tokens - safety_margin.

        Returns:
            Messages ready for LLM request, compacted if necessary.
        """
        ...

    async def get_messages(self) -> list[dict[str, Any]]:
        """Get all messages (raw, uncompacted) for transcripts/debugging."""
        ...

    async def set_messages(self, messages: list[dict[str, Any]]) -> None:
        """Set messages directly (for session resume)."""
        ...

    async def clear(self) -> None:
        """Clear all messages."""
        ...
```

---

## Message Format

Messages follow a standard structure:

```python
# User message
{
    "role": "user",
    "content": "User's input text"
}

# Assistant message
{
    "role": "assistant",
    "content": "Assistant's response"
}

# Assistant message with tool calls
{
    "role": "assistant",
    "content": None,
    "tool_calls": [
        {
            "id": "call_123",
            "type": "function",
            "function": {"name": "read_file", "arguments": "{...}"}
        }
    ]
}

# System message
{
    "role": "system",
    "content": "System instructions"
}

# Tool result
{
    "role": "tool",
    "tool_call_id": "call_123",
    "content": "Tool output"
}
```

---

## Entry Point Pattern

### mount() Function

```python
async def mount(coordinator: ModuleCoordinator, config: dict) -> ContextManager | Callable | None:
    """
    Initialize and return context manager instance.

    Returns:
        - ContextManager instance
        - Cleanup callable
        - None for graceful degradation
    """
    context = MyContextManager(
        max_tokens=config.get("max_tokens", 100000),
        compaction_threshold=config.get("compaction_threshold", 0.8)
    )
    await coordinator.mount("session", context, name="context")
    return context
```

### pyproject.toml

```toml
[project.entry-points."amplifier.modules"]
my-context = "my_context:mount"
```

---

## Implementation Requirements

### add_message()

Store messages with proper validation:

```python
async def add_message(self, message: dict[str, Any]) -> None:
    """Add a message to the context."""
    # Validate required fields
    if "role" not in message:
        raise ValueError("Message must have 'role' field")

    # Store message
    self._messages.append(message)

    # Track token count (approximate)
    self._token_count += self._estimate_tokens(message)
```

### get_messages_for_request()

Return messages ready for LLM request, handling compaction internally:

```python
async def get_messages_for_request(
    self,
    token_budget: int | None = None,
    provider: Any | None = None,
) -> list[dict[str, Any]]:
    """
    Get messages ready for an LLM request.

    Handles compaction internally if needed. Orchestrators call this
    before every LLM request and trust the context manager to return
    messages that fit within limits.

    Args:
        token_budget: Optional explicit token limit (deprecated, prefer provider).
        provider: Optional provider instance for dynamic budget calculation.
            If provided, budget = context_window - max_output_tokens - safety_margin.
    """
    budget = self._calculate_budget(token_budget, provider)

    # Check if compaction needed
    if self._token_count > (budget * self._compaction_threshold):
        await self._compact_internal()

    return list(self._messages)  # Return copy to prevent mutation

def _calculate_budget(self, token_budget: int | None, provider: Any | None) -> int:
    """Calculate effective token budget from provider or fallback to config."""
    # Explicit budget takes precedence (for backward compatibility)
    if token_budget is not None:
        return token_budget

    # Try provider-based dynamic budget
    if provider is not None:
        try:
            info = provider.get_info()
            defaults = info.defaults or {}
            context_window = defaults.get("context_window")
            max_output_tokens = defaults.get("max_output_tokens")

            if context_window and max_output_tokens:
                safety_margin = 1000  # Buffer to avoid hitting hard limits
                return context_window - max_output_tokens - safety_margin
        except Exception:
            pass  # Fall back to configured max_tokens

    return self._max_tokens
```

### get_messages()

Return all messages for transcripts/debugging (no compaction):

```python
async def get_messages(self) -> list[dict[str, Any]]:
    """Get all messages (raw, uncompacted) for transcripts/debugging."""
    return list(self._messages)  # Return copy to prevent mutation
```

### set_messages()

Set messages directly for session resume:

```python
async def set_messages(self, messages: list[dict[str, Any]]) -> None:
    """Set messages directly (for session resume)."""
    self._messages = list(messages)
    self._token_count = sum(self._estimate_tokens(m) for m in self._messages)
```

**File-Based Context Managers - Special Behavior**:

For context managers with persistent file storage (like `context-persistent`), the behavior on session resume is different:

```python
async def set_messages(self, messages: list[dict[str, Any]]) -> None:
    """
    Set messages - behavior depends on whether we loaded from file.
    
    If we already loaded from our own file (session resume):
      - IGNORE this call to preserve our complete history
      - CLI's filtered transcript would lose system/developer messages
    
    If this is a fresh session or migration:
      - Accept the messages and write to our file
    """
    if self._loaded_from_file:
        # Already have complete history - ignore CLI's filtered transcript
        logger.info("Ignoring set_messages - loaded from persistent file")
        return
    
    # Fresh session: accept messages
    self._messages = list(messages)
    self._write_to_file()
```

**Why This Pattern?**:
- CLI's `SessionStore` saves a **filtered** transcript (no system/developer messages)
- File-based context managers save the **complete** history
- On resume, the context manager's file is authoritative
- Prevents loss of system context during session resume

### clear()

Reset context state:

```python
async def clear(self) -> None:
    """Clear all messages."""
    self._messages = []
    self._token_count = 0
```

---

## Internal Compaction

Compaction is an **internal implementation detail** of the context manager. It happens automatically when `get_messages_for_request()` is called and the context exceeds thresholds.

### Non-Destructive Compaction (REQUIRED)

**Critical Design Principle**: Compaction MUST be **ephemeral** - it returns a compacted VIEW without modifying the stored history.

```
┌─────────────────────────────────────────────────────────────────┐
│                    NON-DESTRUCTIVE COMPACTION                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  messages[]                    get_messages_for_request()       │
│  ┌──────────┐                  ┌──────────┐                     │
│  │ msg 1    │                  │ msg 1    │  (compacted view)   │
│  │ msg 2    │   ──────────▶    │ [summ]   │                     │
│  │ msg 3    │   ephemeral      │ msg N    │                     │
│  │ ...      │   compaction     └──────────┘                     │
│  │ msg N    │                                                   │
│  └──────────┘                  get_messages()                   │
│       │                        ┌──────────┐                     │
│       │                        │ msg 1    │  (FULL history)     │
│       └───────────────────▶    │ msg 2    │                     │
│         unchanged              │ msg 3    │                     │
│                                │ ...      │                     │
│                                │ msg N    │                     │
│                                └──────────┘                     │
│                                                                 │
│  Key: Internal state is NEVER modified by compaction.           │
│       Compaction produces temporary views for LLM requests.     │
│       Full history is always available via get_messages().      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Why Non-Destructive?**:
- **Transcript integrity**: Full conversation history is preserved for replay/debugging
- **Session resume**: Can resume from any point with complete context
- **Reproducibility**: Same inputs produce same outputs
- **Observability**: Hook systems can observe the full conversation

**Implementation Pattern**:
```python
async def get_messages_for_request(self, token_budget=None, provider=None):
    """Return compacted VIEW without modifying internal state."""
    budget = self._calculate_budget(token_budget, provider)
    
    # Read current messages (don't modify)
    messages = list(self._messages)  # Copy!
    
    # Check if compaction needed
    token_count = self._count_tokens(messages)
    if not self._should_compact(token_count, budget):
        return messages
    
    # Compact EPHEMERALLY - return compacted copy
    return self._compact_messages(messages, budget)  # Returns NEW list

async def get_messages(self):
    """Return FULL history (never compacted)."""
    return list(self._messages)  # Always complete
```

### Tool Pair Preservation

**Critical**: During compaction, tool_use and tool_result messages must be kept together. Separating them causes LLM API errors.

```python
async def _compact_internal(self) -> None:
    """Internal compaction - preserves tool pairs."""
    # Emit pre-compaction event
    await self._hooks.emit("context:pre_compact", {
        "message_count": len(self._messages),
        "token_count": self._token_count
    })

    # Build tool_call_id -> tool_use index map
    tool_use_ids = set()
    for msg in self._messages:
        if msg.get("role") == "assistant" and msg.get("tool_calls"):
            for tc in msg["tool_calls"]:
                tool_use_ids.add(tc.get("id"))

    # Identify which tool results have matching tool_use
    orphan_result_indices = []
    for i, msg in enumerate(self._messages):
        if msg.get("role") == "tool":
            if msg.get("tool_call_id") not in tool_use_ids:
                orphan_result_indices.append(i)

    # Strategy: Keep system messages + recent messages
    # But ensure we don't split tool pairs
    system_messages = [m for m in self._messages if m["role"] == "system"]

    # Find safe truncation point (not in middle of tool sequence)
    keep_count = self._keep_recent
    recent_start = max(0, len(self._messages) - keep_count)

    # Adjust start to not split tool sequences
    while recent_start > 0:
        msg = self._messages[recent_start]
        if msg.get("role") == "tool":
            # This is a tool result - need to include the tool_use before it
            recent_start -= 1
        else:
            break

    recent_messages = self._messages[recent_start:]

    self._messages = system_messages + recent_messages
    self._token_count = sum(self._estimate_tokens(m) for m in self._messages)

    # Emit post-compaction event
    await self._hooks.emit("context:post_compact", {
        "message_count": len(self._messages),
        "token_count": self._token_count
    })
```

### Compaction Strategies

Different strategies for different use cases:

#### Simple Truncation

Keep N most recent messages (with tool pair preservation):

```python
# Find safe truncation point
keep_from = len(self._messages) - keep_count
# Adjust to not split tool pairs
while keep_from > 0 and self._messages[keep_from].get("role") == "tool":
    keep_from -= 1
self._messages = self._messages[keep_from:]
```

#### Summarization

Use LLM to summarize older messages:

```python
# Summarize old messages
old_messages = self._messages[:-keep_recent]
summary = await summarize(old_messages)

# Replace with summary
self._messages = [
    {"role": "system", "content": f"Previous conversation summary: {summary}"},
    *self._messages[-keep_recent:]
]
```

#### Importance-Based

Keep messages based on importance score:

```python
scored = [(m, self._score_importance(m)) for m in self._messages]
scored.sort(key=lambda x: x[1], reverse=True)
# Keep high-importance messages, but preserve tool pairs
self._messages = self._reorder_preserving_tool_pairs(
    [m for m, _ in scored[:keep_count]]
)
```

---

## Configuration

Context managers receive configuration via Mount Plan:

```yaml
session:
  orchestrator: loop-basic
  context: my-context

# Context config can be passed via top-level config
```

See [MOUNT_PLAN_SPECIFICATION.md](../specs/MOUNT_PLAN_SPECIFICATION.md) for full schema.

---

## Observability

Register compaction events:

```python
coordinator.register_contributor(
    "observability.events",
    "my-context",
    lambda: ["context:pre_compact", "context:post_compact"]
)
```

Standard events to emit:
- `context:pre_compact` - Before compaction (include message_count, token_count)
- `context:post_compact` - After compaction (include new counts)

See [CONTRIBUTION_CHANNELS.md](../specs/CONTRIBUTION_CHANNELS.md) for the pattern.

---

## Canonical Example

**Reference implementation**: [amplifier-module-context-simple](https://github.com/microsoft/amplifier-module-context-simple)

Study this module for:
- Basic ContextManager implementation
- Token counting approach
- Internal compaction with tool pair preservation

Additional examples:
- [amplifier-module-context-persistent](https://github.com/microsoft/amplifier-module-context-persistent) - File-based persistence

---

## Validation Checklist

### Required

- [ ] Implements all 5 ContextManager protocol methods
- [ ] `mount()` function with entry point in pyproject.toml
- [ ] `get_messages_for_request()` handles compaction internally
- [ ] Compaction preserves tool_use/tool_result pairs
- [ ] Messages returned in conversation order

### Recommended

- [ ] Token counting for accurate compaction triggers
- [ ] Emits context:pre_compact and context:post_compact events
- [ ] Preserves system messages during compaction
- [ ] Thread-safe for concurrent access
- [ ] Configurable thresholds

---

## Testing

Use test utilities from `amplifier_core/testing.py`:

```python
from amplifier_core.testing import MockContextManager

@pytest.mark.asyncio
async def test_context_manager():
    context = MyContextManager(max_tokens=1000)

    # Add messages
    await context.add_message({"role": "user", "content": "Hello"})
    await context.add_message({"role": "assistant", "content": "Hi there!"})

    # Get messages for request (may compact)
    messages = await context.get_messages_for_request()
    assert len(messages) == 2
    assert messages[0]["role"] == "user"

    # Get raw messages (no compaction)
    raw_messages = await context.get_messages()
    assert len(raw_messages) == 2

    # Test clear
    await context.clear()
    assert len(await context.get_messages()) == 0


@pytest.mark.asyncio
async def test_compaction_preserves_tool_pairs():
    """Verify tool_use and tool_result stay together during compaction."""
    context = MyContextManager(max_tokens=100, compaction_threshold=0.5)

    # Add messages including tool sequence
    await context.add_message({"role": "user", "content": "Read file.txt"})
    await context.add_message({
        "role": "assistant",
        "content": None,
        "tool_calls": [{"id": "call_123", "type": "function", "function": {...}}]
    })
    await context.add_message({
        "role": "tool",
        "tool_call_id": "call_123",
        "content": "File contents..."
    })

    # Force compaction by adding more messages
    for i in range(50):
        await context.add_message({"role": "user", "content": f"Message {i}"})

    # Get messages for request (triggers compaction)
    messages = await context.get_messages_for_request()

    # Verify tool pairs are preserved
    tool_use_ids = set()
    tool_result_ids = set()
    for msg in messages:
        if msg.get("tool_calls"):
            for tc in msg["tool_calls"]:
                tool_use_ids.add(tc.get("id"))
        if msg.get("role") == "tool":
            tool_result_ids.add(msg.get("tool_call_id"))

    # Every tool result should have matching tool use
    assert tool_result_ids.issubset(tool_use_ids), "Orphaned tool results found!"


@pytest.mark.asyncio
async def test_session_resume():
    """Verify set_messages works for session resume."""
    context = MyContextManager(max_tokens=1000)

    saved_messages = [
        {"role": "user", "content": "Previous conversation"},
        {"role": "assistant", "content": "Previous response"}
    ]

    await context.set_messages(saved_messages)

    messages = await context.get_messages()
    assert len(messages) == 2
    assert messages[0]["content"] == "Previous conversation"
```

### MockContextManager for Testing

```python
from amplifier_core.testing import MockContextManager

# For testing orchestrators
context = MockContextManager()

await context.add_message({"role": "user", "content": "Test"})
messages = await context.get_messages_for_request()

# Access internal state for assertions
assert len(context.messages) == 1
```

---

## Quick Validation Command

```bash
# Structural validation
amplifier module validate ./my-context --type context
```

---

**Related**: [README.md](README.md) | [ORCHESTRATOR_CONTRACT.md](ORCHESTRATOR_CONTRACT.md)
