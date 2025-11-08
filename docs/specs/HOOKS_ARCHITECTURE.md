# Hooks Architecture Specification

**Version**: 1.0.0
**Status**: Implemented

---

## Overview

The Amplifier hook system enables observation, validation, feedback injection, and approval control at key lifecycle points. Hooks are deterministic, priority-ordered functions that execute synchronously, with capabilities to block operations, modify data, inject context to the agent, request user approval, and control output visibility.

**Core capabilities**:
1. **Observe** - Monitor operations (logging, metrics, audit)
2. **Block** - Prevent operations (validation, security)
3. **Modify** - Transform data (preprocessing, enrichment)
4. **Inject Context** - Add feedback to agent's conversation (automated correction)
5. **Request Approval** - Ask user for permission (dynamic policies)
6. **Control Output** - Hide noise, show targeted messages (clean UX)

---

## System Components

```
┌─────────────────────────────────────────────────────────────┐
│  Hook Registry                                               │
│  • Event → Handler mapping                                   │
│  • Priority ordering                                         │
│  • Emit/collect methods                                      │
└───────────────┬─────────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────────────────────────┐
│  Session Coordinator                                         │
│  • Routes HookResult actions                                 │
│  • Delegates to subsystems                                   │
│  • Manages injection budget                                  │
│  • Handles approval flow                                     │
└─────┬───────┬───────┬────────┬──────────────────────────────┘
      │       │       │        │
      ▼       ▼       ▼        ▼
  ┌────────┬──────┬────────┬────────┐
  │Context │Approval│Display│ Audit  │
  │Manager │System │System │Logger  │
  └────────┴──────┴────────┴────────┘
```

### Component Responsibilities

**Hook Registry**:
- Register handlers for events
- Emit events to handlers (sequential by priority)
- Return aggregated HookResult
- Provide event constants

**Session Coordinator**:
- Orchestrates hook execution
- Routes HookResult actions to appropriate subsystems
- Manages context injection budget
- Delegates approval requests
- Controls output visibility

**Context Manager**:
- Stores conversation messages
- Accepts hook-injected messages
- Tags with provenance metadata
- Enforces size limits

**Approval System** (pluggable):
- Presents approval prompts to user
- Manages session-scoped cache
- Handles timeouts with safe defaults
- Returns user decisions

**Display System** (pluggable):
- Shows user messages (separate from context)
- Respects severity levels (info/warning/error)
- Filters based on suppress_output flag

**Audit Logger**:
- Records all hook actions
- Logs injections with provenance
- Logs approval requests and decisions
- Provides audit trail

---

## Execution Model

### Sequential Processing

Hooks execute **sequentially** by priority (lower number = earlier).

```python
handlers = [
    HookHandler(handler=hook_a, priority=0),  # Runs first
    HookHandler(handler=hook_b, priority=10), # Runs second
    HookHandler(handler=hook_c, priority=20)  # Runs third
]

for handler in sorted(handlers):  # Sorted by priority
    result = await handler.execute(event, current_data)

    if result.action == "deny":
        return result  # Short-circuit on deny

    if result.action == "modify":
        current_data = result.data  # Chain modifications
```

**Benefits**:
- Predictable execution order
- Data chaining works naturally
- Simpler reasoning
- Easier debugging

**Trade-off**: No parallelism (intentional - sequential is simpler).

### Short-Circuit on Deny

First "deny" action stops handler chain:

```python
# Hook 1 (priority=0): Returns deny
# Hook 2 (priority=10): Never executes
# Hook 3 (priority=20): Never executes

# Agent sees: "Operation denied by hook_1: [reason]"
```

### Data Modification Chaining

"Modify" actions chain through handlers:

```python
# Initial data: {"value": 10}

# Hook 1: Multiply by 2
result_1 = HookResult(action="modify", data={"value": 20})

# Hook 2: Add 5
result_2 = HookResult(action="modify", data={"value": 25})

# Hook 3: Log value
result_3 = HookResult(action="continue")

# Final data: {"value": 25}
```

---

## Context Injection Flow

### Architecture

```
Hook returns inject_context action
    ↓
Session coordinator validates (size < 10KB)
    ↓
Adds to context as message with specified role
    ↓
Tags with provenance (source hook, event, timestamp)
    ↓
Logs injection for audit
    ↓
Agent's next response includes injected content
    ↓
Agent responds to feedback
```

### Immediate Injection

Injections occur **immediately** - agent sees feedback within same turn:

