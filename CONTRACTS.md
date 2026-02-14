# amplifier-core Contracts

> **Purpose:** This document is the authoritative Rust↔Python type mapping for coding
> agents working on either side of the boundary. Read this before modifying any shared
> type, trait/protocol, or error.

## Naming Convention

| Concept | Rust | Python |
|---------|------|--------|
| Data model | `struct Foo` with `#[derive(Serialize, Deserialize)]` | `class Foo(BaseModel)` (Pydantic v2) |
| Interface | `trait Bar` (async, `dyn`-safe) | `class Bar(Protocol)` (structural typing) |
| Enum (string) | `enum Baz { Variant }` with `#[serde(rename_all = "snake_case")]` | `Literal["variant"]` |
| Tagged union | `enum E { A { .. }, B { .. } }` with `#[serde(tag = "type")]` | Discriminated `Union[A, B]` |
| Error | `Result<T, E>` with `thiserror` | `T` (raises exception) |
| Optional | `Option<T>` | `T \| None` |
| List | `Vec<T>` | `list[T]` |
| Map | `HashMap<K, V>` | `dict[K, V]` |
| JSON blob | `serde_json::Value` | `dict[str, Any]` |

**Serialization boundary:** All data crosses the PyO3 bridge as JSON (via
`serde_json::to_string` → `json.loads` and vice versa). Field names must be
identical on both sides. Rust uses `#[serde(rename = "...")]` where the Rust
field name differs from the JSON key.

---

## Trait ↔ Protocol Mapping

| Rust Trait | Location (Rust) | Python Protocol | Location (Python) | Notes |
|-----------|-----------------|----------------|-------------------|-------|
| `Tool` | `crates/amplifier-core/src/traits.rs` | `Tool` | `interfaces.py` | Rust `execute` takes `Value`; Python takes `dict[str, Any]`. Rust adds `get_spec() -> ToolSpec`. |
| `Provider` | `crates/amplifier-core/src/traits.rs` | `Provider` | `interfaces.py` | 1:1 — `name`, `get_info`, `list_models`, `complete`, `parse_tool_calls`. |
| `Orchestrator` | `crates/amplifier-core/src/traits.rs` | `Orchestrator` | `interfaces.py` | Rust passes `hooks`/`coordinator` as `Value`; Python passes typed objects + `**kwargs`. |
| `ContextManager` | `crates/amplifier-core/src/traits.rs` | `ContextManager` | `interfaces.py` | 1:1 — `add_message`, `get_messages_for_request`, `get_messages`, `set_messages`, `clear`. |
| `HookHandler` | `crates/amplifier-core/src/traits.rs` | `HookHandler` | `interfaces.py` | Rust: `handle(event, data)`; Python: `__call__(event, data)`. |
| `ApprovalProvider` | `crates/amplifier-core/src/traits.rs` | `ApprovalProvider` | `interfaces.py` | 1:1 — `request_approval(ApprovalRequest) -> ApprovalResponse`. |

---

## Data Model Mapping

### Core Models (`models.rs` ↔ `models.py`)

| Rust Struct/Enum | Python Class | Serialization | Notes |
|-----------------|-------------|---------------|-------|
| `HookResult` | `HookResult` (BaseModel) | JSON round-trip at PyO3 boundary | Field-for-field match. Rust `HookAction` enum ↔ Python `Literal` strings. |
| `HookAction` | `Literal["continue","deny","modify","inject_context","ask_user"]` | `serde(rename_all = "snake_case")` | Enum variants map 1:1 to string literals. |
| `ToolResult` | `ToolResult` (BaseModel) | JSON round-trip | 1:1. Python adds `__str__()` and `get_serialized_output()` convenience methods. |
| `ModelInfo` | `ModelInfo` (BaseModel) | JSON round-trip | 1:1. |
| `ConfigField` | `ConfigField` (BaseModel) | JSON round-trip | Rust `default_value` ↔ Python `default`. |
| `ConfigFieldType` | `Literal["text","secret","choice","boolean"]` | snake_case | |
| `ProviderInfo` | `ProviderInfo` (BaseModel) | JSON round-trip | 1:1. |
| `ModuleInfo` | `ModuleInfo` (BaseModel) | JSON round-trip | Rust `module_type` serializes as JSON key `"type"`. |
| `ModuleType` | `Literal["orchestrator","provider","tool","context","hook","resolver"]` | snake_case | |
| `SessionStatus` | `SessionStatus` (BaseModel) | JSON round-trip | 1:1. Rust `started_at` is `String`; Python is `datetime`. |
| `SessionState` | `Literal["running","completed","failed","cancelled"]` | snake_case | |
| `ContextInjectionRole` | `Literal["system","user","assistant"]` | snake_case | |
| `ApprovalDefault` | `Literal["allow","deny"]` | snake_case | |
| `UserMessageLevel` | `Literal["info","warning","error"]` | snake_case | |
| `ApprovalRequest` | `ApprovalRequest` (BaseModel) | JSON round-trip | 1:1. Python has `model_post_init` validation. |
| `ApprovalResponse` | `ApprovalResponse` (BaseModel) | JSON round-trip | 1:1. |

