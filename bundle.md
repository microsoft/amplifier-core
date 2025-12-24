---
bundle:
  name: core
  version: 1.0.0
  description: Amplifier kernel - ultra-thin mechanism layer providing stable contracts

includes:
  - bundle: git+https://github.com/microsoft/amplifier-foundation@main
  - bundle: core:behaviors/core-expert
---

# Amplifier Core (Kernel)

@core:context/kernel-overview.md

---

The **core** bundle is the representative for the Amplifier kernel. The kernel is intentionally tiny (~2,600 lines) and provides MECHANISMS only, never POLICIES.

**Core Principle**: "The center stays still so the edges can move fast."

## Documentation

### Core Documentation

@core:docs/

Key documents:
- **DESIGN_PHILOSOPHY.md** - Why the kernel is tiny and boring
- **HOOKS_API.md** - Hook system for observability and control
- **MODULE_SOURCE_PROTOCOL.md** - How modules are loaded

### Specifications

@core:docs/specs/

- **MOUNT_PLAN_SPECIFICATION.md** - The configuration contract
- **PROVIDER_SPECIFICATION.md** - Provider module contract
- **CONTRIBUTION_CHANNELS.md** - Module contribution system

### Contracts

@core:docs/contracts/

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ KERNEL (amplifier-core) - Ring 0                             │
│                                                              │
│ Provides ONLY:                                               │
│ • Module loading/unloading                                  │
│ • Session lifecycle                                         │
│ • Event system                                              │
│ • Coordinator infrastructure                                │
│ • Stable contracts (protocols)                              │
│                                                              │
│ NEVER decides:                                               │
│ • Which modules to use (that's app layer)                   │
│ • How to orchestrate (that's orchestrator module)           │
│ • Which provider to call (that's policy)                    │
│ • What to log (that's hooks module)                         │
└─────────────────────────────────────────────────────────────┘
```

## When to Use This Bundle

Include this bundle when you need:
- Deep understanding of kernel contracts
- Module development guidance
- Understanding protocol specifications
- Deciding if something belongs in kernel vs module

## Expert Agent

The **core-expert** agent is the authoritative consultant for kernel internals. Consult it for:
- Kernel contracts and protocols
- Module development
- Event system and hooks
- Session lifecycle
- Deciding kernel vs module placement

---

@foundation:context/shared/common-system-base.md
