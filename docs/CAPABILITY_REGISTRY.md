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
| `provider.before_load` | `async (coordinator, provider_spec: dict) → None` | host application | Python session initialization |
| `provider.load_failure` | `async (coordinator, provider_spec: dict, error: Exception) → None` | host application | Python session initialization |

### Provider initialization failures

`provider.before_load` is an optional asynchronous host preflight. It receives
an independent copy of the exact configured entry before loader or mount code
runs. Returning continues normal loading; raising routes through the same mount
restoration, observability event and `provider.load_failure` callback as an
import or mount failure. It lets a host retain a provider whose configuration
failed validation without attempting it with incomplete or substituted credentials.
Neither callback changes the configured entry unless the host explicitly edits
its own session state.

Register `provider.load_failure` before session initialization to apply host
policy when a configured provider cannot load, mount, or finish instance remapping.
It runs once for the failed configuration entry, after restoring the provider
mount table and emitting `module:load_failed`, and before loading the next entry.
The spec is a deep copy of that entry, including its source, `instance_id`, and
configuration. The callback may mount a host-owned unavailable-provider marker,
record an outcome, or raise to abort initialization. Callback exceptions propagate;
observability event-handler exceptions do not.

Without a callback, provider failure keeps the existing warn-and-continue policy.
Core does not select a replacement, prune configuration, reorder providers, or
decide whether a failed provider is required. Hosts must preserve configured
identity and implement routing policy themselves. A callback that retains a
failure marker should use the configured instance identity and a safe reason code;
the spec and exception may contain secrets and must not be copied to public state.

Restoration covers the provider mount table, including an overwritten default
slot and mounts added by a failed attempt. It does not undo arbitrary external
side effects inside third-party mount functions. A failed instance's readiness
callback is not queued. Rollback failure aborts initialization rather than asking
the host to recover against a partially restored table.

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
