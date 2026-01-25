---
spec_type: api_reference
module_type: hook
last_modified: 2025-01-29
related_contracts:
  - path: contracts/HOOK_CONTRACT.md
    relationship: contract_hub
  - path: ../amplifier_core/interfaces.py#HookHandler
    relationship: protocol_definition
    lines: 205-220
---

# Hooks API Reference

Complete API documentation for Amplifier's hook system.

---

## Overview

Hooks are functions that execute at specific points in Amplifier's lifecycle, enabling observation, validation, feedback injection, and approval control. Hooks receive event data and return a `HookResult` indicating what action to take.

**Capabilities**:
- **Observe**: Monitor operations (logging, metrics, audit trails)
- **Block**: Prevent operations from proceeding (security, validation)
- **Modify**: Transform event data (preprocessing, enrichment)
- **Inject Context**: Add feedback to agent's conversation (automated correction)
- **Request Approval**: Ask user for permission (dynamic policies)
- **Control Output**: Hide verbose output, show targeted messages (clean UX)

---

## HookResult

The result returned by hook handlers to control execution flow.

### Class Definition

```python
from amplifier_core.models import HookResult

class HookResult(BaseModel):
    # Core action
    action: Literal["continue", "deny", "modify", "inject_context", "ask_user"]

    # Existing fields
    data: dict[str, Any] | None = None
    reason: str | None = None

    # Context injection
    context_injection: str | None = None
    context_injection_role: Literal["system", "user", "assistant"] = "system"
    ephemeral: bool = False

    # Approval gates
    approval_prompt: str | None = None
    approval_options: list[str] | None = None
    approval_timeout: float = 300.0
    approval_default: Literal["allow", "deny"] = "deny"

    # Output control
    suppress_output: bool = False
    user_message: str | None = None
    user_message_level: Literal["info", "warning", "error"] = "info"

    # Injection placement control
    append_to_last_tool_result: bool = False
```

### Actions

| Action | Behavior | Use Case |
|--------|----------|----------|
| `continue` | Proceed normally | Default action, operation continues |
| `deny` | Block operation | Validation failed, security violation |
| `modify` | Transform data | Preprocess input, enrich event data |
| `inject_context` | Add to agent's context | Provide feedback, enable correction loops |
| `ask_user` | Request approval | Dynamic permissions, high-risk operations |

### Action Precedence

When multiple handlers return different actions for the same event, they are resolved according to this precedence hierarchy (highest to lowest):

| Priority | Action | Type | Behavior |
|----------|--------|------|----------|
| 1 | `deny` | Blocking | Short-circuits immediately, no further handlers run |
| 2 | `ask_user` | Blocking | Requires user approval before proceeding |
| 3 | `inject_context` | Non-blocking | Adds context (multiple results are merged) |
| 4 | `modify` | Non-blocking | Chains data through handlers |
| 5 | `continue` | Non-blocking | Default pass-through |

**Key principle**: Blocking actions (`deny`, `ask_user`) always take precedence over non-blocking actions (`inject_context`, `modify`, `continue`). This ensures security gates cannot be silently bypassed by information-flow actions.

