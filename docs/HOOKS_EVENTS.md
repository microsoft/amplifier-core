# Hooks Events Reference

Complete reference for all hook events in Amplifier.

---

## Event Lifecycle

```
SESSION_START
    ↓
PROMPT_SUBMIT
    ↓
TOOL_PRE → [tool execution] → TOOL_POST
    ↓
ORCHESTRATOR_COMPLETE
    ↓
USER_NOTIFICATION (as needed)
    ↓
CONTEXT_PRE_COMPACT (when needed)
    ↓
AGENT_SPAWN → [agent execution] → AGENT_COMPLETE
    ↓
SESSION_END
```

---

## Session Lifecycle Events

### SESSION_START

**Event name**: `session:start`

**When**: Session begins or resumes

**Data schema**:
```python
{
    "session_id": str,         # Unique session identifier
    "mount_plan": dict,        # Complete mount plan configuration
    "source": str,             # "startup" | "resume" | "compact"
    "timestamp": str           # ISO8601 timestamp
}
```

**Use cases**:
- Load session context (recent issues, tasks)
- Initialize session-specific resources
- Log session start for audit trails

**Example**:
```python
async def session_context_loader(event: str, data: dict) -> HookResult:
    """Load recent issues into agent's context on session start."""
    issues = load_recent_issues(limit=5)

    return HookResult(
        action="inject_context",
        context_injection=f"Recent open issues:\n{format_issues(issues)}",
        suppress_output=True
    )
```

---

### SESSION_END

**Event name**: `session:end`

**When**: Session ends (normal completion, user exit, error)

**Data schema**:
```python
{
    "session_id": str,
    "reason": str,             # "complete" | "user_exit" | "error" | "timeout"
    "duration_ms": int,        # Total session duration
    "stats": {                 # Session statistics
        "total_messages": int,
        "tool_invocations": int,
        "total_tokens": int
    },
    "timestamp": str
}
```

**Use cases**:
- Log session statistics
- Clean up session resources
- Send completion notifications
- Save session summary

**Example**:
```python
async def session_logger(event: str, data: dict) -> HookResult:
    """Log session stats on completion."""
    stats = data["stats"]
    logger.info(
        "session_complete",
        session_id=data["session_id"],
        duration_ms=data["duration_ms"],
        messages=stats["total_messages"],
        tools=stats["tool_invocations"]
    )

    return HookResult(action="continue")
```

---

## Prompt Lifecycle Events

### PROMPT_SUBMIT

**Event name**: `prompt:submit`

**When**: User submits a prompt, before agent processes it

**Data schema**:
```python
{
    "session_id": str,
    "prompt": str,             # User's input text
    "metadata": dict,          # Optional prompt metadata
    "timestamp": str
}
```

**Use cases**:
- Inject additional context based on prompt
- Validate prompt content
- Log user inputs
- Add dynamic context (current time, environment state)

**Example**:
```python
async def datetime_injector(event: str, data: dict) -> HookResult:
    """Inject current date/time into agent's context."""
    from datetime import datetime

    now = datetime.now()
    context = f"Current date/time: {now.isoformat()}"

    return HookResult(
        action="inject_context",
        context_injection=context,
        suppress_output=True
    )
```

---

## Tool Lifecycle Events

### TOOL_PRE

**Event name**: `tool:pre`

**When**: Before tool execution (can block tool call)

**Data schema**:
```python
{
    "session_id": str,
    "tool_name": str,          # Name of tool being called
    "tool_input": dict,        # Tool parameters
    "timestamp": str
}
```

**Use cases**:
- Validate tool parameters
- Block dangerous operations
- Request approval for high-risk tools
- Log tool invocations