```
Turn N:
  User: "Update config"
  Agent: [Write tool]
  Tool executes
  Hook injects: "Linter errors..."
  Agent sees injection
  Agent: [Edit tool to fix errors]
  All in same turn!
```

**Enables autonomous correction** without waiting for next user message.

### Injection Roles

**System role** (default, recommended):
```
System: Linter found issues in config.py:
  Line 42: E501 line too long
```

**User role** (simulate user input):
```
User: Please ensure all files pass linting
```

**Assistant role** (agent self-talk):
```
Assistant: I should check for security issues next
```

**Recommendation**: Use "system" for environmental feedback (99% of cases).

### Size Limits and Budgets

**Per-injection limit**: 10KB
**Per-turn budget**: 1,000 tokens recommended

Multiple injections in same turn are batched:

```
System: Hook feedback:

From linter_feedback (245 bytes):
[linter output]

From type_checker (180 bytes):
[type checker output]

From test_runner (320 bytes):
[test results]
```

---

## Approval Gates Flow

### Architecture

```
Hook returns ask_user action
    ↓
Session coordinator delegates to approval system
    ↓
Approval system checks cache (session-scoped)
    ↓
If cached: Return cached decision
If not cached: Show prompt to user
    ↓
User selects option (or timeout occurs)
    ↓
Decision returned to coordinator
    ↓
If "Deny": Block operation
If "Allow once": Proceed this time
If "Allow always": Cache + proceed
```

### Pluggable Approval System

Different environments use different approval implementations:

**CLI**: Rich terminal prompts with timeout
**Web UI**: Modal dialogs with WebSocket communication
**API**: Return approval request to client, await response

All implement same interface:

```python
class ApprovalSystem(Protocol):
    async def request_approval(
        prompt: str,
        options: list[str],
        timeout: float,
        default: Literal["allow", "deny"]
    ) -> str
```

### Session-Scoped Cache

"Allow always" decisions cached for session:

```python
cache_key = f"{hook_name}:{approval_prompt}"
cache[cache_key] = "Allow once"  # Simplified to "allow"

# Cleared on session end
```

**Rationale**: YAGNI - persistent cache adds complexity without proven need.

### Timeout Handling

On timeout, use `approval_default`:

```python
try:
    decision = await approval_system.request_approval(...)
except ApprovalTimeout:
    if result.approval_default == "deny":
        return HookResult(action="deny", reason="Timeout - denied by default")
    else:
        return HookResult(action="continue")  # Allow by default
```

**Security**: Default to "deny" for sensitive operations.

---

## Output Control Flow

### Architecture

```
Hook returns HookResult with output control fields
    ↓
Session coordinator processes:
  • suppress_output: Filter hook's stdout/stderr from transcript
  • user_message: Display to user via display system
    ↓
Hook output hidden/shown accordingly
User message displayed with severity level
```

### Message Routing

**Context injection** (to agent):
```python
if result.context_injection:
    self.context.add_message(
        role=result.context_injection_role,
        content=result.context_injection
    )
```

**User message** (to user):
```python
if result.user_message:
    self.display.show_message(
        message=result.user_message,
        level=result.user_message_level,
        source=f"hook:{hook_name}"
    )
```

**Separate channels**: Agent sees context injection, user sees user_message. They're independent.

### Output Suppression

**Suppresses hook's own output only**:

```python
if result.suppress_output:
    # Filter hook's stdout/stderr from transcript
    # Tool output still visible (security)
    pass
```

**Security principle**: Tools are primary actors (must be visible). Hooks are observers (can hide their own noise).

---

## Event Lifecycle with Hooks

### Complete Turn Flow

```
1. User submits prompt
   → PROMPT_SUBMIT event
   → Hooks can inject context (e.g., current date/time)

2. Orchestrator selects tool
   → DECISION_TOOL_RESOLUTION event
   → Hooks can log decision

3. Before tool execution
   → TOOL_PRE event
   → Hooks can validate, block, or request approval

4. Tool executes
   [Tool execution - not hookable]

5. After tool execution
   → TOOL_POST event
   → Hooks can validate output, inject feedback, show messages

6. Orchestrator completes
   → ORCHESTRATOR_COMPLETE event
   → Hooks can log stats, send notifications

7. User needs attention
   → USER_NOTIFICATION event
   → Hooks can send desktop alerts
```

### Error Path

```
Tool/Provider/Orchestrator error occurs
   → ERROR_* event
   → Hooks can log, alert, inject context
```

---

## Security Architecture

### Defense in Depth

**Layer 1: Size Limits**
- Context injection: 10KB max
- Prevents context flooding

