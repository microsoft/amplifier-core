# Amplifier Hooks User Guide

Learn how to create powerful hooks that observe, validate, and guide your Amplifier sessions.

---

## What Are Hooks?

Hooks are functions that run at specific points in Amplifier's lifecycle. They enable you to:

- **Observe** what's happening (logging, metrics)
- **Validate** operations before they happen (security, constraints)
- **Guide** the agent with automated feedback (linter errors → agent fixes)
- **Control** what requires approval (production files, expensive operations)
- **Customize** output visibility (hide noise, show important messages)

**Key insight**: Hooks don't just observe - they actively participate in the agent's thinking process.

---

## Quick Start

### Your First Hook: Date/Time Injector

Let's create a simple hook that injects the current date and time into the agent's context at session start.

```python
from amplifier_core.models import HookResult
from datetime import datetime

async def datetime_injector(event: str, data: dict) -> HookResult:
    """Inject current date/time into agent's context on session start."""

    # Generate current date/time
    now = datetime.now()
    context = f"Current date/time: {now.strftime('%Y-%m-%d %H:%M:%S %Z')}"

    # Inject to agent's context
    return HookResult(
        action="inject_context",
        context_injection=context,
        suppress_output=True  # Don't show to user
    )
```

**Register it**:
```python
from amplifier_core.hooks import HookRegistry

registry = HookRegistry()
registry.register("session:start", datetime_injector)
```

**What happens**:
1. Session starts
2. Hook injects current date/time to agent's context
3. Agent is always aware of current date/time
4. User asks "what's today's date?" → Agent knows!

---

## Hook Capabilities

### 1. Observe (Logging, Metrics)

Simple observation - log what happens:

```python
async def tool_logger(event: str, data: dict) -> HookResult:
    """Log all tool executions."""
    tool_name = data.get("tool_name")
    logger.info(f"Tool executed: {tool_name}")

    return HookResult(action="continue")  # Don't interfere
```

### 2. Validate (Block Bad Operations)

Prevent operations from proceeding:

```python
async def bash_validator(event: str, data: dict) -> HookResult:
    """Block dangerous bash commands."""
    if data.get("tool_name") != "Bash":
        return HookResult(action="continue")

    command = data["tool_input"]["command"]

    if "rm -rf /" in command:
        return HookResult(
            action="deny",
            reason="Dangerous command blocked: rm -rf /"
        )

    return HookResult(action="continue")
```

**Agent sees**: "Operation denied: Dangerous command blocked: rm -rf /"

### 3. Inject Context (Guide the Agent)

**This is the most powerful capability** - inject feedback directly to the agent's context:

```python
async def linter_feedback(event: str, data: dict) -> HookResult:
    """Run linter and inject errors to agent."""
    import subprocess

    if data.get("tool_name") not in ["Write", "Edit", "MultiEdit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]

    # Run linter
    result = subprocess.run(
        ["ruff", "check", file_path],
        capture_output=True
    )

    if result.returncode != 0:
        # Inject linter errors to agent's context
        return HookResult(
            action="inject_context",
            context_injection=f"Linter found issues in {file_path}:\n{result.stderr.decode()}",
            user_message=f"Found linting issues in {file_path}",
            user_message_level="warning",
            suppress_output=True
        )

    return HookResult(action="continue")
```

**What happens**:
1. Agent writes Python file
2. Hook runs ruff linter
3. Linter finds 3 errors
4. Hook injects: "Linter found issues: Line 42 too long, Line 58 unused import..."
5. **Agent sees the errors immediately** (same turn!)
6. Agent fixes all 3 errors automatically
7. Done!

**This is autonomous correction** - agent self-corrects without waiting for user.

### 4. Request Approval (Dynamic Permissions)

Ask user for approval on high-risk operations:

```python
async def production_protection(event: str, data: dict) -> HookResult:
    """Require approval for production file writes."""
    file_path = data["tool_input"]["file_path"]

    if "/production/" in file_path or file_path.endswith(".env"):
        return HookResult(
            action="ask_user",
            approval_prompt=f"Allow write to: {file_path}?",
            approval_options=["Allow once", "Allow always", "Deny"],
            approval_timeout=300.0,  # 5 minutes
            approval_default="deny",  # Safe default
            reason="Production file requires user approval"
        )

    return HookResult(action="continue")
```

**What happens**:
1. Agent tries to write to `production/config.py`
2. Hook asks you: "Allow write to production/config.py?"
3. You see options: "Allow once", "Allow always", "Deny"
4. You select "Allow once"
5. Write proceeds this time
6. Next production write → asks again (unless you chose "Allow always")

### 5. Control Output (Clean UX)

