# Amplifier Core: Kernel Specifications

**Mechanism, not policy.** These documents define kernel contracts that all implementations must honor.

---

## Kernel Specifications

### [MODULE_SOURCE_PROTOCOL.md](./MODULE_SOURCE_PROTOCOL.md)

Protocols for custom module source resolution.

- ModuleSource protocol
- ModuleSourceResolver protocol
- Loader injection mechanism
- Error contracts

### [SESSION_FORK_SPECIFICATION.md](./SESSION_FORK_SPECIFICATION.md)

Session forking and lineage tracking.

- parent_id parameter
- Child session creation
- Lineage tracking
- Event emission

### [COORDINATOR_INFRASTRUCTURE_CONTEXT.md](./COORDINATOR_INFRASTRUCTURE_CONTEXT.md)

Mount point system and coordinator architecture.

- Mount point mechanism
- Component lifecycle
- Module mounting contracts

---

## What Belongs Here

**Include:**
- ✅ Protocol definitions
- ✅ Kernel API contracts
- ✅ Mechanisms (what kernel provides)
- ✅ Invariants (what must always hold)
- ✅ Error contracts

**Exclude:**
- ❌ Policy decisions
- ❌ Reference implementations
- ❌ App-layer conventions
- ❌ CLI commands
- ❌ Configuration formats

---

## Reference Implementations

Reference implementations and app-layer policies live in:
- [amplifier-dev/docs](https://github.com/microsoft/amplifier-dev/tree/main/docs)

---

## Philosophy

These specifications align with:
- [KERNEL_PHILOSOPHY.md](https://github.com/microsoft/amplifier-dev/blob/main/docs/context/KERNEL_PHILOSOPHY.md)
- [AMPLIFIER_AS_LINUX_KERNEL.md](https://github.com/microsoft/amplifier-dev/blob/main/docs/AMPLIFIER_AS_LINUX_KERNEL.md)