**Layer 2: Audit Trail**
- All injections logged with provenance
- All approvals logged with decisions
- Tamper-evident log (JSONL append-only)

**Layer 3: Provenance Tagging**
- Injected messages tagged with source hook
- Traceability for debugging and security review

**Layer 4: Safe Defaults**
- Approval timeout → deny by default
- Injection failure → log and continue (non-interference)
- Output suppression → hook output only, not tools

**Layer 5: Kernel-Controlled Approval**
- Hooks request approval, can't grant it
- Approval system managed by kernel
- Cache managed by kernel, not hooks

### Attack Vector Mitigations

**Malicious context injection**:
- ✅ Size limits prevent overwhelming agent
- ✅ Audit trail shows what was injected
- ✅ Provenance tags identify source hook
- ✅ User can review injections (debug mode)

**Approval bypass**:
- ✅ Kernel controls approval system
- ✅ Hooks can only request, not grant
- ✅ Cache managed by kernel
- ✅ Audit logs all approval flows

**Output hiding**:
- ✅ Tools always visible (security)
- ✅ Hooks can only hide their own output
- ✅ User messages still shown
- ✅ Critical operations logged regardless

---

## Performance Characteristics

### Latency Impact

**Per hook execution**:
- Hook logic: Varies (1ms - 1s typical)
- Result processing: ~10ms
- Context injection: ~10ms
- Approval request: 1s - 5min (user-dependent)
- Output control: ~2ms

**Sequential execution** (3 hooks):
- Total: Sum of individual latencies
- No parallelism overhead
- Predictable timing

### Token Usage

**Context injection impact**:
- 10KB injection ≈ 2,500 tokens
- Recommended budget: 1,000 tokens/turn
- Warning if exceeded
- Does not block (soft limit)

**Typical usage**:
- Linter feedback: 100-500 tokens
- Type checker: 50-300 tokens
- Test results: 200-800 tokens

---

## Design Principles

### 1. Mechanism Not Policy (Kernel Philosophy)

Hooks provide **capabilities**, hook implementations decide **policies**.

**Kernel provides**:
- ✅ Ability to inject context
- ✅ Ability to request approval
- ✅ Ability to control output

**Hook decides**:
- ❌ What to inject
- ❌ When to ask approval
- ❌ What to show/hide

### 2. Non-Interference

Hook failures don't crash kernel or block operations (unless explicitly intended):

```python
try:
    result = await hook_handler(event, data)
except Exception as e:
    logger.error("Hook failed", hook=hook_name, error=str(e))
    # Continue with default result (action="continue")
```

**Exception**: Validation hooks intentionally return "deny" on validation failure.

### 3. Backward Compatibility

All new capabilities are **opt-in**:

```python
# Old hook (still works)
async def simple_hook(event, data):
    return HookResult(action="continue")

# New hook (opts into capabilities)
async def enhanced_hook(event, data):
    return HookResult(
        action="inject_context",
        context_injection="feedback"  # NEW - opt-in
    )
```

No breaking changes to existing hooks.

### 4. Observability

All hook actions are observable:

**Events logged**:
- Hook execution start/end
- Context injections (with provenance)
- Approval requests and decisions
- Output control actions
- Errors and timeouts

**Audit trail**: Complete record of all hook activity in JSONL log.

---

## Implementation Architecture

### Hook Registry (`amplifier_core/hooks.py`)

```python
class HookRegistry:
    """Manages lifecycle hooks with deterministic execution."""

    # Event constants
    SESSION_START = "session:start"
    TOOL_POST = "tool:post"
    ORCHESTRATOR_COMPLETE = "orchestrator:complete"
    # ... all events

    def __init__(self):
        self._handlers: dict[str, list[HookHandler]] = defaultdict(list)

    def register(self, event, handler, priority=0, name=None):
        """Register handler for event."""
        # Add to handlers, sort by priority
        # Return unregister function

    async def emit(self, event: str, data: dict) -> HookResult:
        """Emit event, execute all handlers sequentially."""
        # Execute handlers by priority
        # Short-circuit on deny
        # Chain data modifications
        # Return final result
```

### Session Coordinator (Integration Point)

