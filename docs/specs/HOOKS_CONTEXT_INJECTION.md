# Context Injection Specification

**Version**: 1.0.0
**Status**: Implemented

---

## Purpose

Enable hooks to inject text directly into the agent's conversation context, creating automated feedback loops where hooks can guide agent behavior without requiring user intervention.

**Key use case**: Linter hook finds errors → injects to agent's context → agent sees and fixes immediately.

---

## Architecture

### Flow Diagram

```
Hook executes → Returns HookResult(action="inject_context")
    ↓
Session Coordinator receives result
    ↓
Validates injection (size limit, format)
    ↓
Adds to Context Manager as message with specified role
    ↓
Tags with provenance metadata (source hook, event, timestamp)
    ↓
Logs injection for audit trail
    ↓
Agent's next turn includes injected content in context
    ↓
Agent responds to feedback (e.g., fixes errors)
```

### Components

**1. Hook Handler** - Returns HookResult with injection
**2. Session Coordinator** - Routes injection to context
**3. Context Manager** - Stores injected message
**4. Audit Logger** - Records all injections
**5. Agent** - Sees injection in conversation context

---

## Injection Timing

**Decision**: Immediate injection (within same turn)

### Same-Turn Correction Loop

```
Turn N:
  1. User: "Update config.py"
  2. Agent: [uses Write tool]
  3. Write tool executes
  4. Post-tool hook runs
  5. Hook injects: "Linter found error: line 42 too long"
  6. Agent sees injection immediately
  7. Agent: "I see the linter found an issue, let me fix that"
  8. Agent: [uses Edit tool to fix line 42]
  9. All within single turn!
```

**Benefit**: Agent can fix issues without waiting for next user message. Enables autonomous correction.

**Alternative (rejected)**: Next-turn injection would require user to send another message before agent sees feedback.

---

## Injection Roles

Injected content appears as a conversation message with specified role.

### System Role (Default, Recommended)

```python
HookResult(
    action="inject_context",
    context_injection="Linter found issues...",
    context_injection_role="system"  # Environmental feedback
)
```

**Appears as**:
```
System: Linter found issues in config.py:
  Line 42: E501 line too long
  Line 58: F401 unused import
```

**Use cases**:
- Environmental feedback (linter, type checker, tests)
- System state updates (file changes, resource usage)
- Validation results (constraint violations)

### User Role

```python
HookResult(
    action="inject_context",
    context_injection="Please ensure all files pass linting",
    context_injection_role="user"  # Simulated user input
)
```

**Appears as**:
```
User: Please ensure all files pass linting
```

**Use cases**:
- Simulating user preferences
- Policy reminders ("always use type hints")
- Constraint restating ("maximum file size: 1000 lines")

**Caution**: Can be confusing if agent thinks user actually said this.

### Assistant Role

```python
HookResult(
    action="inject_context",
    context_injection="I should check for security issues next",
    context_injection_role="assistant"  # Agent self-talk
)
```

**Appears as**:
```
Assistant: I should check for security issues next
```

**Use cases**:
- Agent self-reminders
- Prompting specific behaviors
- Guided thinking

**Caution**: Highly unusual pattern, use sparingly.

---

## Security Model

### Size Limits

**Maximum injection size**: 10KB per injection

**Enforcement**:
```python
MAX_INJECTION_SIZE = 10 * 1024  # 10KB

if len(context_injection) > MAX_INJECTION_SIZE:
    logger.error("Hook injection too large", size=len(context_injection))
    raise ValueError(f"Context injection exceeds max size: {len(context_injection)} bytes")
```

**Rationale**: Prevents hooks from flooding context with excessive content, consuming tokens, or overwhelming the agent.

### Audit Trail

Every injection is logged with provenance metadata.

**Log entry**:
```json
{
  "event": "hook:context_injection",
  "session_id": "abc-123",
  "hook_name": "linter_feedback",
  "hook_event": "tool:post",
  "injection_size": 245,
  "injection_role": "system",
  "timestamp": "2025-11-07T12:34:56Z"
}
```

**Purpose**: Security audit, debugging, usage analysis

### Provenance Tagging

Injected messages tagged with metadata:

```python
{
    "role": "system",
    "content": "Linter found issues...",
    "metadata": {
        "source": "hook",
        "hook_name": "linter_feedback",
        "event": "tool:post",
        "timestamp": "2025-11-07T12:34:56Z"
    }
}
```

**Benefits**:
- Traceability (which hook injected what)
- Debugging (inspect injection sources)
- Filtering (exclude hook messages if needed)

---

## Implementation

### In Session Coordinator

```python
async def handle_hook_result(self, event: str, result: HookResult, hook_name: str):
    """Process hook result including context injection."""

    # Handle context injection
    if result.action == "inject_context" and result.context_injection:
        # Validate size
        if len(result.context_injection) > MAX_INJECTION_SIZE:
            logger.error("Hook injection too large",
                hook=hook_name, size=len(result.context_injection))
            raise ValueError("Context injection exceeds maximum size")

        # Add to context with provenance
        self.context.add_message(
            role=result.context_injection_role,
            content=result.context_injection,
            metadata={
                "source": "hook",
                "hook_name": hook_name,
                "event": event,
                "timestamp": datetime.now().isoformat()
            }
        )

        # Audit log
        logger.info("hook_context_injection",
            hook=hook_name,
            event=event,
            size=len(result.context_injection),
            role=result.context_injection_role)

    # Handle user message (separate from context injection)
    if result.user_message:
        self.display.show_message(
            message=result.user_message,
            level=result.user_message_level,
            source=f"hook:{hook_name}"
        )
```

