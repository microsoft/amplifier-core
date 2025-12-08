# Module Contracts

**Start here for building Amplifier modules.**

This directory contains the authoritative guidance for building each type of Amplifier module. Each contract document explains:

1. **What it is** - Purpose and responsibilities
2. **Protocol reference** - Link to interfaces.py with exact line numbers
3. **Entry point pattern** - How modules are discovered and loaded
4. **Configuration** - Mount Plan integration
5. **Canonical example** - Reference implementation
6. **Validation** - How to verify your module works

---

## Module Types

| Module Type | Contract | Purpose |
|-------------|----------|---------|
| **Provider** | [PROVIDER_CONTRACT.md](PROVIDER_CONTRACT.md) | LLM backend integration |
| **Tool** | [TOOL_CONTRACT.md](TOOL_CONTRACT.md) | Agent capabilities |
| **Hook** | [HOOK_CONTRACT.md](HOOK_CONTRACT.md) | Lifecycle observation and control |
| **Orchestrator** | [ORCHESTRATOR_CONTRACT.md](ORCHESTRATOR_CONTRACT.md) | Agent loop execution strategy |
| **Context** | [CONTEXT_CONTRACT.md](CONTEXT_CONTRACT.md) | Conversation memory management |

---

## Quick Start Pattern

All modules follow this pattern:

```python
# 1. Implement the Protocol from interfaces.py
class MyModule:
    # ... implement required methods
    pass

# 2. Provide mount() function
async def mount(coordinator, config):
    """Initialize and register module."""
    instance = MyModule(config)
    await coordinator.mount("category", instance, name="my-module")
    return instance  # or cleanup function

# 3. Register entry point in pyproject.toml
# [project.entry-points."amplifier.modules"]
# my-module = "my_package:mount"
```

---

## Source of Truth

**Protocols are in code**, not docs:

- **Protocol definitions**: `amplifier_core/interfaces.py`
- **Data models**: `amplifier_core/models.py`
- **Message models**: `amplifier_core/message_models.py` (Pydantic models for request/response envelopes)
- **Content models**: `amplifier_core/content_models.py` (dataclass types for events and streaming)

These contract documents provide **guidance** that code cannot express. Always read the code docstrings first.

---

## Related Documentation

- [MOUNT_PLAN_SPECIFICATION.md](../specs/MOUNT_PLAN_SPECIFICATION.md) - Configuration contract
- [MODULE_SOURCE_PROTOCOL.md](../MODULE_SOURCE_PROTOCOL.md) - Module loading mechanism
- [CONTRIBUTION_CHANNELS.md](../specs/CONTRIBUTION_CHANNELS.md) - Module contribution pattern
- [DESIGN_PHILOSOPHY.md](../DESIGN_PHILOSOPHY.md) - Kernel design principles

---

## Validation

Verify your module before release:

```bash
# Structural validation
amplifier module validate ./my-module
```

See individual contract documents for type-specific validation requirements.

---

**For ecosystem overview**: [amplifier](https://github.com/microsoft/amplifier)