### Message Models (`messages.rs` ↔ `message_models.py`)

| Rust Type | Python Type | Serialization | Notes |
|----------|-------------|---------------|-------|
| `Message` | `Message` (BaseModel) | JSON round-trip | 1:1 fields. |
| `ContentBlock` (tagged enum) | `ContentBlockUnion` (discriminated Union) | `serde(tag = "type")` | Rust variants = Python separate BaseModel classes. |
| `ToolSpec` | `ToolSpec` (BaseModel) | JSON round-trip | 1:1. |
| `ChatRequest` | `ChatRequest` (BaseModel) | JSON round-trip | 1:1. |
| `ToolCall` | `ToolCall` (BaseModel) | JSON round-trip | 1:1. |
| `Usage` | `Usage` (BaseModel) | JSON round-trip | 1:1. |
| `Degradation` | `Degradation` (BaseModel) | JSON round-trip | 1:1. |
| `ChatResponse` | `ChatResponse` (BaseModel) | JSON round-trip | 1:1. |
| `ResponseFormat` (tagged enum) | `ResponseFormat` (Union) | `serde(tag = "type")` | Text/Json/JsonSchema variants match. |
| `Role` (enum) | `Literal["system","developer","user","assistant","function","tool"]` | snake_case | |
| `Visibility` (enum) | `Literal["internal","developer","user"]` | snake_case | |

### Content Block Variants

Python uses separate classes for each content block type. Rust uses variants of the `ContentBlock` enum.

| Rust Variant | Python Class | Location (Python) |
|-------------|-------------|-------------------|
| `ContentBlock::Text` | `TextBlock` | `message_models.py` |
| `ContentBlock::Thinking` | `ThinkingBlock` | `message_models.py` |
| `ContentBlock::RedactedThinking` | `RedactedThinkingBlock` | `message_models.py` |
| `ContentBlock::ToolCall` | `ToolCallBlock` | `message_models.py` |
| `ContentBlock::ToolResult` | `ToolResultBlock` | `message_models.py` |
| `ContentBlock::Image` | `ImageBlock` | `message_models.py` |
| `ContentBlock::Reasoning` | `ReasoningBlock` | `message_models.py` |

### Streaming Content Models (`content_models.py`)

| Rust Equivalent | Python Class | Notes |
|----------------|-------------|-------|
| `ContentBlock::Text` variant | `TextContent` (dataclass) | |
| `ContentBlock::Thinking` variant | `ThinkingContent` (dataclass) | |
| `ContentBlock::ToolCall` variant | `ToolCallContent` (dataclass) | |
| `ContentBlock::ToolResult` variant | `ToolResultContent` (dataclass) | |
| `ContentBlockType` enum | `ContentBlockType` (str Enum) | Text/Thinking/ToolCall/ToolResult |

---

## Behavioral Type Mapping

These are the core engine types that the Rust kernel implements and the PyO3
bridge exposes.

| Rust Type | PyO3 Wrapper | Python Name | Python Original | Notes |
|----------|-------------|-------------|----------------|-------|
| `Session` | `RustSession` | `AmplifierSession` (M7) | `session.py:AmplifierSession` | Rust is leaner: no `ModuleLoader`, no auto-load in `initialize()`. |
| `Coordinator` | `RustCoordinator` | `ModuleCoordinator` (M7) | `coordinator.py:ModuleCoordinator` | Rust has core mount/get/hooks/cancel. Python adds `process_hook_result`, session back-refs, budget limits. |
| `HookRegistry` | `RustHookRegistry` | `HookRegistry` (M7) | `hooks.py:HookRegistry` | 1:1 core API: `register`, `emit`, `unregister`, `list_handlers`. |
| `CancellationToken` | `RustCancellationToken` | `CancellationToken` (M7) | `cancellation.py:CancellationToken` | 1:1: `state`, `is_cancelled`, `request_graceful`, `request_immediate`, `reset`. |
| `CancellationState` | *(stays Python)* | `CancellationState` | `cancellation.py:CancellationState` | Simple enum — no Rust bridge needed. |
| `SessionConfig` | *(internal)* | *(dict)* | *(inline in `__init__`)* | Rust-specific typed config. Python uses raw dict. |

