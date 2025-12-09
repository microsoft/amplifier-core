---
contract_type: module_specification
module_type: provider
contract_version: 1.0.0
last_modified: 2025-01-29
related_files:
  - path: amplifier_core/interfaces.py#Provider
    relationship: protocol_definition
    lines: 54-119
  - path: amplifier_core/message_models.py
    relationship: request_response_models
  - path: amplifier_core/content_models.py
    relationship: event_content_types
  - path: amplifier_core/models.py#ProviderInfo
    relationship: metadata_models
  - path: ../specs/PROVIDER_SPECIFICATION.md
    relationship: detailed_spec
  - path: ../specs/MOUNT_PLAN_SPECIFICATION.md
    relationship: configuration
  - path: ../specs/CONTRIBUTION_CHANNELS.md
    relationship: observability
  - path: amplifier_core/testing.py
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

**Source**: `amplifier_core/interfaces.py` lines 54-119

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

## Entry Point Pattern

### mount() Function

```python
async def mount(coordinator: ModuleCoordinator, config: dict) -> Provider | Callable | None:
    """
    Initialize and return provider instance.

    Returns:
        - Provider instance (registered automatically)
        - Cleanup callable (for resource cleanup on unmount)
        - None for graceful degradation (e.g., missing API key)
    """
    api_key = config.get("api_key") or os.environ.get("MY_API_KEY")
    if not api_key:
        logger.warning("No API key - provider not mounted")
        return None

    provider = MyProvider(api_key=api_key, config=config)
    await coordinator.mount("providers", provider, name="my-provider")

    async def cleanup():
        await provider.client.close()

    return cleanup
```

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

- [ ] Graceful degradation on missing config (return None from mount)
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