```python
class SessionCoordinator:
    """Integrates hooks with session lifecycle."""

    def __init__(self, hooks, context, approval_system, display_system):
        self.hooks = hooks
        self.context = context
        self.approval_system = approval_system
        self.display_system = display_system
        self.injection_budget = 0  # Reset per turn

    async def execute_with_hooks(self, event: str, data: dict):
        """Execute operation with hook integration."""

        # Emit event to hooks
        result = await self.hooks.emit(event, data)

        # Process result
        await self.process_hook_result(result, event, hook_name="composite")

        return result

    async def process_hook_result(self, result, event, hook_name):
        """Route HookResult to appropriate subsystems."""

        # 1. Context injection
        if result.action == "inject_context" and result.context_injection:
            await self.handle_context_injection(result, hook_name, event)

        # 2. Approval request
        if result.action == "ask_user":
            return await self.handle_approval_request(result, hook_name)

        # 3. User message (separate from context)
        if result.user_message:
            await self.handle_user_message(result, hook_name)

        # 4. Output suppression
        if result.suppress_output:
            # Mark for filtering in transcript

        return result
```

### Context Injection Handler

```python
async def handle_context_injection(self, result, hook_name, event):
    """Process context injection with security checks."""

    content = result.context_injection
    role = result.context_injection_role

    # 1. Validate size
    if len(content) > MAX_INJECTION_SIZE:
        logger.error("Injection too large", hook=hook_name, size=len(content))
        raise ValueError(f"Context injection exceeds {MAX_INJECTION_SIZE} bytes")

    # 2. Check budget
    tokens = len(content) // 4  # Rough estimate
    if self.injection_budget + tokens > INJECTION_BUDGET_PER_TURN:
        logger.warning("Injection budget exceeded",
            current=self.injection_budget,
            attempted=tokens,
            budget=INJECTION_BUDGET_PER_TURN)

    self.injection_budget += tokens

    # 3. Add to context
    self.context.add_message(
        role=role,
        content=content,
        metadata={
            "source": "hook",
            "hook_name": hook_name,
            "event": event,
            "timestamp": datetime.now().isoformat()
        }
    )

    # 4. Audit log
    logger.info("hook_context_injection",
        hook=hook_name,
        event=event,
        size=len(content),
        role=role,
        tokens=tokens)
```

### Approval Request Handler

```python
async def handle_approval_request(self, result, hook_name):
    """Process approval request."""

    prompt = result.approval_prompt or "Allow this operation?"
    options = result.approval_options or ["Allow", "Deny"]

    # Log request
    logger.info("approval_requested",
        hook=hook_name,
        prompt=prompt,
        options=options)

    try:
        # Delegate to approval system
        decision = await self.approval_system.request_approval(
            prompt=prompt,
            options=options,
            timeout=result.approval_timeout,
            default=result.approval_default
        )

        # Log decision
        logger.info("approval_decision",
            hook=hook_name,
            decision=decision)

        # Process decision
        if decision == "Deny":
            return HookResult(action="deny", reason=f"User denied: {prompt}")

        return HookResult(action="continue")

    except ApprovalTimeout:
        # Log timeout
        logger.warning("approval_timeout", hook=hook_name)

        # Apply default
        if result.approval_default == "deny":
            return HookResult(action="deny", reason="Timeout - denied by default")

        return HookResult(action="continue")
```

---

## Event Data Schemas

See [HOOKS_EVENTS.md](./HOOKS_EVENTS.md) for complete event data schemas.

**Common pattern**:
```python
{
    "session_id": str,         # Always present
    "timestamp": str,          # ISO8601
    # ... event-specific fields
}
```

---

## Integration Points

### Orchestrators

All orchestrators emit `ORCHESTRATOR_COMPLETE` event:

```python
async def execute(self, prompt, context, providers, tools):
    """Execute orchestration loop."""

    # ... orchestration logic ...

    # Emit completion event
    await self.hooks.emit("orchestrator:complete", {
        "session_id": self.session_id,
        "orchestrator": self.__class__.__name__,
        "turn_count": self.turn_count,
        "duration_ms": int((time.time() - start_time) * 1000),
        "status": "success"
    })
```

### Context Manager

Accepts hook-sourced messages with metadata:

```python
def add_message(self, role, content, metadata=None):
    """Add message to context."""

    message = {
        "role": role,
        "content": content,
        "metadata": metadata or {},
        "timestamp": datetime.now().isoformat()
    }

    self.messages.append(message)

    # If from hook, log provenance
    if metadata and metadata.get("source") == "hook":
        logger.debug("message_from_hook",
            hook=metadata.get("hook_name"),
            role=role,
            size=len(content))
```

### Tools

Tools don't directly interact with hooks (hooks observe tool execution via events).

**Pattern**:
```
Tool executes → Session coordinator emits tool:pre → Hooks execute
Tool completes → Session coordinator emits tool:post → Hooks execute
```