**Example**:
```python
async def bash_validator(event: str, data: dict) -> HookResult:
    """Validate bash commands before execution."""
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

---

### TOOL_POST

**Event name**: `tool:post`

**When**: After tool execution completes

**Data schema**:
```python
{
    "session_id": str,
    "tool_name": str,
    "tool_input": dict,        # Tool parameters used
    "tool_result": dict,       # Tool execution result
    "success": bool,           # Whether tool succeeded
    "duration_ms": int,        # Tool execution time
    "timestamp": str
}
```

**Use cases**:
- Validate tool output
- Inject feedback to agent (linter errors, test failures)
- Log tool results
- Trigger follow-up actions

**Example**:
```python
async def linter_feedback(event: str, data: dict) -> HookResult:
    """Run linter after file writes, inject errors to agent."""
    if data.get("tool_name") not in ["Write", "Edit", "MultiEdit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]
    result = subprocess.run(["ruff", "check", file_path], capture_output=True)

    if result.returncode != 0:
        return HookResult(
            action="inject_context",
            context_injection=f"Linter issues in {file_path}:\n{result.stderr.decode()}",
            user_message=f"Found {len(result.stderr.splitlines())} linting issues",
            user_message_level="warning",
            suppress_output=True
        )

    return HookResult(action="continue")
```

---

## Orchestrator Events

### ORCHESTRATOR_COMPLETE

**Event name**: `orchestrator:complete`

**When**: Main orchestrator finishes processing (after agent's final response)

**Data schema**:
```python
{
    "session_id": str,
    "orchestrator": str,       # Orchestrator type (loop-basic, loop-streaming, etc.)
    "final_message": dict,     # Last assistant message
    "turn_count": int,         # Number of conversation turns
    "duration_ms": int,        # Total orchestration time
    "status": str,             # "success" | "error" | "interrupted"
    "timestamp": str
}
```

**Use cases**:
- Log session statistics
- Send completion notifications
- Trigger cleanup tasks
- Calculate costs/usage

**Example**:
```python
async def session_completion(event: str, data: dict) -> HookResult:
    """Send desktop notification on session completion."""
    turns = data["turn_count"]
    duration_sec = data["duration_ms"] / 1000

    # Send notification (using system notify-send)
    subprocess.run([
        "notify-send",
        "Amplifier Session Complete",
        f"Completed {turns} turns in {duration_sec:.1f}s"
    ])

    return HookResult(
        action="continue",
        user_message=f"Session complete: {turns} turns in {duration_sec:.1f}s",
        suppress_output=True
    )
```

---

## Agent Delegation Events

### AGENT_SPAWN

**Event name**: `agent:spawn`

**When**: Sub-agent is spawned (before execution)

**Data schema**:
```python
{
    "session_id": str,
    "parent_session_id": str,  # Parent session ID
    "agent_name": str,         # Agent identifier
    "task": str,               # Task description
    "config_override": dict,   # Agent-specific config
    "timestamp": str
}
```

**Use cases**:
- Log agent spawns
- Track agent usage
- Inject context to sub-agents

---

### AGENT_COMPLETE

**Event name**: `agent:complete`

**When**: Sub-agent finishes execution

**Data schema**:
```python
{
    "session_id": str,
    "parent_session_id": str,
    "agent_name": str,
    "task": str,
    "result": dict,            # Agent's result
    "duration_ms": int,
    "status": str,             # "success" | "error" | "interrupted"
    "timestamp": str
}
```

**Use cases**:
- Log agent completions
- Track agent performance
- Inject agent results to parent context

---

## Context Management Events

### CONTEXT_PRE_COMPACT

**Event name**: `context:pre_compact`

**When**: Before context compaction occurs

**Data schema**:
```python
{
    "session_id": str,
    "trigger": str,            # "auto" | "manual"
    "current_tokens": int,     # Token count before compaction
    "target_tokens": int,      # Target token count after compaction
    "messages_count": int,     # Number of messages in context
    "timestamp": str
}
```

**Use cases**:
- Log compaction events
- Export context before compaction
- Inject summary before compaction

---

## Notification Events

### USER_NOTIFICATION

**Event name**: `user:notification`

**When**: User needs attention (awaiting input, approval required, errors)

**Data schema**:
```python
{
    "session_id": str,
    "notification_type": str,  # "awaiting_input" | "approval_required" | "error" | "alert"
    "message": str,            # Notification message
    "metadata": dict,          # Additional context
    "timestamp": str
}
```

**Use cases**:
- Desktop notifications
- Sound alerts
- External system alerts (Slack, Discord)
- Visual indicators (system tray)

**Example**:
```python
async def desktop_notifier(event: str, data: dict) -> HookResult:
    """Send desktop notification when user needs attention."""
    notification_type = data["notification_type"]
    message = data["message"]

    subprocess.run([
        "notify-send",
        f"Amplifier: {notification_type}",
        message
    ])

    return HookResult(action="continue", suppress_output=True)
```

---

## Decision Events

### DECISION_TOOL_RESOLUTION

**Event name**: `decision:tool_resolution`

**When**: Orchestrator selects which tool to use

**Data schema**:
```python
{
    "session_id": str,
    "available_tools": list[str],  # Tools available
    "selected_tool": str,          # Tool chosen
    "reason": str,                 # Why this tool
    "timestamp": str
}
```

**Use cases**:
- Log tool selection decisions
- Validate tool choices
- Track tool usage patterns

---

### DECISION_AGENT_RESOLUTION

**Event name**: `decision:agent_resolution`

**When**: Orchestrator decides to delegate to agent

**Data schema**:
```python
{
    "session_id": str,
    "agent_name": str,         # Agent selected
    "reason": str,             # Why delegation occurred
    "timestamp": str
}
```

---

### DECISION_CONTEXT_RESOLUTION

**Event name**: `decision:context_resolution`

**When**: Orchestrator selects context management strategy

**Data schema**:
```python
{
    "session_id": str,
    "strategy": str,           # Context strategy selected
    "reason": str,
    "timestamp": str
}
```

---

## Error Events

### ERROR_TOOL

**Event name**: `error:tool`

**When**: Tool execution fails

**Data schema**:
```python
{
    "session_id": str,
    "tool_name": str,
    "error": dict,             # Error details
    "timestamp": str
}
```

---

### ERROR_PROVIDER

**Event name**: `error:provider`

**When**: Provider (LLM) call fails

**Data schema**:
```python
{
    "session_id": str,
    "provider": str,
    "error": dict,
    "timestamp": str
}
```

---

### ERROR_ORCHESTRATION

**Event name**: `error:orchestration`

**When**: Orchestrator encounters error

**Data schema**:
```python
{
    "session_id": str,
    "error": dict,
    "timestamp": str
}
```

---

## Event Constants

All event names are defined in `amplifier_core.hooks.HookRegistry`:

```python
from amplifier_core.hooks import HookRegistry

# Session lifecycle
HookRegistry.SESSION_START = "session:start"
HookRegistry.SESSION_END = "session:end"

# Prompt lifecycle
HookRegistry.PROMPT_SUBMIT = "prompt:submit"

# Tool lifecycle
HookRegistry.TOOL_PRE = "tool:pre"
HookRegistry.TOOL_POST = "tool:post"

# Orchestrator lifecycle
HookRegistry.ORCHESTRATOR_COMPLETE = "orchestrator:complete"

# Context management
HookRegistry.CONTEXT_PRE_COMPACT = "context:pre-compact"

# Agent delegation
HookRegistry.AGENT_SPAWN = "agent:spawn"
HookRegistry.AGENT_COMPLETE = "agent:complete"

# Notifications
HookRegistry.USER_NOTIFICATION = "user:notification"

# Decision events
HookRegistry.DECISION_TOOL_RESOLUTION = "decision:tool_resolution"
HookRegistry.DECISION_AGENT_RESOLUTION = "decision:agent_resolution"
HookRegistry.DECISION_CONTEXT_RESOLUTION = "decision:context_resolution"

# Error events
HookRegistry.ERROR_TOOL = "error:tool"
HookRegistry.ERROR_PROVIDER = "error:provider"
HookRegistry.ERROR_ORCHESTRATION = "error:orchestration"
```

---

## Event Data Fields

### Common Fields

All events include these standard fields:

```python
{
    "session_id": str,         # Always present
    "timestamp": str,          # ISO8601 timestamp
    # ... event-specific fields
}
```

### Default Fields

The registry supports default fields merged with all events:

```python
registry.set_default_fields(
    session_id="abc-123",
    environment="production"
)

# All events will include these fields automatically
```

---

## Hook Registration by Event

Register hooks for specific events:

```python
from amplifier_core.hooks import HookRegistry

registry = HookRegistry()

# Session lifecycle
registry.register(HookRegistry.SESSION_START, my_session_start_hook)
registry.register(HookRegistry.SESSION_END, my_session_end_hook)

# Tool lifecycle
registry.register(HookRegistry.TOOL_PRE, my_pre_tool_hook)
registry.register(HookRegistry.TOOL_POST, my_post_tool_hook)

# Orchestrator lifecycle
registry.register(HookRegistry.ORCHESTRATOR_COMPLETE, my_completion_hook)

# Notifications
registry.register(HookRegistry.USER_NOTIFICATION, my_notification_hook)
```

---

## See Also

- [Hooks API Reference](./HOOKS_API.md) - Complete HookResult API documentation
- [Hook Patterns Guide](../../docs/guides/HOOK_PATTERNS.md) - Common patterns and examples
- [Example Hooks](../../examples/hooks/) - Complete working implementations
