---
spec_type: kernel_mechanism
last_modified: 2025-01-29
related_contracts:
  - path: ../contracts/PROVIDER_CONTRACT.md
    relationship: referenced_by
  - path: ../contracts/HOOK_CONTRACT.md
    relationship: referenced_by
  - path: ../contracts/ORCHESTRATOR_CONTRACT.md
    relationship: referenced_by
  - path: ../contracts/CONTEXT_CONTRACT.md
    relationship: referenced_by
---

# Contribution Channels

Mechanism for pull-based aggregation across kernel modules.

---

## Purpose

Contribution channels allow multiple modules to produce data that consumers aggregate on demand. The kernel coordinates registration and collection but does not interpret the payload.

Key properties:

- **Pull-based** – consumers call `collect_contributions()` when they need data.
- **Non-interfering** – a failing contributor is logged and skipped; other results remain intact.
- **Order preserving** – contributions are returned in registration order.
- **Format agnostic** – callbacks choose their return type; consumers interpret it.

This document is the canonical kernel contract.

---

## Coordinator API

### Registering contributors

```python
coordinator.register_contributor(
    channel: str,
    name: str,
    callback: Callable[[], Any] | Callable[[], Awaitable[Any]],
) -> None
```

- `channel` – channel identifier such as `"observability.events"` or `"capabilities"`.
- `name` – contributor label surfaced in diagnostics.
- `callback` – sync or async callable that returns the contribution. Returning `None` skips the entry.

Implementation: [`amplifier_core/coordinator.py`](../../amplifier_core/coordinator.py#L246).

### Collecting contributions

```python
contributions: list[Any] = await coordinator.collect_contributions(channel: str)
```

- Iterates through every registered callback for the channel.
- Supports callbacks that are async functions or that return coroutines.
- Filters out `None` values before returning.
- Logs exceptions and continues (non-interfering failure handling).
- Returns an empty list when no contributors are registered.

Reference tests: [`tests/test_contribution_channels.py`](../../tests/test_contribution_channels.py).

---

## Channel Naming

Use `{domain}.{purpose}` for shared channels. Examples:

- `observability.events` – modules declare lifecycle events.
- `capabilities.catalog` – aggregate callable capabilities.
- `session.metadata` – runtime metadata snapshots.

Bundles can scope private channels with `{bundle}:{purpose}` (e.g., `toolkit:debug-metrics`).

---

## Lifecycle Pattern

### Module registration

Modules typically register contributors during `mount()`:

```python
async def mount(coordinator, config):
    coordinator.register_contributor(
        "observability.events",
        "tool-filesystem",
        lambda: [
            "filesystem:read",
            "filesystem:write",
            "filesystem:delete",
        ],
    )
```

### Consumer collection

Consumers gather contributions when they require the aggregated data:

```python
events = await coordinator.collect_contributions("observability.events")
flattened: list[str] = []
for contribution in events:
    if isinstance(contribution, list):
        flattened.extend(contribution)
```

The kernel returns raw contributions; consumers own interpretation and shaping (flattening, validation, etc.).

---

## Failure Handling

- Callbacks run sequentially in registration order. Keep them lightweight to avoid blocking the event loop.
- Any exception raised by a callback is logged with the contributor name and channel, then collection proceeds.
- Returning `None` indicates “no contribution right now” and is filtered out of the final list.
- Repeated registrations with the same `name` are allowed; use unique names when diagnostics matter.

---

## Guidance for Implementers

- **Idempotency** – callbacks may be invoked multiple times within a session. Avoid side effects or guard them explicitly.
- **Dynamic contributions** – if a module’s contribution depends on runtime state, compute it within the callback. Returning `None` skips stale results.
- **Async helpers** – callbacks can perform asynchronous work, but prefer cached or precomputed data to keep collection fast.
- **Testing** – add coverage similar to `tests/test_contribution_channels.py` for new channels or behaviors (ordering, filtering, failure handling).

---

## Example: Module Observability

1. **Module declares events**

   ```python
   coordinator.register_contributor(
       "observability.events",
       "module-hooks-streaming-ui",
       lambda: [
           "streaming-ui:content-block-start",
           "streaming-ui:content-block-end",
       ],
   )
   ```

2. **Logging module collects**

   ```python
   discovered = await coordinator.collect_contributions("observability.events")
   for contribution in discovered:
       for event_name in contribution or []:
           register_handler(event_name)
   ```

3. **Runtime**

   Modules emit events via the hook system. Because contributions are retrieved dynamically, newly mounted modules are included automatically on the next collection.

---

## Summary

Contribution channels provide:

- Safe aggregation where each module keeps ownership of its data.
- Pull-based coordination that avoids shared mutable state.
- Extensibility without kernel changes; new channels register through the same API.

Use these contracts whenever multiple modules need to publish independent data to a shared consumer.