---

## Error Handling

### Hook Execution Errors

```python
try:
    result = await hook_handler(event, data)
except Exception as e:
    logger.error("Hook execution failed", hook=hook_name, error=str(e))
    # Return safe default (don't block on hook failure)
    result = HookResult(action="continue")
```

**Principle**: Hook failures don't crash kernel (non-interference).

### Invalid HookResult

```python
if not isinstance(result, HookResult):
    logger.warning("Hook returned invalid result", hook=hook_name, type=type(result))
    result = HookResult(action="continue")  # Safe default
```

### Context Injection Failures

```python
try:
    self.context.add_message(...)
except Exception as e:
    logger.error("Context injection failed", hook=hook_name, error=str(e))
    # Don't block - show user message if provided
    if result.user_message:
        self.display.show_message(result.user_message, level="error")
```

---

## Testing Strategy

### Unit Tests

Test each component in isolation:

```python
# Test HookResult validation
def test_hook_result_fields():
    result = HookResult(
        action="inject_context",
        context_injection="test",
        context_injection_role="system"
    )
    assert result.action == "inject_context"
    assert result.context_injection_role == "system"

# Test size limit enforcement
def test_injection_size_limit():
    large_content = "x" * (MAX_INJECTION_SIZE + 1)
    with pytest.raises(ValueError, match="exceeds"):
        coordinator.handle_context_injection(
            HookResult(context_injection=large_content)
        )
```

### Integration Tests

Test complete flows:

```python
@pytest.mark.asyncio
async def test_linter_feedback_loop():
    """Test end-to-end linter feedback with context injection."""

    # 1. Register linter hook
    registry.register("tool:post", linter_feedback_hook)

    # 2. Simulate file write (bad code)
    await session.execute_tool("Write", {
        "file_path": "/tmp/test.py",
        "content": "x" * 200  # Line too long
    })

    # 3. Hook runs, injects feedback
    # 4. Agent sees feedback in context
    messages = session.context.get_messages()
    injection = [m for m in messages if m.get("metadata", {}).get("source") == "hook"]

    assert len(injection) == 1
    assert "line too long" in injection[0]["content"].lower()
```

### Real-World Tests

Test actual scenarios:

```python
async def test_production_protection_real_world():
    """Test production protection with actual approval flow."""

    # Mock approval system (auto-deny)
    approval_system = Mock ApprovalSystem(default_decision="Deny")

    # Attempt write to production file
    result = await session.execute_tool("Write", {
        "file_path": "/production/config.py",
        "content": "..."
    })

    # Verify blocked
    assert result.success == False
    assert "denied" in result.error.lower()

    # Verify approval was requested
    assert approval_system.request_approval.called
```

---

## Migration Guide

### From Context Manager Hack to Hook

**Old pattern** (git status hack in context manager):
```python
class ContextManager:
    def get_messages(self):
        # HACK: Inject git status
        status = subprocess.run(["git", "status"], ...)
        messages.insert(0, {"role": "system", "content": status})
        return messages
```

**New pattern** (clean hook):
```python
# In session initialization
registry.register("session:start", git_status_injector)

# Hook implementation
async def git_status_injector(event, data):
    status = subprocess.run(["git", "status", "--short"], capture_output=True)
    return HookResult(
        action="inject_context",
        context_injection=f"Git status:\n{status.stdout.decode()}",
        suppress_output=True
    )
```

**Benefits**:
- Cleaner separation of concerns
- Context manager doesn't have side effects
- Git status is observable (logged)
- Can be disabled without modifying core

---

## Future Enhancements

### Persistent Approval Cache (v2)

Store "Allow always" decisions across sessions:

```python
# ~/.amplifier/approval_rules.json
{
  "production_protection:production/*": "allow",
  "cost_control:openai:gpt-4": "deny"
}
```

**When to add**: If users request it (YAGNI for v1).

### Parallel Hook Execution (v3)

Execute hooks in parallel instead of sequential:

**Trade-offs**:
- Faster execution
- More complex (data modification chaining breaks)
- Less predictable

**When to add**: If sequential execution becomes bottleneck (unlikely).

---

## See Also

- [Context Injection Spec](./CONTEXT_INJECTION.md) - Detailed context injection architecture
- [Approval Gates Spec](./APPROVAL_GATES.md) - Detailed approval system design
- [Hooks API Reference](../../amplifier-core/docs/HOOKS_API.md) - Complete API documentation
- [Example Hooks](../../examples/hooks/) - Working implementations
