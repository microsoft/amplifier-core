# Capability Registry

**Purpose**: Enable module ↔ app communication without direct dependencies via inversion of control.

---

## Overview

The capability registry allows app layers to provide functionality that modules can consume without knowing the implementation. This maintains clean separation: modules depend only on kernel (amplifier-core), while apps register capabilities that modules request at runtime.

## API

```python
# App layer registers capability
coordinator.register_capability("session.spawn", spawn_fn)

# Module requests capability
spawn_fn = coordinator.get_capability("session.spawn")
if spawn_fn:
    result = await spawn_fn(agent_name="zen-architect", task="analyze this")
else:
    # Handle gracefully - capability not available in this app
    raise ToolError("session.spawn capability not available")
```

## Standard Capabilities

| Capability | Contract | Provider | Consumer |
|------------|----------|----------|----------|
| `session.spawn` | `async (agent_name: str, task: str, parent_session) → dict` | amplifier-app-cli | tool-task |
| `session.resume` | `async (session_id: str, task: str) → dict` | amplifier-app-cli | tool-task |

## Pattern

```
┌─────────────────────────────────────────┐
│  App Layer                               │
│  PROVIDES: capabilities                  │
│  - Implements with app-specific logic    │
│  - Registers at session creation         │
└─────────────────────────────────────────┘
                    │ registers
                    ▼
┌─────────────────────────────────────────┐
│  Kernel (coordinator)                    │
│  MECHANISM: capability registry          │
│  - register_capability(name, fn)         │
│  - get_capability(name) → fn | None      │
└─────────────────────────────────────────┘
                    │ requests
                    ▼
┌─────────────────────────────────────────┐
│  Module                                  │
│  CONSUMES: capabilities                  │
│  - Requests via coordinator              │
│  - Handles missing gracefully            │
│  - NO app-layer imports                  │
└─────────────────────────────────────────┘
```

## Guidelines

**For App Developers**:
- Register capabilities during session creation
- Document the contract (parameters, return type)
- Capabilities are session-scoped

**For Module Developers**:
- Always check if capability exists before using
- Provide clear error message when capability missing
- Never import from app layer - use capabilities instead

## Implementation

See `coordinator.py` lines 230-254 for the kernel mechanism.

See `amplifier-app-cli/amplifier_app_cli/main.py` (`_register_session_spawning()`) for how amplifier-app-cli registers session capabilities.
