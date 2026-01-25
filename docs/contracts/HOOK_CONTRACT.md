---
contract_type: module_specification
module_type: hook
contract_version: 1.0.0
last_modified: 2025-01-29
related_files:
  - path: amplifier_core/interfaces.py#HookHandler
    relationship: protocol_definition
    lines: 205-220
  - path: amplifier_core/models.py#HookResult
    relationship: result_model
  - path: ../HOOKS_API.md
    relationship: detailed_api
  - path: ../specs/MOUNT_PLAN_SPECIFICATION.md
    relationship: configuration
  - path: ../specs/CONTRIBUTION_CHANNELS.md
    relationship: observability
  - path: amplifier_core/testing.py#EventRecorder
    relationship: test_utilities
canonical_example: https://github.com/microsoft/amplifier-module-hooks-logging
---

# Hook Contract

Hooks observe, validate, and control agent lifecycle events.

---

## Purpose

Hooks enable:
- **Observation** - Logging, metrics, audit trails
- **Validation** - Security checks, input validation
- **Feedback injection** - Automated correction loops
- **Approval gates** - Dynamic permission requests
- **Output control** - Clean user experience

---

## Detailed API Reference

**See [HOOKS_API.md](../HOOKS_API.md)** for complete documentation including:

- HookResult actions and fields
- Registration patterns
- Common patterns with examples
- Best practices

This contract provides the essentials. The API reference contains full details.

---

## Protocol Definition

**Source**: `amplifier_core/interfaces.py` lines 205-220

```python
@runtime_checkable
class HookHandler(Protocol):
    async def __call__(self, event: str, data: dict[str, Any]) -> HookResult:
        """
        Handle a lifecycle event.

        Args:
            event: Event name (e.g., "tool:pre", "execution:start")
            data: Event-specific data

        Returns:
            HookResult indicating action to take
        """
        ...
```

---

## HookResult Actions

**Source**: `amplifier_core/models.py`

| Action | Behavior | Use Case |
|--------|----------|----------|
| `continue` | Proceed normally | Default, observation only |
| `deny` | Block operation | Validation failure, security |
| `modify` | Transform data | Preprocessing, enrichment |
| `inject_context` | Add to agent's context | Feedback loops, corrections |
| `ask_user` | Request approval | High-risk operations |

```python
from amplifier_core.models import HookResult

# Simple observation
HookResult(action="continue")

# Block with reason
HookResult(action="deny", reason="Access denied")

# Inject feedback
HookResult(
    action="inject_context",
    context_injection="Found 3 linting errors...",
    user_message="Linting issues detected"
)

# Request approval
HookResult(
    action="ask_user",
    approval_prompt="Allow write to production file?",
    approval_default="deny"
)
```

---

## Entry Point Pattern

### mount() Function

```python
async def mount(coordinator: ModuleCoordinator, config: dict) -> Callable | None:
    """
    Initialize and register hook handlers.

    Returns:
        Cleanup callable to unregister handlers
    """
    handlers = []

    # Register handlers for specific events
    handlers.append(
        coordinator.hooks.register("tool:pre", my_validation_hook, priority=10)
    )
    handlers.append(
        coordinator.hooks.register("tool:post", my_feedback_hook, priority=20)
    )

    # Return cleanup function
    def cleanup():
        for unregister in handlers:
            unregister()

    return cleanup
```

### pyproject.toml

```toml
[project.entry-points."amplifier.modules"]
my-hook = "my_hook:mount"
```

---

## Event Registration

Register handlers during mount():

```python
from amplifier_core.hooks import HookRegistry

# Get registry from coordinator
registry: HookRegistry = coordinator.hooks

# Register with priority (lower = earlier)
unregister = registry.register(
    event="tool:post",
    handler=my_handler,
    priority=10,
    name="my_handler"
)

# Later: unregister()
```

---

## Common Events

| Event | Trigger | Data Includes |
|-------|---------|---------------|
| `execution:start` | Orchestrator execution begins | prompt |
| `execution:end` | Orchestrator execution completes | response |
| `prompt:submit` | User input | prompt text |
| `tool:pre` | Before tool execution | tool_name, tool_input |
| `tool:post` | After tool execution | tool_name, tool_result |
| `tool:error` | Tool failed | tool_name, error |
| `provider:request` | LLM call starting | provider, messages |
| `provider:response` | LLM call complete | provider, response, usage |

---

## Configuration

Hooks receive configuration via Mount Plan:

```yaml
hooks:
  - module: my-hook
    source: git+https://github.com/org/my-hook@main
    config:
      enabled_events:
        - "tool:pre"
        - "tool:post"
      log_level: "info"
```

See [MOUNT_PLAN_SPECIFICATION.md](../specs/MOUNT_PLAN_SPECIFICATION.md) for full schema.

---

## Observability

Register custom events your hook emits:

```python
coordinator.register_contributor(
    "observability.events",
    "my-hook",
    lambda: ["my-hook:validation_failed", "my-hook:approved"]
)
```

See [CONTRIBUTION_CHANNELS.md](../specs/CONTRIBUTION_CHANNELS.md) for the pattern.

---

## Canonical Example

**Reference implementation**: [amplifier-module-hooks-logging](https://github.com/microsoft/amplifier-module-hooks-logging)

Study this module for:
- Hook registration patterns
- Event handling
- Configuration integration
- Observability contribution

Additional examples:
- [amplifier-module-hooks-approval](https://github.com/microsoft/amplifier-module-hooks-approval) - Approval gates
- [amplifier-module-hooks-redaction](https://github.com/microsoft/amplifier-module-hooks-redaction) - Security redaction

---

## Validation Checklist

### Required

- [ ] Handler implements `async def __call__(event, data) -> HookResult`
- [ ] `mount()` function with entry point in pyproject.toml
- [ ] Returns valid `HookResult` for all code paths
- [ ] Handles exceptions gracefully (don't crash kernel)

### Recommended

- [ ] Register cleanup function to unregister handlers
- [ ] Use appropriate priority (10-90, lower = earlier)
- [ ] Log handler registration for debugging
- [ ] Support configuration for enabled events
- [ ] Register custom events via contribution channels

---

## Testing

Use test utilities from `amplifier_core/testing.py`:

```python
from amplifier_core.testing import TestCoordinator, EventRecorder
from amplifier_core.models import HookResult

@pytest.mark.asyncio
async def test_hook_handler():
    # Test handler directly
    result = await my_validation_hook("tool:pre", {
        "tool_name": "Write",
        "tool_input": {"file_path": "/etc/passwd"}
    })

    assert result.action == "deny"
    assert "denied" in result.reason.lower()

@pytest.mark.asyncio
async def test_hook_registration():
    coordinator = TestCoordinator()
    cleanup = await mount(coordinator, {})

    # Verify handlers registered
    # ... test event emission

    cleanup()
```

### EventRecorder for Testing

```python
from amplifier_core.testing import EventRecorder

recorder = EventRecorder()

# Use in tests
await recorder.record("tool:pre", {"tool_name": "Write"})

# Assert
events = recorder.get_events()
assert len(events) == 1
assert events[0][0] == "tool:pre"  # events are (event_name, data) tuples
```

---

## Quick Validation Command

```bash
# Structural validation
amplifier module validate ./my-hook --type hook
```

---

**Related**: [HOOKS_API.md](../HOOKS_API.md) | [README.md](README.md)
