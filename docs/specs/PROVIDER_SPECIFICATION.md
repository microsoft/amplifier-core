---
spec_type: module_implementation
module_type: provider
last_modified: 2025-01-29
related_contracts:
  - path: ../contracts/PROVIDER_CONTRACT.md
    relationship: contract_hub
---

# Provider Specification

Providers translate between the platform's unified message format and vendor-specific LLM APIs.

## Source of Truth

The authoritative definitions live in code:

| What | Where | Key Classes |
|------|-------|-------------|
| Provider Protocol | `amplifier_core/interfaces.py` | `Provider` |
| Request/Response | `amplifier_core/message_models.py` | `ChatRequest`, `ChatResponse`, `Message` |
| Content Blocks (envelope) | `amplifier_core/message_models.py` | `TextBlock`, `ThinkingBlock`, `ToolCallBlock`, etc. |
| Content Blocks (events) | `amplifier_core/content_models.py` | `ContentBlock`, `TextContent`, `ThinkingContent`, `ToolCallContent` |
| Tool Calls | `amplifier_core/message_models.py` | `ToolCall` (used in `ChatResponse.tool_calls` and `parse_tool_calls()`) |
| Metadata Models | `amplifier_core/models.py` | `ProviderInfo`, `ModelInfo`, `ConfigField` |

**Note**: `message_models.py` provides Pydantic models for request/response envelopes. `content_models.py` provides dataclass types for event emission and streaming UI.

**Read the code docstrings.** This spec covers implementation guidance that code cannot express.

## Protocol Summary

```python
class Provider(Protocol):
    @property
    def name(self) -> str: ...
    def get_info(self) -> ProviderInfo: ...
    async def list_models(self) -> list[ModelInfo]: ...
    async def complete(self, request: ChatRequest, **kwargs) -> ChatResponse: ...
    def parse_tool_calls(self, response: ChatResponse) -> list[ToolCall]: ...
```

## Module Entry Point

Providers are loaded via Python entry points. See [MOUNT_PLAN_SPECIFICATION.md](MOUNT_PLAN_SPECIFICATION.md) for how modules are configured.

### Required: `mount()` Function

```python
async def mount(coordinator: ModuleCoordinator, config: dict) -> Provider | None:
    """
    Initialize and return provider instance.

    Returns None for graceful degradation (e.g., missing API key).
    """
    api_key = config.get("api_key") or os.environ.get("MY_API_KEY")
    if not api_key:
        logger.warning("No API key - provider not mounted")
        return None

    provider = MyProvider(api_key=api_key, config=config, coordinator=coordinator)
    await coordinator.mount("providers", provider, name="my-provider")

    # Optional: Register cleanup
    async def cleanup():
        await provider.client.close()
    return cleanup
```

### Required: Entry Point Registration

```toml
# pyproject.toml
[project.entry-points."amplifier.modules"]
my-provider = "my_provider:mount"
```

## Implementation Requirements

### Content Preservation (Critical)

All content block types must round-trip without loss. Key gotchas:

| Block | Preservation Requirement |
|-------|-------------------------|
| `ThinkingBlock` | Preserve `signature` field (required for multi-turn) |
| `ReasoningBlock` | Preserve `content` and `summary` arrays |
| `ToolCallBlock` | Preserve `id` for result correlation |

### Role Conversion

```
Platform Role    → Common Vendor Mapping
─────────────────────────────────────────
system           → system / instructions parameter
developer        → user (XML-wrapped for context separation)
user             → user
assistant        → assistant
tool             → user (with tool_result blocks)
```

### Tool Sequence Validation

Validate that all `ToolCallBlock` entries have corresponding `ToolResultBlock` with matching `tool_call_id`. If missing:

1. Log warning (indicates context management bug)
2. Either synthesize placeholder result OR let API error

### Auto-Continuation

Some APIs return truncated responses. Handle transparently:

```python
while response.status == "incomplete" and iterations < MAX:
    response = await self._continue(accumulated_output)
    accumulated_output.extend(response.output)
```

## Configuration

### Via Mount Plan

```yaml
providers:
  - module: my-provider
    source: git+https://github.com/org/my-provider@main
    config:
      api_key: "${MY_API_KEY}"
      default_model: model-v1
      debug: true
```

### ConfigField for Interactive Setup

Providers declare configuration needs via `get_info().config_fields`:

```python
ConfigField(
    id="api_key",
    field_type="secret",
    env_var="MY_API_KEY",
    prompt="Enter API key",
)
```

**Conditional fields**: Use `show_when` and `requires_model` for model-dependent configuration.

## Observability

### Event Emission

Orchestrators emit standard `provider:request/response/error` events. Providers may emit additional events via contribution channels (see [CONTRIBUTION_CHANNELS.md](CONTRIBUTION_CHANNELS.md)):

```python
coordinator.register_contributor(
    "observability.events",
    "my-provider",
    lambda: ["my-provider:rate_limit", "my-provider:retry"]
)
```

