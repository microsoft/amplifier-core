---
contract_type: module_specification
module_type: provider
contract_version: 1.0.0
last_modified: 2026-10-22
related_files:
  - path: ../../python/amplifier_core/interfaces.py#Provider
    relationship: protocol_definition
  - path: ../../python/amplifier_core/message_models.py
    relationship: request_response_models
  - path: ../../python/amplifier_core/content_models.py
    relationship: event_content_types
  - path: ../../python/amplifier_core/models.py#ProviderInfo
    relationship: metadata_models
  - path: ../specs/PROVIDER_SPECIFICATION.md
    relationship: detailed_spec
  - path: ../specs/MOUNT_PLAN_SPECIFICATION.md
    relationship: configuration
  - path: ../specs/CONTRIBUTION_CHANNELS.md
    relationship: observability
  - path: ../../python/amplifier_core/testing.py
    relationship: test_utilities
canonical_example: https://github.com/microsoft/amplifier-module-provider-anthropic
---

# Provider Contract

Providers translate between Amplifier's unified message format and vendor-specific LLM APIs.

---

## Detailed Specification

**See [PROVIDER_SPECIFICATION.md](../specs/PROVIDER_SPECIFICATION.md)** for complete implementation guidance including:

- Protocol summary and method signatures
- Content block preservation requirements
- Role conversion patterns
- Auto-continuation handling
- Debug levels and observability

This contract document provides the quick-reference essentials. The specification contains the full details.

---

## Protocol Definition

**Source**: `amplifier_core/interfaces.py` → `class Provider(Protocol)`

```python
@runtime_checkable
class Provider(Protocol):
    @property
    def name(self) -> str: ...

    def get_info(self) -> ProviderInfo: ...

    async def list_models(self) -> list[ModelInfo]: ...

    async def complete(self, request: ChatRequest, **kwargs) -> ChatResponse: ...

    def parse_tool_calls(self, response: ChatResponse) -> list[ToolCall]: ...
```

**Note**: `ToolCall` is from `amplifier_core.message_models` (see [REQUEST_ENVELOPE_V1](../specs/PROVIDER_SPECIFICATION.md) for details)

---

## ModelInfo Extensions (Model Class Routing)

The `list_models()` method returns `list[ModelInfo]`. Beyond the required fields (`id`, `display_name`, `context_window`, `max_output_tokens`), ModelInfo supports optional extension fields for model class routing and cost-aware selection:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `cost_per_input_token` | `float \| None` | `None` | Cost per input token in USD (e.g., `3e-6` for $3/MTok) |
| `cost_per_output_token` | `float \| None` | `None` | Cost per output token in USD |
| `metadata` | `dict[str, Any]` | `{}` | Extensible metadata bag for cost tier, model class, provider-specific tags |

### Cost Fields

Providers **SHOULD** populate `cost_per_input_token` and `cost_per_output_token` when pricing information is available. These enable cost-aware model selection and budget tracking.

### Metadata: `cost_tier`

Providers **SHOULD** set `metadata["cost_tier"]` to one of the well-known cost tier strings:

| Tier | Description |
|------|-------------|
| `free` | No-cost models (local, free-tier) |
| `low` | Budget-friendly models (e.g., Haiku-class) |
| `medium` | Standard pricing (e.g., Sonnet-class) |
| `high` | Premium pricing (e.g., Opus-class) |
| `extreme` | Highest-cost models (e.g., deep research) |

### Capabilities

