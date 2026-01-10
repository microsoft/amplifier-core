---
contract_type: module_specification
module_type: orchestrator
contract_version: 1.0.0
last_modified: 2025-01-29
related_files:
  - path: amplifier_core/interfaces.py#Orchestrator
    relationship: protocol_definition
    lines: 26-52
  - path: amplifier_core/content_models.py
    relationship: event_content_types
  - path: ../specs/MOUNT_PLAN_SPECIFICATION.md
    relationship: configuration
  - path: ../specs/CONTRIBUTION_CHANNELS.md
    relationship: observability
  - path: amplifier_core/testing.py#ScriptedOrchestrator
    relationship: test_utilities
canonical_example: https://github.com/microsoft/amplifier-module-loop-basic
---

# Orchestrator Contract

Orchestrators implement the agent execution loop strategy.

---

## Purpose

Orchestrators control **how** agents execute:
- **Basic loops** - Simple prompt → response → tool → response cycles
- **Streaming** - Real-time response delivery
- **Event-driven** - Complex multi-step workflows
- **Custom strategies** - Domain-specific execution patterns

**Key principle**: The orchestrator is **policy**, not mechanism. Swap orchestrators to change agent behavior without modifying the kernel.

---

## Protocol Definition

**Source**: `amplifier_core/interfaces.py` lines 26-52

```python
@runtime_checkable
class Orchestrator(Protocol):
    async def execute(
        self,
        prompt: str,
        context: ContextManager,
        providers: dict[str, Provider],
        tools: dict[str, Tool],
        hooks: HookRegistry,
    ) -> str:
        """
        Execute the agent loop with given prompt.

        Args:
            prompt: User input prompt
            context: Context manager for conversation state
            providers: Available LLM providers (keyed by name)
            tools: Available tools (keyed by name)
            hooks: Hook registry for lifecycle events

        Returns:
            Final response string
        """
        ...
```

---

## Execution Flow

A typical orchestrator implements this flow:

```
User Prompt
    ↓
Add to Context
    ↓
┌─────────────────────────────────────┐
│  LOOP until response has no tools   │
│                                     │
│  1. emit("provider:request")        │
│  2. provider.complete(messages)     │
│  3. emit("provider:response")       │
│  4. Add response to context         │
│                                     │
│  If tool_calls:                     │
│    for each tool_call:              │
│      5. emit("tool:pre")            │
│      6. tool.execute(input)         │
│      7. emit("tool:post")           │
│      8. Add result to context       │
│                                     │
│  Continue loop...                   │
└─────────────────────────────────────┘
    ↓
Return final text response
```

---

## Entry Point Pattern

### mount() Function

```python
async def mount(coordinator: ModuleCoordinator, config: dict) -> Orchestrator | Callable | None:
    """
    Initialize and return orchestrator instance.

    Returns:
        - Orchestrator instance
        - Cleanup callable
        - None for graceful degradation
    """
    orchestrator = MyOrchestrator(config=config)
    await coordinator.mount("session", orchestrator, name="orchestrator")
    return orchestrator
```

### pyproject.toml

```toml
[project.entry-points."amplifier.modules"]
my-orchestrator = "my_orchestrator:mount"
```

---

## Implementation Requirements

### Event Emission

Orchestrators must emit lifecycle events for observability:

```python
async def execute(self, prompt, context, providers, tools, hooks):
    # Before LLM call
    await hooks.emit("provider:request", {
        "provider": provider_name,
        "messages": messages,
        "model": model_name
    })

    response = await provider.complete(request)

    # After LLM call
    await hooks.emit("provider:response", {
        "provider": provider_name,
        "response": response,
        "usage": response.usage
    })

    # Before tool execution
    await hooks.emit("tool:pre", {
        "tool_name": tool_call.name,
        "tool_input": tool_call.input
    })

    result = await tool.execute(tool_call.input)

    # After tool execution
    await hooks.emit("tool:post", {
        "tool_name": tool_call.name,
        "tool_input": tool_call.input,
        "tool_result": result
    })

    # REQUIRED: At the end of execute(), emit orchestrator:complete
    await hooks.emit("orchestrator:complete", {
        "orchestrator": "my-orchestrator",  # Your orchestrator name
        "turn_count": iteration_count,       # Number of LLM turns
        "status": "success"                  # "success", "incomplete", or "cancelled"
    })
```

#### Required: orchestrator:complete Event

**All orchestrators MUST emit `orchestrator:complete`** at the end of their `execute()` method. This event enables:
- Session analytics and debugging
- Hooks that trigger on turn completion (e.g., session naming)
- Observability and monitoring

