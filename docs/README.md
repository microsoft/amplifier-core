# Amplifier Core Documentation

**Kernel specifications.**

---

## Specifications

- [MOUNT_PLAN_SPECIFICATION.md](specs/MOUNT_PLAN_SPECIFICATION.md) - Configuration format
- [CONTRIBUTION_CHANNELS.md](specs/CONTRIBUTION_CHANNELS.md) - Aggregation mechanism for module contributions
- [HOOKS_API.md](HOOKS_API.md) - Hook system API (HookResult, events, registration)
- [MODULE_SOURCE_PROTOCOL.md](MODULE_SOURCE_PROTOCOL.md) - Module loading mechanism
- [SESSION_FORK_SPECIFICATION.md](SESSION_FORK_SPECIFICATION.md) - Child sessions

---

## Rust Kernel

- [RUST_CORE_TESTING.md](RUST_CORE_TESTING.md) - Development setup and testing guide
- [RUST_CORE_LIMITATIONS.md](RUST_CORE_LIMITATIONS.md) - Known limitations
- [CONTRACTS.md](../CONTRACTS.md) - Authoritative Rust/Python type mapping

---

## Principles

- [DESIGN_PHILOSOPHY.md](DESIGN_PHILOSOPHY.md) - Kernel design framework

---

**Protocols are in code** (`python/amplifier_core/interfaces.py`), not duplicated in docs.

For ecosystem: **-> [amplifier](https://github.com/microsoft/amplifier)**