Providers **SHOULD** populate the `capabilities` list using well-known constants from `amplifier_core.capabilities`. See the [Capabilities Taxonomy](../specs/PROVIDER_SPECIFICATION.md#capabilities-taxonomy) in the Provider Specification for the full list.

### Backward Compatibility

All extension fields are optional with sensible defaults. Existing providers that do not populate these fields continue to work unchanged — they simply won't participate in cost-aware or capability-based routing.

---

## Entry Point Pattern

Current session initialization requires a new mounted provider object, including
the named-account rules in [provider initialization failures](../CAPABILITY_REGISTRY.md#provider-initialization-failures).
The entrypoint must explicitly mount that object. Returning a `Provider` does not
register it automatically. Returning `None` without a new mounted provider,
including when required configuration is missing, is a provider mount failure.
The [capability-only lifecycle proposal](../plans/2026-10-22-capability-only-provider-lifecycle-design.md)
is a draft for discussion, not an implemented exception.

### mount() Function

```python
import asyncio
import os
from collections.abc import Awaitable, Callable


async def mount(
    coordinator: ModuleCoordinator, config: dict
) -> Callable[[], Awaitable[None]] | None:
    """Mount a new provider explicitly; return its resource cleanup callback."""
    api_key = config.get("api_key") or os.environ.get("MY_API_KEY")
    if not api_key:
        raise ValueError("Configure api_key or MY_API_KEY for my-provider")

    provider = MyProvider(api_key=api_key, config=config)
    try:
        await coordinator.mount("providers", provider, name="my-provider")
    except BaseException as primary:
        # No returned handle exists yet: unwind this attempt's own allocation.
        try:
            await provider.client.close()
        except asyncio.CancelledError as cancellation:
            raise cancellation from primary
        except BaseException as cleanup_error:
            raise primary from cleanup_error
        raise

    async def cleanup():
        await provider.client.close()

    return cleanup
```

An ordinary provider may return `None` **after** mounting a new provider when it
owns no resource requiring cleanup. For a named account, mount the new object at
the module's default slot; Core validates it before remapping to `instance_id`.
Do not reuse a sibling account's provider to satisfy this check.

If initialization fails or is cancelled before returning cleanup, the module owns
unwinding its partial allocations and registrations. It must preserve the
initiating failure when reporting any additional cleanup failure. This example
allocates only a client; modules publishing other capabilities must also undo
their own registrations. The draft above proposes changes to **Core's** handling
of a returned cleanup handle after a rejected mount; those changes are not
available in Core 2.0.2.

### pyproject.toml

```toml
[project.entry-points."amplifier.modules"]
my-provider = "my_provider:mount"
```

---

## Configuration

Providers receive configuration via Mount Plan:

```yaml
providers:
  - module: my-provider
    source: git+https://github.com/org/my-provider@main
    config:
      api_key: "${MY_API_KEY}"
      default_model: model-v1
      debug: true
```

See [MOUNT_PLAN_SPECIFICATION.md](../specs/MOUNT_PLAN_SPECIFICATION.md) for full schema.

---

## Observability

Register custom events via contribution channels:

```python
coordinator.register_contributor(
    "observability.events",
    "my-provider",
    lambda: ["my-provider:rate_limit", "my-provider:retry"]
)
```

See [CONTRIBUTION_CHANNELS.md](../specs/CONTRIBUTION_CHANNELS.md) for the pattern.

### `request_id` — Call Correlation (Kernel-Supplied)

Providers **SHOULD NOT** do anything. The kernel stamps `request_id` onto
`llm:request` on the emit path and echoes the same value onto the matching
`llm:response` (and onto `provider:error` when the call fails or times out),
so a consumer can pair a call's events by identity instead of by position.

Two requirements fall on providers:

1. **Emit `llm:request` before the call and `llm:response`/`provider:error`
   after it, from the same async task.** Correlation is scoped by
   `contextvars`, which is what keeps two concurrent calls apart. A provider
   that emits the response from a *different* task (e.g. a streaming callback
   scheduled separately) must pass `request_id` explicitly instead.

2. **Do not overwrite it.** If a provider has a meaningful upstream id of its
   own it may put `request_id` in the event data — an explicit value always
   wins and the kernel adopts it for the rest of the call. Otherwise leave the
   field alone.

Out-of-process (gRPC/WASM) providers do not share the kernel's Python context
and **MUST** supply `request_id` themselves on both events if they want their
calls correlated.

See [HOOKS_API.md](../HOOKS_API.md#request_id--llm-call-correlation) for the
full policy and its backward-compatibility guarantees.

### `llm:response` Event — `usage` Payload Schema

Providers **MUST** emit `llm:response` with the following `usage` payload. Key names are normative — derived from the kernel `Usage` struct (`crates/amplifier-core/src/messages.rs`):

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `input_tokens` | `int` | **MUST** | Total input tokens — gross total (fresh + cache_read combined) |
| `output_tokens` | `int` | **MUST** | Total output tokens generated |
| `cache_read_tokens` | `int` | SHOULD (when non-zero) | Tokens served from prompt cache |
| `cache_write_tokens` | `int` | SHOULD (when non-zero) | Tokens written to prompt cache (billed on top of gross) |

**DO NOT use:** `"input"`, `"output"`, `"input_tokens_used"`, `"completion_tokens"`, or any other variant. These break consumers silently.

`total_tokens` is not included in the event payload. Consumers that need it can derive it as `input_tokens + output_tokens` from the event `usage` dict.

**Reference emit pattern** (build `ChatResponse` before emitting):

```python
chat_response = self._convert_to_chat_response(response)  # build first

event_usage: dict[str, Any] = {
    "input_tokens": chat_response.usage.input_tokens,
    "output_tokens": chat_response.usage.output_tokens,
}
if chat_response.usage.cache_read_tokens is not None:
    event_usage["cache_read_tokens"] = chat_response.usage.cache_read_tokens
if chat_response.usage.cache_write_tokens is not None:
    event_usage["cache_write_tokens"] = chat_response.usage.cache_write_tokens

await coordinator.hooks.emit("llm:response", {
    "provider": self.name,
    "model": model,
    "status": "ok",
    "duration_ms": elapsed_ms,
    "usage": event_usage,
})

return chat_response  # return the already-built response
```

---

## Canonical Example

**Reference implementation**: [amplifier-module-provider-anthropic](https://github.com/microsoft/amplifier-module-provider-anthropic)

Study this module for:
- Complete Provider protocol implementation
- Content block handling patterns
- Configuration and credential management
- Debug logging integration

---

## Validation Checklist

### Required

- [ ] Implements all 5 Provider protocol methods
- [ ] `mount()` function with entry point in pyproject.toml
- [ ] Preserves all content block types (especially `signature` in ThinkingBlock)
- [ ] Reports `Usage` (input/output/total tokens)
- [ ] Returns `ChatResponse` from `complete()`

### Recommended

- [ ] Explicit failure on missing required configuration; unwind owned partial allocations on failure/cancellation
- [ ] Validates tool call/result sequences
- [ ] Supports debug configuration flags
- [ ] Registers cleanup function for resource management
- [ ] Registers observability events via contribution channels

---

## Testing

Use test utilities from `amplifier_core/testing.py`:

```python
from amplifier_core.testing import TestCoordinator, create_test_coordinator

@pytest.mark.asyncio
async def test_provider_mount():
    coordinator = create_test_coordinator()
    cleanup = await mount(coordinator, {"api_key": "test-key"})

    assert "my-provider" in coordinator.get_mounted("providers")

    if cleanup:
        await cleanup()
```

---

## Quick Validation Command

```bash
# Structural validation
amplifier module validate ./my-provider --type provider
```

---

**Related**: [PROVIDER_SPECIFICATION.md](../specs/PROVIDER_SPECIFICATION.md) | [README.md](README.md)