| Field | Type | Description |
|-------|------|-------------|
| `orchestrator` | string | Name of the orchestrator module |
| `turn_count` | int | Number of LLM call iterations |
| `status` | string | Exit status: `"success"`, `"incomplete"`, or `"cancelled"` |

### Hook Processing

Handle HookResult actions:

```python
# Before tool execution
pre_result = await hooks.emit("tool:pre", data)

if pre_result.action == "deny":
    # Don't execute tool
    return ToolResult(is_error=True, output=pre_result.reason)

if pre_result.action == "modify":
    # Use modified data
    data = pre_result.data

if pre_result.action == "inject_context":
    # Add feedback to context
    await context.add_message({
        "role": pre_result.context_injection_role,
        "content": pre_result.context_injection
    })

if pre_result.action == "ask_user":
    # Request approval (requires approval provider)
    approved = await request_approval(pre_result)
    if not approved:
        return ToolResult(is_error=True, output="User denied")
```

### Context Management

Manage conversation state:

```python
# Add user message
await context.add_message({"role": "user", "content": prompt})

# Add assistant response
await context.add_message({"role": "assistant", "content": response.content})

# Add tool result
await context.add_message({
    "role": "tool",
    "tool_call_id": tool_call.id,
    "content": result.output
})

# Get messages for LLM request (context handles compaction internally)
messages = await context.get_messages_for_request()
```

### Provider Selection

Handle multiple providers:

```python
# Get default or configured provider
provider_name = config.get("default_provider", list(providers.keys())[0])
provider = providers[provider_name]

# Or allow per-request provider selection
provider_name = request_options.get("provider", default_provider_name)
```

---

## Configuration

Orchestrators receive configuration via Mount Plan:

```yaml
session:
  orchestrator: my-orchestrator
  context: context-simple

# Orchestrator-specific config can be passed via providers/tools config
```

See [MOUNT_PLAN_SPECIFICATION.md](../specs/MOUNT_PLAN_SPECIFICATION.md) for full schema.

---

## Observability

Register custom events your orchestrator emits:

```python
coordinator.register_contributor(
    "observability.events",
    "my-orchestrator",
    lambda: [
        "my-orchestrator:loop_started",
        "my-orchestrator:loop_iteration",
        "my-orchestrator:loop_completed"
    ]
)
```

See [CONTRIBUTION_CHANNELS.md](../specs/CONTRIBUTION_CHANNELS.md) for the pattern.

---

## Canonical Example

**Reference implementation**: [amplifier-module-loop-basic](https://github.com/microsoft/amplifier-module-loop-basic)

Study this module for:
- Complete execute() implementation
- Event emission patterns
- Hook result handling
- Context management

Additional examples:
- [amplifier-module-loop-streaming](https://github.com/microsoft/amplifier-module-loop-streaming) - Real-time streaming
- [amplifier-module-loop-events](https://github.com/microsoft/amplifier-module-loop-events) - Event-driven patterns

---

## Validation Checklist

### Required

- [ ] Implements `execute(prompt, context, providers, tools, hooks) -> str`
- [ ] `mount()` function with entry point in pyproject.toml
- [ ] Emits standard events (provider:request/response, tool:pre/post)
- [ ] **Emits `orchestrator:complete` at the end of execute()**
- [ ] Handles HookResult actions appropriately
- [ ] Manages context (add messages, check compaction)

### Recommended

- [ ] Supports multiple providers
- [ ] Implements max iterations limit (prevent infinite loops)
- [ ] Handles provider errors gracefully
- [ ] Registers custom observability events
- [ ] Supports streaming via async generators

---

## Testing

Use test utilities from `amplifier_core/testing.py`:

```python
from amplifier_core.testing import (
    TestCoordinator,
    MockTool,
    MockContextManager,
    ScriptedOrchestrator,
    EventRecorder
)

@pytest.mark.asyncio
async def test_orchestrator_basic():
    orchestrator = MyOrchestrator(config={})
    context = MockContextManager()
    providers = {"test": MockProvider()}
    tools = {"test_tool": MockTool()}
    hooks = HookRegistry()

    result = await orchestrator.execute(
        prompt="Test prompt",
        context=context,
        providers=providers,
        tools=tools,
        hooks=hooks
    )

    assert isinstance(result, str)
    assert len(context.messages) > 0
```

### ScriptedOrchestrator for Testing

```python
from amplifier_core.testing import ScriptedOrchestrator

# For testing components that use orchestrators
orchestrator = ScriptedOrchestrator(responses=["Response 1", "Response 2"])

result = await orchestrator.execute(...)
assert result == "Response 1"
```

---

## Quick Validation Command

```bash
# Structural validation
amplifier module validate ./my-orchestrator --type orchestrator
```

---

**Related**: [README.md](README.md) | [CONTEXT_CONTRACT.md](CONTEXT_CONTRACT.md)