Hide verbose processing, show targeted messages:

```python
async def progress_reporter(event: str, data: dict) -> HookResult:
    """Show clean progress, hide verbose logs."""
    files_processed = count_processed_files()

    return HookResult(
        action="continue",
        user_message=f"Processed {files_processed}/100 files",
        user_message_level="info",
        suppress_output=True  # Hide detailed processing logs
    )
```

**User sees**: "Processed 42/100 files" (clean!)
**User doesn't see**: Verbose hook processing logs (suppressed)

---

## Common Patterns

### Pattern 1: Linter/Formatter Feedback Loop

**Problem**: Agent writes code with style issues

**Solution**: Hook runs linter, injects errors, agent fixes automatically

```python
async def auto_format_feedback(event: str, data: dict) -> HookResult:
    """Run formatter and inject issues."""
    if data.get("tool_name") not in ["Write", "Edit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]

    # Run formatter
    result = subprocess.run(["black", "--check", file_path], capture_output=True)

    if result.returncode != 0:
        return HookResult(
            action="inject_context",
            context_injection=f"Formatting issues:\n{result.stderr.decode()}",
            user_message="Auto-formatting needed",
            user_message_level="info"
        )

    return HookResult(action="continue")
```

**Register**:
```python
registry.register("tool:post", auto_format_feedback, priority=10)
```

### Pattern 2: Test Runner Integration

**Problem**: Agent changes code, breaks tests

**Solution**: Hook runs tests, injects failures, agent fixes

```python
async def test_runner(event: str, data: dict) -> HookResult:
    """Run tests after code changes."""
    if data.get("tool_name") not in ["Write", "Edit"]:
        return HookResult(action="continue")

    # Run tests
    result = subprocess.run(["pytest", "-x"], capture_output=True)

    if result.returncode != 0:
        return HookResult(
            action="inject_context",
            context_injection=f"Tests failed:\n{result.stdout.decode()}",
            user_message="Test failures detected",
            user_message_level="error"
        )

    return HookResult(
        action="continue",
        user_message="All tests passing",
        user_message_level="info"
    )
```

### Pattern 3: Git Status on Session Start

**Problem**: Agent doesn't know about uncommitted changes

**Solution**: Hook injects git status on session start

```python
async def git_status_injector(event: str, data: dict) -> HookResult:
    """Inject git status into agent's context."""
    import subprocess

    # Get git status
    result = subprocess.run(
        ["git", "status", "--short"],
        capture_output=True,
        cwd=data.get("cwd", ".")
    )

    if result.stdout:
        status = result.stdout.decode().strip()
        return HookResult(
            action="inject_context",
            context_injection=f"Git repository status:\n{status}",
            suppress_output=True
        )

    return HookResult(action="continue")
```

**Register**:
```python
registry.register("session:start", git_status_injector)
```

**Benefit**: Agent always aware of repo state, can reference uncommitted files.

### Pattern 4: Cost Control with Approval

**Problem**: Don't want agent making expensive API calls without approval

**Solution**: Hook calculates cost, asks approval if over threshold

```python
async def cost_control(event: str, data: dict) -> HookResult:
    """Ask approval for expensive LLM calls."""
    # Calculate estimated cost
    tokens = data.get("estimated_tokens", 0)
    cost = tokens * 0.00001  # Example rate

    if cost > 1.0:  # Over $1
        return HookResult(
            action="ask_user",
            approval_prompt=f"This call will cost ~${cost:.2f}. Continue?",
            approval_options=["Allow", "Deny"],
            approval_timeout=60.0,
            approval_default="deny"
        )

    return HookResult(action="continue")
```

---

## Hook Registration

### Basic Registration

```python
from amplifier_core.hooks import HookRegistry

registry = HookRegistry()

# Register hook for specific event
registry.register(
    event="tool:post",
    handler=my_hook_function,
    priority=0,   # Optional (default: 0)
    name="my_hook"  # Optional (uses function name if not provided)
)
```

### Priority Order

Lower priority number = runs earlier:

```python
registry.register("tool:post", hook_a, priority=0)   # Runs first
registry.register("tool:post", hook_b, priority=10)  # Runs second
registry.register("tool:post", hook_c, priority=20)  # Runs third
```

**Use case**: Validation hooks (priority=0) run before logging hooks (priority=10).

### Unregister

Registration returns an unregister function:

```python
unregister = registry.register("tool:post", my_hook)

# Later, to remove hook:
unregister()
```

---

## Events Reference

### Session Lifecycle

- `session:start` - Session begins (inject initial context)
- `session:end` - Session ends (log stats, cleanup)

### Prompt Lifecycle