**Example scenario**:
- Handler A (priority 5) returns `ask_user` (approval required)
- Handler B (priority 10) returns `inject_context` (add context)
- **Result**: `ask_user` is returned (Handler A's blocking action takes precedence)

**Multiple `inject_context` results**: When multiple handlers return `inject_context`, their injections are merged into a single result. Settings (role, ephemeral, suppress_output) are taken from the first result.

### Fields

#### Core Fields

**`action`** (required)
- Type: `Literal["continue", "deny", "modify", "inject_context", "ask_user"]`
- Default: `"continue"`
- Description: Action to take after hook execution

**`data`** (optional)
- Type: `dict[str, Any] | None`
- Default: `None`
- Description: Modified event data (for `action="modify"`). Changes chain through subsequent handlers.

**`reason`** (optional)
- Type: `str | None`
- Default: `None`
- Description: Explanation for deny/modification. Shown to agent when operation is blocked.

#### Context Injection Fields

**`context_injection`** (optional)
- Type: `str | None`
- Default: `None`
- Description: Text to inject into agent's conversation context (for `action="inject_context"`). Agent sees this content and can respond to it. Default limit 10 KB per injection (configurable via `session.injection_size_limit`).
- Security: Size-limited, audited, tagged with source hook

**`context_injection_role`** (optional)
- Type: `Literal["system", "user", "assistant"]`
- Default: `"system"`
- Description: Role for injected message. `"system"` (default) for environmental feedback, `"user"` to simulate user input, `"assistant"` for agent self-talk.
- Recommendation: Use `"system"` for most cases

**`ephemeral`** (optional)
- Type: `bool`
- Default: `False`
- Description: If `True`, injection is temporary (only for current LLM call, not stored in conversation history). Use for transient state that updates frequently (todo reminders, live status). Orchestrator appends ephemeral injection to messages without storing in context.
- Use Cases: Todo state, live metrics, temporary warnings
- Not Recommended For: Persistent feedback, linter errors that need to stay visible

#### Approval Gate Fields

**`approval_prompt`** (optional)
- Type: `str | None`
- Default: `None`
- Description: Question to ask user (for `action="ask_user"`). Should clearly explain what operation requires approval and why.

**`approval_options`** (optional)
- Type: `list[str] | None`
- Default: `None` (defaults to `["Allow", "Deny"]`)
- Description: User choice options for approval. Can include `"Allow once"`, `"Allow always"`, `"Deny"` for flexible permission control.

**`approval_timeout`** (optional)
- Type: `float`
- Default: `300.0` (5 minutes)
- Description: Seconds to wait for user response. On timeout, `approval_default` action is taken.

**`approval_default`** (optional)
- Type: `Literal["allow", "deny"]`
- Default: `"deny"`
- Description: Default decision on timeout or error. `"deny"` (default) is safer for security-sensitive operations.

#### Output Control Fields

**`suppress_output`** (optional)
- Type: `bool`
- Default: `False`
- Description: Hide hook's stdout/stderr from user transcript. Use to prevent verbose processing output from cluttering UI.
- Security: Only suppresses hook's own output, not tool output

**`user_message`** (optional)
- Type: `str | None`
- Default: `None`
- Description: Message to display to user (separate from `context_injection`). Use for alerts, warnings, or status updates that user should see.

**`user_message_level`** (optional)
- Type: `Literal["info", "warning", "error"]`
- Default: `"info"`
- Description: Severity level for `user_message`. `"info"` for status updates, `"warning"` for non-critical issues, `"error"` for failures.

---

## Hook Registration

Register hooks to handle specific events.

### Function Signature

```python
from amplifier_core.hooks import HookRegistry

registry = HookRegistry()

unregister = registry.register(
    event: str,
    handler: Callable[[str, dict[str, Any]], Awaitable[HookResult]],
    priority: int = 0,
    name: str | None = None
)
```

### Parameters

**`event`** (required)
- Type: `str`
- Description: Event name to hook into (see [Events Reference](./HOOKS_EVENTS.md))
- Examples: `"tool:pre"`, `"tool:post"`, `"prompt:submit"`, `"execution:start"`

**`handler`** (required)
- Type: `Callable[[str, dict[str, Any]], Awaitable[HookResult]]`
- Description: Async function that handles the event
- Signature: `async def handler(event: str, data: dict[str, Any]) -> HookResult`

**`priority`** (optional)
- Type: `int`
- Default: `0`
- Description: Execution priority (lower number = earlier execution). Handlers execute sequentially by priority.

**`name`** (optional)
- Type: `str | None`
- Default: `None` (uses handler's `__name__`)
- Description: Handler name for debugging and logging

### Return Value

**`unregister`**
- Type: `Callable[[], None]`
- Description: Function to remove this handler from the registry
- Usage: `unregister()` to remove handler

### Example

```python
async def linter_hook(event: str, data: dict[str, Any]) -> HookResult:
    """Run linter after file writes and inject feedback."""
    if data.get("tool_name") not in ["Write", "Edit", "MultiEdit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]

    # Run linter
    result = subprocess.run(["ruff", "check", file_path], capture_output=True)

    if result.returncode != 0:
        # Inject linter errors to agent's context
        return HookResult(
            action="inject_context",
            context_injection=f"Linter found issues in {file_path}:\n{result.stderr.decode()}",
            user_message=f"Found linting issues in {file_path}",
            user_message_level="warning"
        )

    return HookResult(action="continue")

# Register hook
unregister = registry.register(
    event="tool:post",
    handler=linter_hook,
    priority=10,
    name="linter_feedback"
)
```

---

## Common Patterns

### Pattern 1: Context Injection (Automated Feedback)

Inject feedback to agent's context for immediate correction within same turn.

```python
async def validation_hook(event: str, data: dict) -> HookResult:
    """Validate output and inject feedback if issues found."""
    validation_errors = validate(data["tool_result"])

    if validation_errors:
        return HookResult(
            action="inject_context",
            context_injection=f"Validation errors:\n{format_errors(validation_errors)}",
            context_injection_role="system",  # Environmental feedback
            user_message="Validation found issues",
            user_message_level="warning",
            suppress_output=True  # Hide verbose validation output
        )

    return HookResult(action="continue")
```

**When to use**: Automated correction loops, quality checks, constraint enforcement

### Pattern 2: Approval Gates (Dynamic Permissions)

Request user approval for high-risk operations.

```python
async def production_protection_hook(event: str, data: dict) -> HookResult:
    """Require user approval for production file writes."""
    file_path = data["tool_input"]["file_path"]

    if "/production/" in file_path or file_path.endswith(".env"):
        return HookResult(
            action="ask_user",
            approval_prompt=f"Allow write to production file: {file_path}?",
            approval_options=["Allow once", "Allow always", "Deny"],
            approval_timeout=300.0,
            approval_default="deny",
            reason="Production file requires explicit user approval"
        )

    return HookResult(action="continue")
```

**When to use**: Production deployments, sensitive operations, cost controls

### Pattern 3: Output Control (Clean UX)

Control visibility for clean user experience.

```python
async def progress_hook(event: str, data: dict) -> HookResult:
    """Show clean progress message, hide verbose details."""
    files_processed = data.get("files_processed", 0)

    return HookResult(
        action="continue",
        user_message=f"Processed {files_processed} files",
        user_message_level="info",
        suppress_output=True  # Hide detailed processing logs
    )
```

**When to use**: Long-running operations, verbose processing, status updates

### Pattern 4: Combined Capabilities

Use multiple capabilities together.

```python
async def comprehensive_hook(event: str, data: dict) -> HookResult:
    """Validate, inject feedback, and show clean message."""
    issues = check_for_issues(data)

    if issues["critical"]:
        # Critical issues - inject context and show warning
        return HookResult(
            action="inject_context",
            context_injection=f"Critical issues:\n{format_issues(issues['critical'])}",
            user_message=f"Found {len(issues['critical'])} critical issues",
            user_message_level="error",
            suppress_output=True
        )
    elif issues["warnings"]:
        # Warnings only - show message but don't inject
        return HookResult(
            action="continue",
            user_message=f"Found {len(issues['warnings'])} warnings",
            user_message_level="warning",
            suppress_output=True
        )

    return HookResult(action="continue")
```

---

## Best Practices

### Security

1. **Validate inputs**: Never trust event data blindly
2. **Limit injection size**: Respect the configured `session.injection_size_limit` (default 10 KB, `None` for unlimited)
3. **Safe defaults**: Use `approval_default="deny"` for security-sensitive operations
4. **Audit trail**: All context injections are automatically logged with provenance
5. **Output scope**: Remember hooks can only suppress their own output, not tool output

### Performance

1. **Quick validation**: Keep pre-tool hooks fast to avoid blocking
2. **Async I/O**: Use `asyncio` for external calls (linters, APIs)
3. **Timeouts**: Set reasonable `approval_timeout` (default 5 min)
4. **Injection budget**: Consider token usage when injecting feedback - budget is configurable via `session.injection_budget_per_turn` (default: 10,000 tokens/turn, `None` for unlimited)

### User Experience

1. **Clear messages**: Make `approval_prompt` and `user_message` self-explanatory
2. **Appropriate levels**: Use `user_message_level` correctly (info/warning/error)
3. **Hide noise**: Use `suppress_output=True` for verbose processing
4. **Fast feedback**: Context injection enables immediate correction (no waiting for next turn)

### Code Quality

1. **Single responsibility**: Each hook should do one thing well
2. **Error handling**: Catch exceptions, return appropriate HookResult
3. **Testing**: Test hooks in isolation with mock event data
4. **Documentation**: Comment why you inject context vs show user message

---

## Error Handling

Hooks should handle errors gracefully and return appropriate `HookResult`.

```python
async def safe_hook(event: str, data: dict) -> HookResult:
    """Hook with proper error handling."""
    try:
        # Hook logic here
        result = do_something(data)

        if result.has_issues:
            return HookResult(
                action="inject_context",
                context_injection=f"Issues found: {result.issues}",
                user_message="Validation found issues"
            )

        return HookResult(action="continue")

    except Exception as e:
        # Log error, return safe result
        logger.error(f"Hook failed: {e}", exc_info=True)
        return HookResult(
            action="continue",  # Don't block on hook failure
            user_message=f"Hook error: {str(e)}",
            user_message_level="error"
        )
```

**Principle**: Hook failures should not crash the kernel or block operations unless explicitly intended (e.g., validation failure should return `action="deny"` on purpose).

---

## Testing

Test hooks in isolation with mock event data.

```python
import pytest
from amplifier_core.models import HookResult

@pytest.mark.asyncio
async def test_linter_hook():
    """Test linter hook injects context on errors."""
    # Arrange
    event = "tool:post"
    data = {
        "tool_name": "Write",
        "tool_input": {"file_path": "/tmp/test.py"},
        "tool_result": {"success": True}
    }

    # Act
    result = await linter_hook(event, data)

    # Assert
    if linter_found_errors:
        assert result.action == "inject_context"
        assert "Linter found issues" in result.context_injection
        assert result.user_message is not None
        assert result.user_message_level == "warning"
    else:
        assert result.action == "continue"
```

---

## See Also

- [Hooks Events Reference](./HOOKS_EVENTS.md) - Complete list of events and their data schemas
- [Hooks Guide](./HOOKS_GUIDE.md) - Tutorial introduction to hooks
- [Hook Patterns Guide](./HOOKS_PATTERNS.md) - Common patterns and examples
- [Hook Security](./HOOKS_SECURITY.md) - Security best practices
