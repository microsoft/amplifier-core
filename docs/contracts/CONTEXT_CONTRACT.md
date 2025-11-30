---
contract_type: module_specification
module_type: context
contract_version: 1.0.0
last_modified: 2025-01-29
related_files:
  - path: amplifier_core/interfaces.py#ContextManager
    relationship: protocol_definition
    lines: 148-171
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
- **Compaction** - Reduce context when approaching limits
- **Persistence** - Optionally persist across sessions
- **Memory strategies** - Implement various memory patterns

**Key principle**: The context manager is **policy** for memory. Swap to change how agents remember without modifying the kernel.

---

## Protocol Definition

**Source**: `amplifier_core/interfaces.py` lines 148-171

```python
@runtime_checkable
class ContextManager(Protocol):
    async def add_message(self, message: dict[str, Any]) -> None:
        """Add a message to the context."""
        ...

    async def get_messages(self) -> list[dict[str, Any]]:
        """Get all messages in the context."""
        ...

    async def should_compact(self) -> bool:
        """Check if context should be compacted."""
        ...

    async def compact(self) -> None:
        """Compact the context to reduce size."""
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

### get_messages()

Return messages in conversation order:

```python
async def get_messages(self) -> list[dict[str, Any]]:
    """Get all messages in the context."""
    return list(self._messages)  # Return copy to prevent mutation
```

### should_compact()

Check if context exceeds threshold:

```python
async def should_compact(self) -> bool:
    """Check if context should be compacted."""
    return self._token_count > (self._max_tokens * self._compaction_threshold)
```

### compact()

Reduce context size while preserving key information:

```python
async def compact(self) -> None:
    """Compact the context to reduce size."""
    # Emit pre-compaction event
    await self._hooks.emit("context:pre_compact", {
        "message_count": len(self._messages),
        "token_count": self._token_count
    })

    # Strategy: Keep system messages + recent messages
    system_messages = [m for m in self._messages if m["role"] == "system"]
    recent_messages = self._messages[-self._keep_recent:]

    self._messages = system_messages + recent_messages
    self._token_count = sum(self._estimate_tokens(m) for m in self._messages)

    # Emit post-compaction event
    await self._hooks.emit("context:post_compact", {
        "message_count": len(self._messages),
        "token_count": self._token_count
    })
```

### clear()

Reset context state:

```python
async def clear(self) -> None:
    """Clear all messages."""
    self._messages = []
    self._token_count = 0
```

---

## Compaction Strategies

Different strategies for different use cases:

### Simple Truncation

Keep N most recent messages:

```python
self._messages = self._messages[-keep_count:]
```

### Summarization

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

### Importance-Based

Keep messages based on importance score:

```python
scored = [(m, self._score_importance(m)) for m in self._messages]
scored.sort(key=lambda x: x[1], reverse=True)
self._messages = [m for m, _ in scored[:keep_count]]
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
- Compaction logic

Additional examples:
- [amplifier-module-context-persistent](https://github.com/microsoft/amplifier-module-context-persistent) - File-based persistence

---

## Validation Checklist

### Required

- [ ] Implements all 5 ContextManager protocol methods
- [ ] `mount()` function with entry point in pyproject.toml
- [ ] Messages returned in conversation order
- [ ] Compaction reduces context size

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

    # Verify storage
    messages = await context.get_messages()
    assert len(messages) == 2
    assert messages[0]["role"] == "user"

    # Test compaction
    if await context.should_compact():
        await context.compact()
        new_messages = await context.get_messages()
        assert len(new_messages) < len(messages)

    # Test clear
    await context.clear()
    assert len(await context.get_messages()) == 0

@pytest.mark.asyncio
async def test_compaction():
    context = MyContextManager(max_tokens=100, compaction_threshold=0.5)

    # Add many messages to trigger compaction
    for i in range(50):
        await context.add_message({"role": "user", "content": f"Message {i}"})

    assert await context.should_compact()
    await context.compact()

    messages = await context.get_messages()
    assert len(messages) < 50
```

### MockContextManager for Testing

```python
from amplifier_core.testing import MockContextManager

# For testing orchestrators
context = MockContextManager()

await context.add_message({"role": "user", "content": "Test"})
messages = await context.get_messages()

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