- `prompt:submit` - User submits prompt (inject dynamic context)

### Tool Lifecycle

- `tool:pre` - Before tool execution (validate, block, approve)
- `tool:post` - After tool execution (validate output, inject feedback)

### Orchestrator Lifecycle

- `orchestrator:complete` - Orchestrator finishes (log stats, notify)

### Agent Delegation

- `agent:spawn` - Sub-agent spawned
- `agent:complete` - Sub-agent finished

### Context Management

- `context:pre_compact` - Before context compaction

### Notifications

- `user:notification` - User needs attention (desktop alerts)

**Complete list**: See [Events Reference](./HOOKS_EVENTS.md)

---

## Best Practices

### Security

1. **Validate inputs**: Don't trust event data blindly
2. **Safe defaults**: Use `approval_default="deny"` for sensitive operations
3. **Size limits**: Keep injections under 10KB
4. **Audit aware**: Remember all actions are logged

### Performance

1. **Keep hooks fast**: Pre-tool hooks should be quick (<100ms)
2. **Async I/O**: Use `asyncio` for external calls
3. **Reasonable timeouts**: Don't block forever (default 5min)
4. **Budget awareness**: Large injections use tokens

### User Experience

1. **Clear messages**: Make prompts and messages self-explanatory
2. **Appropriate severity**: Use correct `user_message_level` (info/warning/error)
3. **Hide noise**: Use `suppress_output=True` for verbose processing
4. **Fast feedback**: Context injection enables immediate correction

### Code Quality

1. **Single responsibility**: Each hook does one thing well
2. **Error handling**: Catch exceptions, return safe defaults
3. **Testing**: Test hooks in isolation
4. **Documentation**: Comment why you use each capability

---

## Debugging Hooks

### Enable Hook Logging

```python
import logging

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger("amplifier_core.hooks")
```

**You'll see**:
```
DEBUG:amplifier_core.hooks:Emitting event 'tool:post' to 3 handlers
DEBUG:amplifier_core.hooks:Handler 'linter_feedback' modified event data
INFO:amplifier_core.hooks:hook_context_injection hook=linter_feedback size=245
```

### Test Hook in Isolation

```python
import asyncio

async def test_my_hook():
    """Test hook with mock data."""
    event = "tool:post"
    data = {
        "tool_name": "Write",
        "tool_input": {"file_path": "/tmp/test.py"}
    }

    result = await my_hook(event, data)
    print(f"Action: {result.action}")
    print(f"Context injection: {result.context_injection}")

# Run test
asyncio.run(test_my_hook())
```

### View Audit Logs

All hook actions are logged:

```bash
# View hook-related logs
grep "hook" ~/.amplifier/logs/session.jsonl | jq .

# View context injections
grep "hook_context_injection" ~/.amplifier/logs/session.jsonl | jq .

# View approval requests
grep "approval_requested" ~/.amplifier/logs/session.jsonl | jq .
```

---

## Example Hooks

### Example 1: Git Status Injector

**File**: `amplifier-module-hooks-status-context (working implementation)`

Injects git repository status on session start so agent is always aware of uncommitted changes.

**Use case**: "What files have changed?" → Agent knows exactly which files.

### Example 2: Linter Feedback

**File**: `amplifier-module-hooks-status-context (working implementation)`

Runs ruff linter after file writes, injects errors to agent for immediate correction.

**Use case**: Agent writes code → Linter finds issues → Agent fixes automatically.

### Example 3: Production Protection

**File**: `amplifier-module-hooks-status-context (working implementation)`

Requires user approval before writing to production files.

**Use case**: Agent tries production write → You approve/deny → Safe operation.

### Example 4: Session Completion Notifier

**File**: `amplifier-module-hooks-status-context (working implementation)`

Sends desktop notification when session completes.

**Use case**: Long session → You're notified when done.

### Example 5: Type Checker

**File**: `amplifier-module-hooks-status-context (working implementation)`

Runs pyright after Python file edits, injects type errors for correction.

**Use case**: Agent modifies code → Type errors found → Agent adds annotations.

