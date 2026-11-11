---
contract_type: module_specification
module_type: tool
contract_version: 1.0.0
last_modified: 2025-01-29
related_files:
  - path: ../../python/amplifier_core/interfaces.py#Tool
    relationship: protocol_definition
  - path: ../../python/amplifier_core/models.py#ToolResult
    relationship: result_model
  - path: ../../python/amplifier_core/message_models.py#ToolCall
    relationship: invocation_model
  - path: ../specs/MOUNT_PLAN_SPECIFICATION.md
    relationship: configuration
  - path: ../../python/amplifier_core/testing.py#MockTool
    relationship: test_utilities
canonical_example: https://github.com/microsoft/amplifier-module-tool-filesystem
---

# Tool Contract

Tools provide capabilities that agents can invoke during execution.

---

## Purpose

Tools extend agent capabilities beyond pure conversation:
- **Filesystem operations** - Read, write, edit files
- **Command execution** - Run shell commands
- **Web access** - Fetch URLs, search
- **Task delegation** - Spawn sub-agents
- **Custom capabilities** - Domain-specific operations

---

## Protocol Definition

**Source**: `amplifier_core/interfaces.py` → `class Tool(Protocol)`

```python
@runtime_checkable
class Tool(Protocol):
    @property
    def name(self) -> str:
        """Tool name for invocation."""
        ...

    @property
    def description(self) -> str:
        """Human-readable tool description."""
        ...

    @property
    def input_schema(self) -> dict[str, Any]:
        """JSON Schema describing the tool's input parameters.

        Returns an empty dict by default for backward compatibility
        with tools that predate this convention.
        """
        return {}

    async def execute(self, input: dict[str, Any]) -> ToolResult:
        """
        Execute tool with given input.

        Args:
            input: Tool-specific input parameters

        Returns:
            Tool execution result
        """
        ...
```

> **Note:** `input_schema` has a concrete default (`return {}`) and is excluded from
> `isinstance()` structural checks so that tools written before this field was introduced
> continue to satisfy the protocol without modification.  Callers that need the schema
> should always use `getattr(tool, "input_schema", {})` for maximum compatibility.

---

## Data Models

### ToolCall (Input)

**Source**: `amplifier_core/message_models.py`

```python
class ToolCall(BaseModel):
    id: str                    # Unique ID for correlation
    name: str                  # Tool name to invoke
    arguments: dict[str, Any]  # Tool-specific parameters
```

### ToolResult (Output)

**Source**: `amplifier_core/models.py`

```python
class ToolResult(BaseModel):
    success: bool = True              # Whether execution succeeded
    output: Any | None = None         # Tool output (typically str or dict)
    error: dict[str, Any] | None = None  # Error details if failed
```

---

## Entry Point Pattern

### mount() Function

Session initialization checks source-resolved Python tools on their actual mounted
instances after registering returned cleanup; the loader does not mount a temporary
tool first. A runtime mount that returns `None`
without adding or replacing a tool is a successful no-op, including graceful
degradation and an idempotent remount. Explicit `ToolValidator.validate()` remains
a separate diagnostic and reports failure if it cannot obtain a tool or cleanup
callable. See [runtime admission and cleanup](../../CONTRACTS.md#source-resolved-python-tool-validation).

```python
async def mount(coordinator: ModuleCoordinator, config: dict) -> Tool | Callable | None:
    """
    Initialize and register tool.

    Returns:
        - Tool instance
        - Cleanup callable (for resource cleanup)
        - None for graceful degradation
    """
    tool = MyTool(config=config)
    await coordinator.mount("tools", tool, name="my-tool")
    return tool
```

### pyproject.toml

```toml
[project.entry-points."amplifier.modules"]
my-tool = "my_tool:mount"
```

---

## Implementation Requirements

### Name and Description

Tools must provide clear identification:

```python
class MyTool:
    @property
    def name(self) -> str:
        return "my_tool"  # Used for invocation

    @property
    def description(self) -> str:
        return "Performs specific action with given parameters."
```

**Best practices**:
- `name`: Short, snake_case, unique across mounted tools
- `description`: Clear explanation of what the tool does and expects

### execute() Method

Handle inputs and return structured results:

```python
async def execute(self, input: dict[str, Any]) -> ToolResult:
    try:
        # Validate input
        required_param = input.get("required_param")
        if not required_param:
            return ToolResult(
                success=False,
                error={"message": "required_param is required"}
            )

        # Do the work
        result = await self._do_work(required_param)

        return ToolResult(
            success=True,
            output=result
        )

    except Exception as e:
        return ToolResult(
            success=False,
            error={"message": str(e), "type": type(e).__name__}
        )
```

### Tool Schema (Optional but Recommended)

Provide JSON schema for input validation:

```python
def get_schema(self) -> dict:
    """Return JSON schema for tool input."""
    return {
        "type": "object",
        "properties": {
            "required_param": {
                "type": "string",
                "description": "Description of parameter"
            },
            "optional_param": {
                "type": "integer",
                "default": 10
            }
        },
        "required": ["required_param"]
    }
```

---

## Configuration

Tools receive configuration via Mount Plan:

```yaml
tools:
  - module: my-tool
    source: git+https://github.com/org/my-tool@main
    config:
      max_size: 1048576
      allowed_paths:
        - /home/user/projects
```

See [MOUNT_PLAN_SPECIFICATION.md](../specs/MOUNT_PLAN_SPECIFICATION.md) for full schema.

---

## Observability

Register lifecycle events:

```python
coordinator.register_contributor(
    "observability.events",
    "my-tool",
    lambda: ["my-tool:started", "my-tool:completed", "my-tool:error"]
)
```

Standard tool events emitted by orchestrators:
- `tool:pre` - Before tool execution
- `tool:post` - After successful execution
- `tool:error` - On execution failure

---

## Canonical Example

**Reference implementation**: [amplifier-module-tool-filesystem](https://github.com/microsoft/amplifier-module-tool-filesystem)

Study this module for:
- Tool protocol implementation
- Input validation patterns
- Error handling and result formatting
- Configuration integration

Additional examples:
- [amplifier-module-tool-bash](https://github.com/microsoft/amplifier-module-tool-bash) - Command execution
- [amplifier-module-tool-web](https://github.com/microsoft/amplifier-module-tool-web) - Web access

---

## Validation Checklist

### Required

- [ ] Implements Tool protocol (name, description, execute)
- [ ] `mount()` function with entry point in pyproject.toml
- [ ] Returns `ToolResult` from execute()
- [ ] Handles errors gracefully (returns success=False, doesn't crash)

### Recommended

- [ ] Provides JSON schema via `get_schema()`
- [ ] Validates input before processing
- [ ] Logs operations at appropriate levels
- [ ] Registers observability events

---

## Testing

Use test utilities from `amplifier_core/testing.py`:

```python
from amplifier_core.testing import TestCoordinator, MockTool

@pytest.mark.asyncio
async def test_tool_execution():
    tool = MyTool(config={})

    result = await tool.execute({
        "required_param": "value"
    })

    assert result.success
    assert result.error is None
```

### MockTool for Testing Orchestrators

```python
from amplifier_core.testing import MockTool

mock_tool = MockTool(
    name="test_tool",
    description="Test tool",
    return_value="mock result"
)

# After use
assert mock_tool.call_count == 1
assert mock_tool.last_input == {...}
```

---

## Quick Validation Command

```bash
# Structural validation
amplifier module validate ./my-tool --type tool
```

---

**Related**: [README.md](README.md) | [HOOK_CONTRACT.md](HOOK_CONTRACT.md)