### Debug Levels

Support via config flags:

| Flag | Events | Content |
|------|--------|---------|
| (default) | `llm:request`, `llm:response` | Summary only |
| `debug: true` | `llm:request:debug`, `llm:response:debug` | Truncated payloads |
| `debug: true, raw_debug: true` | `llm:request:raw`, `llm:response:raw` | Complete API I/O |

## Quick Reference Checklist

### Required

- [ ] Implement `Provider` protocol (5 methods)
- [ ] `mount()` function with entry point in pyproject.toml
- [ ] Preserve all content block types
- [ ] Report `Usage` (input/output/total tokens)

### Recommended

- [ ] Graceful degradation on missing config (return None from mount)
- [ ] Validate tool call/result sequences
- [ ] Support debug configuration flags
- [ ] Register cleanup function

### Content Block Reference

| Type | Key Fields | Notes |
|------|------------|-------|
| `text` | `text` | Standard text content |
| `thinking` | `thinking`, `signature` | Vendor thinking (signature critical) |
| `redacted_thinking` | `data` | Redacted by vendor policy |
| `reasoning` | `content`, `summary` | Reasoning chain (o-series style) |
| `tool_call` | `id`, `name`, `input` | Correlate with results via `id` |
| `tool_result` | `tool_call_id`, `output` | Must match `tool_call.id` |
| `image` | `source` | Vendor-specific source format |

All blocks support `visibility` field and `extra="allow"` for vendor extensions.

## Capabilities Taxonomy

Model capabilities are declared in `ModelInfo.capabilities` as a list of strings. To ensure consistency across providers, `amplifier_core.capabilities` defines well-known capability constants.

### Tier 1: Universal Capabilities

Core capabilities that most LLM providers can report on:

| Constant | String Value | Description |
|----------|-------------|-------------|
| `TOOLS` | `"tools"` | Function/tool calling support |
| `STREAMING` | `"streaming"` | Streaming response support |
| `THINKING` | `"thinking"` | Extended reasoning / chain-of-thought (see naming note below) |
| `VISION` | `"vision"` | Image input understanding |
| `JSON_MODE` | `"json_mode"` | Structured JSON output mode |

### Tier 2: Specialized Capabilities

Extended capabilities for specialized model features:

| Constant | String Value | Description |
|----------|-------------|-------------|
| `FAST` | `"fast"` | Optimized for low latency |
| `CODE_EXECUTION` | `"code_execution"` | Server-side code execution |
| `WEB_SEARCH` | `"web_search"` | Built-in web search |
| `DEEP_RESEARCH` | `"deep_research"` | Extended multi-step research |
| `LOCAL` | `"local"` | Runs locally (e.g., Ollama) |
| `AUDIO` | `"audio"` | Audio input/output support |
| `IMAGE_GENERATION` | `"image_generation"` | Image generation support |
| `COMPUTER_USE` | `"computer_use"` | Desktop/browser automation |
| `EMBEDDINGS` | `"embeddings"` | Text embedding generation |
| `LONG_CONTEXT` | `"long_context"` | Extended context window support |
| `BATCH` | `"batch"` | Batch processing API support |

### Naming Decision: `thinking` is Canonical

The canonical capability string for extended reasoning is **`"thinking"`**, not `"reasoning"`. This applies regardless of vendor terminology:

- Anthropic's "extended thinking" → `"thinking"` ✓
- OpenAI's "reasoning" (o-series) → should be mapped to `"thinking"` ✓

Providers **SHOULD** map vendor-specific terminology to `"thinking"` when populating `ModelInfo.capabilities`. The `MODEL_CLASS_CAPABILITIES` mapping in `amplifier_core.capabilities` handles normalization of provider-reported model classes.

### Extensibility

Providers **MAY** include capability strings not in the well-known set. The taxonomy is intentionally open — `ALL_WELL_KNOWN_CAPABILITIES` is a `frozenset` for validation, not a closed enum. Custom capabilities should use descriptive lowercase strings with underscores (e.g., `"my_custom_feature"`).

### Cost Tiers

Cost tiers categorize models by relative pricing. Providers **SHOULD** set `metadata["cost_tier"]` on `ModelInfo` to one of:

| Constant | String Value | Description |
|----------|-------------|-------------|
| `COST_TIER_FREE` | `"free"` | No-cost models (local, free-tier) |
| `COST_TIER_LOW` | `"low"` | Budget-friendly (e.g., Haiku-class) |
| `COST_TIER_MEDIUM` | `"medium"` | Standard pricing (e.g., Sonnet-class) |
| `COST_TIER_HIGH` | `"high"` | Premium pricing (e.g., Opus-class) |
| `COST_TIER_EXTREME` | `"extreme"` | Highest-cost (e.g., deep research) |

All cost tier constants are available in `amplifier_core.capabilities` and collected in `ALL_COST_TIERS` (`frozenset`).