**Complete examples**: See [amplifier-module-hooks-status-context](https://github.com/microsoft/amplifier-module-hooks-status-context)

---

## Advanced Topics

### Combining Capabilities

Use multiple capabilities together:

```python
async def comprehensive_validator(event: str, data: dict) -> HookResult:
    """Validate, inject feedback, and show clean message."""

    issues = validate(data)

    if issues["critical"]:
        # Critical - inject context and show error
        return HookResult(
            action="inject_context",
            context_injection=f"Critical issues:\n{format_issues(issues['critical'])}",
            user_message=f"Found {len(issues['critical'])} critical issues",
            user_message_level="error",
            suppress_output=True
        )
    elif issues["warnings"]:
        # Warnings - just show message
        return HookResult(
            action="continue",
            user_message=f"Found {len(issues['warnings'])} warnings",
            user_message_level="warning"
        )

    return HookResult(action="continue")
```

### Conditional Context Injection

Only inject when relevant:

```python
async def smart_injector(event: str, data: dict) -> HookResult:
    """Inject context only when needed."""

    prompt = data.get("prompt", "")

    # Only inject git status if user asks about changes
    if "change" in prompt.lower() or "modified" in prompt.lower():
        status = get_git_status()
        return HookResult(
            action="inject_context",
            context_injection=f"Git status:\n{status}",
            suppress_output=True
        )

    return HookResult(action="continue")
```

### Progressive Approval

Cache low-risk approvals, always ask for high-risk:

```python
async def tiered_protection(event: str, data: dict) -> HookResult:
    """Different approval policies for different paths."""

    file_path = data["tool_input"]["file_path"]

    if "/production/" in file_path:
        # High-risk: Always ask, never cache
        return HookResult(
            action="ask_user",
            approval_prompt=f"PRODUCTION WRITE: {file_path}?",
            approval_options=["Allow once", "Deny"],  # No "Allow always"
            approval_timeout=600.0,  # 10 minutes to review
            approval_default="deny"
        )
    elif "/staging/" in file_path:
        # Medium-risk: Ask with caching
        return HookResult(
            action="ask_user",
            approval_prompt=f"Allow staging write: {file_path}?",
            approval_options=["Allow once", "Allow always", "Deny"],
            approval_timeout=120.0,
            approval_default="deny"
        )

    # Low-risk: No approval needed
    return HookResult(action="continue")
```

---

## Troubleshooting

### Hook Not Executing

**Check registration**:
```python
# List all registered hooks
handlers = registry.list_handlers()
print(handlers)

# Should see your hook name
```

**Check event name**:
```python
# Use exact event constants
from amplifier_core.hooks import HookRegistry

registry.register(HookRegistry.TOOL_POST, my_hook)  # ✅ Correct
registry.register("tool:post", my_hook)  # ✅ Also works
registry.register("tool_post", my_hook)   # ❌ Wrong (underscore)
```

### Context Injection Not Showing

**Check action**:
```python
return HookResult(
    action="inject_context",  # Must be this action!
    context_injection="your content"
)
```

**Check size**:
```python
# Max 10KB
if len(content) > 10240:
    logger.warning("Injection too large!")
```

### Approval Not Working

**Check return value**:
```python
# Hook must return ask_user action
return HookResult(
    action="ask_user",  # Not "continue"!
    approval_prompt="Your question?"
)
```

**Check timeout**:
```python
# Default is 5 minutes - may be too long
approval_timeout=60.0  # Reduce to 1 minute for testing
```

---

## FAQ

**Q: Can hooks modify the agent's response?**

A: No. Hooks observe and validate operations, inject context for agent to consider, but don't directly modify what the agent says. They influence via context injection.

**Q: Can multiple hooks inject context in same turn?**

A: Yes! All injections are batched into a single system message.

**Q: What happens if user doesn't respond to approval?**

A: After timeout (default 5min), the `approval_default` action is taken (`"deny"` by default).

**Q: Can hooks see each other's injections?**

A: No. Hooks execute sequentially but don't see each other's context injections. Only the agent sees all injections.

**Q: Do hooks slow down the agent?**

A: Minimally. Most hooks add <10ms. Approval requests are blocking (user-dependent).

**Q: Can I disable hooks temporarily?**

A: Yes, unregister them:
```python
unregister = registry.register(...)
# Later:
unregister()
```

---

## Next Steps

1. **Read the examples**: See [amplifier-module-hooks-status-context](https://github.com/microsoft/amplifier-module-hooks-status-context) for complete implementations
2. **Try simple hooks**: Start with datetime_injector or tool_logger
3. **Build feedback loops**: Try linter_feedback for autonomous correction
4. **Add safety**: Create approval gates for your critical operations
5. **Review patterns**: See [Hook Patterns Guide](./HOOK_PATTERNS.md) for more ideas

---

## See Also

- [Hooks API Reference](./HOOKS_API.md) - Complete API documentation
- [Hook Patterns](./HOOK_PATTERNS.md) - Common patterns and anti-patterns
- [Hook Security](./HOOK_SECURITY.md) - Security best practices
- [Example Hook Module: hooks-status-context](https://github.com/microsoft/amplifier-module-hooks-status-context) - Working implementation