> **(M7)** = switchover from Python to Rust implementation planned for Milestone 7.
> Currently both implementations coexist: Python types are the default exports,
> Rust types are available as `RustSession`, `RustHookRegistry`, etc.

---

## Error Mapping

### LLM/Provider Errors (`errors.rs:ProviderError` ↔ `llm_errors.py`)

| Rust Variant | Python Exception | Notes |
|-------------|-----------------|-------|
| `ProviderError::RateLimit` | `RateLimitError` | Rust has `retry_after: Option<f64>` field. |
| `ProviderError::Authentication` | `AuthenticationError` | |
| `ProviderError::ContextLength` | `ContextLengthError` | |
| `ProviderError::ContentFilter` | `ContentFilterError` | |
| `ProviderError::InvalidRequest` | `InvalidRequestError` | |
| `ProviderError::Unavailable` | `ProviderUnavailableError` | |
| `ProviderError::Timeout` | `LLMTimeoutError` | |
| `ProviderError::Other` | `LLMError` (base class) | Catch-all. |

### Session Errors (`errors.rs:SessionError`)

| Rust Variant | Python Equivalent | Notes |
|-------------|------------------|-------|
| `SessionError::NotInitialized` | `RuntimeError("No orchestrator...")` | Python raises generic `RuntimeError`. |
| `SessionError::ConfigMissing` | `ValueError("Configuration must specify...")` | |
| `SessionError::AlreadyCompleted` | *(no equivalent)* | Rust-only guard. |
| `SessionError::Other` | *(various RuntimeErrors)* | |

### Hook Errors (`errors.rs:HookError`)

| Rust Variant | Python Equivalent | Notes |
|-------------|------------------|-------|
| `HookError::HandlerFailed` | *(logged, not raised)* | Python catches silently. |
| `HookError::Timeout` | `TimeoutError` (via `asyncio.wait_for`) | |
| `HookError::Other` | *(generic Exception)* | |

### Rust-Only Error Types

| Rust Type | Notes |
|----------|-------|
| `AmplifierError` | Top-level wrapper enum. Python has no unified equivalent. |
| `ToolError` | Python represents tool errors as `ToolResult(success=False, error={...})`. |
| `ContextError` | Python context managers raise generic exceptions. |

---

## Python-Only Types (Not Ported to Rust)

These types stay as Python by design — they are app-layer concerns, not kernel.

| Python Type | Location | Why Not Ported |
|------------|----------|---------------|
| `ModuleLoader` | `loader.py` | Module loading/discovery is app-layer. |
| `ModuleValidationError` | `loader.py` | Validation framework stays Python. |
| `ApprovalSystem` | `approval.py` | App-layer approval policy. |
| `DisplaySystem` | `display.py` | App-layer UX. |
| `validation/` package | `validation/` | Structural + behavioral test framework stays Python. |
| `testing` module | `testing.py` | Test utilities (`MockTool`, `TestCoordinator`, etc.) stay Python. |
| `pytest_plugin` | `pytest_plugin.py` | Pytest integration stays Python. |
| `cli` | `cli.py` | CLI entry point stays Python. |

---

## Event Constants

Rust defines event names in `crates/amplifier-core/src/events.rs`. Python defines
them as class-level constants on `HookRegistry` in `hooks.py`. They must be
identical strings:

| Event | Value |
|-------|-------|
| Session start | `"session:start"` |
| Session end | `"session:end"` |
| Turn start | `"turn:start"` |
| Turn end | `"turn:end"` |
| Tool pre | `"tool:pre"` |
| Tool post | `"tool:post"` |
| LLM pre | `"llm:pre"` |
| LLM post | `"llm:post"` |
| Context compaction | `"context:compaction"` |

---

## Rules for Modifying Shared Types

1. **Field names must match.** If you add a field to a Rust struct, add the
   identical field to the Python BaseModel (and vice versa).

2. **Enum variants must match.** Rust `snake_case` serde names = Python
   `Literal` string values.

3. **JSON is the contract.** Both sides must produce identical JSON for the
   same logical value. Test with round-trip serialization.

4. **Update this document.** Any change to a shared type must be reflected
   here. CI will eventually enforce this.

5. **Method names must match** for PyO3-bridged types (`Session`, `Coordinator`,
   `HookRegistry`, `CancellationToken`). The Python-visible name is set by
   `#[pyclass(name = "...")]` and `#[pymethods]`.