---

## Token Budget Management

### Injection Budget

To prevent context overflow from excessive injections, track token usage:

```python
INJECTION_BUDGET_PER_TURN = 1000  # tokens

class SessionCoordinator:
    def __init__(self):
        self.current_turn_injections = 0

    async def handle_injection(self, content: str):
        # Estimate tokens (rough: 1 token ≈ 4 chars)
        estimated_tokens = len(content) // 4

        if self.current_turn_injections + estimated_tokens > INJECTION_BUDGET_PER_TURN:
            logger.warning("Hook injection budget exceeded this turn",
                current=self.current_turn_injections,
                attempted=estimated_tokens,
                budget=INJECTION_BUDGET_PER_TURN)
            # Continue but log warning

        self.current_turn_injections += estimated_tokens

    def reset_turn(self):
        """Called at turn boundary."""
        self.current_turn_injections = 0
```

### Multiple Injections

When multiple hooks inject in same turn:

```python
# Hook 1 injects 200 tokens
# Hook 2 injects 300 tokens
# Hook 3 injects 150 tokens

# Batched into single system message:
"""
System: Hook feedback:

From linter_feedback:
Linter found issues in config.py:
  Line 42: E501 line too long

From type_checker:
Type errors in models.py:
  Line 15: Missing return type annotation

From test_runner:
Tests failed: 2 failures, 0 errors
"""
```

**Benefit**: Single consolidated message vs message spam.

---

## Error Handling

### Injection Failures

```python
try:
    self.context.add_message(
        role=result.context_injection_role,
        content=result.context_injection,
        metadata={...}
    )
except Exception as e:
    # Log error, don't crash
    logger.error("Failed to inject context", hook=hook_name, error=str(e))

    # Show user message if provided
    if result.user_message:
        self.display.show_message(
            message=f"{result.user_message} (injection failed)",
            level="error",
            source=f"hook:{hook_name}"
        )

    # Continue execution (non-interference principle)
```

**Principle**: Hook failures don't crash kernel. Log error and continue.

---

## Use Cases

### Use Case 1: Linter Feedback Loop

**Scenario**: Agent writes Python code, linter finds issues, agent fixes them.

**Flow**:
1. Agent uses Write tool to create `config.py`
2. Post-tool hook runs ruff linter
3. Linter finds 3 errors
4. Hook injects: "Linter errors in config.py: Line 42 too long, Line 58 unused import, Line 73 indentation"
5. Agent sees feedback immediately (same turn)
6. Agent uses Edit tool to fix all 3 errors
7. Done!

**Hook implementation**: See `examples/hooks/linter_feedback.py`

### Use Case 2: Type Checker Integration

**Scenario**: Agent modifies Python code, type checker validates, agent corrects type errors.

**Flow**:
1. Agent uses Edit tool on `models.py`
2. Post-tool hook runs pyright
3. Type checker finds missing annotations
4. Hook injects: "Type errors: Line 15 missing return type, Line 32 implicit Any"
5. Agent adds missing type annotations
6. Done!

### Use Case 3: Test Runner Integration

**Scenario**: Agent changes code, tests run, agent fixes failures.

**Flow**:
1. Agent uses Edit tool on `utils.py`
2. Post-tool hook runs pytest
3. 2 tests fail
4. Hook injects: "Test failures: test_process_data failed (AssertionError: Expected 5, got 3), test_validate failed (ValueError: Invalid input)"
5. Agent analyzes failures, fixes the bugs
6. Done!

### Use Case 4: Git Status Injection

**Scenario**: On session start, inject current git status so agent is aware of uncommitted changes.

**Flow**:
1. Session starts
2. Hook runs `git status --short`
3. Finds uncommitted changes
4. Hook injects: "Git status: 3 modified files (config.py, models.py, tests.py), 1 untracked file (temp.log)"
5. Agent is aware of repo state
6. User asks about changes, agent references exact files

**Hook implementation**: See `examples/hooks/git_status_injector.py`

**Note**: This replaces the previous "context manager hack" where git status was manually loaded.

---

## Performance Considerations

### Injection Overhead

**Cost per injection**:
- Size validation: ~1ms
- Context manager add: ~5ms
- Metadata tagging: ~1ms
- Audit logging: ~2ms
- **Total**: ~10ms per injection

**Negligible impact** for typical usage (1-5 injections per turn).

### Token Usage

**10KB injection** ≈ 2,500 tokens (rough estimate: 4 chars/token)

**Budget**: 1,000 tokens per turn recommended
- Allows 4-5 typical injections
- Prevents context overflow
- Warns if exceeded

---

## Security Checklist

When implementing hooks with context injection:

- [ ] Validate injection size (< 10KB)
- [ ] Sanitize injected content (no command injection via content)
- [ ] Use appropriate role (system for most cases)
- [ ] Log injection for audit
- [ ] Handle injection failures gracefully
- [ ] Consider token budget impact
- [ ] Document why injection is needed
- [ ] Test with malicious inputs

---

## See Also

- [Hooks API Reference](../../amplifier-core/docs/HOOKS_API.md) - Complete HookResult API
- [Hook Patterns Guide](../guides/HOOK_PATTERNS.md) - Common patterns
- [Hook Security Guide](../guides/HOOK_SECURITY.md) - Security best practices
- [Example: Linter Feedback](../../examples/hooks/linter_feedback.py) - Working implementation
