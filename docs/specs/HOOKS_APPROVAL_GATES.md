# Approval Gates Specification

**Version**: 1.0.0
**Status**: Implemented

---

## Purpose

Enable hooks to request user approval for operations, providing dynamic permission logic that goes beyond the kernel's built-in approval system. Hooks can ask user-defined questions with custom options and timeout behavior.

**Key use case**: Production protection hook asks "Allow write to production/config.py?" before proceeding.

---

## Architecture

### Flow Diagram

```
Hook executes → Returns HookResult(action="ask_user")
    ↓
Session Coordinator receives result
    ↓
Delegates to Approval System (pluggable interface)
    ↓
CLI Approval System shows prompt to user
    ↓
User selects option (or timeout occurs)
    ↓
Decision returned to coordinator
    ↓
If "Deny" → Block operation (return deny result)
If "Allow once" → Proceed this time
If "Allow always" → Cache decision, proceed
    ↓
Operation continues or is blocked
```

### Components

**1. Hook Handler** - Returns HookResult requesting approval
**2. Session Coordinator** - Mediates approval flow
**3. Approval System** - Pluggable interface (CLI, Web, API)
**4. Approval Cache** - Session-scoped decision storage
**5. Audit Logger** - Records all approval requests and decisions

---

## Approval System Interface

### Protocol

```python
from typing import Protocol, Literal

class ApprovalSystem(Protocol):
    """Pluggable approval interface for different environments."""

    async def request_approval(
        self,
        prompt: str,
        options: list[str],
        timeout: float,
        default: Literal["allow", "deny"]
    ) -> str:
        """
        Request user approval with timeout.

        Args:
            prompt: Question to ask user
            options: Available choices
            timeout: Seconds to wait for response
            default: Action to take on timeout

        Returns:
            Selected option string (one of options)

        Raises:
            ApprovalTimeout: User didn't respond within timeout
        """
        ...
```

### CLI Implementation

```python
from rich.console import Console
from rich.prompt import Prompt
import asyncio

class CLIApprovalSystem:
    """Terminal-based approval with Rich formatting."""

    def __init__(self):
        self.console = Console()
        self.cache: dict[str, str] = {}  # Session-scoped

    async def request_approval(
        self,
        prompt: str,
        options: list[str],
        timeout: float,
        default: Literal["allow", "deny"]
    ) -> str:
        """Show approval prompt in terminal with timeout."""

        # Check cache (for "Allow always" decisions)
        cache_key = f"{prompt}:{','.join(options)}"
        if cache_key in self.cache:
            cached_decision = self.cache[cache_key]
            self.console.print(f"[dim]Using cached approval: {cached_decision}[/dim]")
            return cached_decision

        # Display prompt
        self.console.print()
        self.console.print(f"[yellow]⚠️  Approval Required[/yellow]")
        self.console.print(f"\n{prompt}")
        self.console.print(f"\nOptions: {', '.join(options)}")
        self.console.print(f"[dim]Timeout in {timeout}s, defaults to: {default}[/dim]")
        self.console.print()

        # Get user input with timeout
        try:
            async with asyncio.timeout(timeout):
                choice = await asyncio.to_thread(
                    Prompt.ask,
                    "Your choice",
                    choices=options
                )

                # Cache "Allow always" decisions
                if choice == "Allow always":
                    self.cache[cache_key] = "Allow once"  # Cache as "Allow"
                    self.console.print("[green]✓ Cached: Will allow this operation in future[/green]")

                return choice

        except asyncio.TimeoutError:
            self.console.print(f"\n[yellow]⏱ Timeout - using default: {default}[/yellow]")
            raise ApprovalTimeout(f"User approval timeout after {timeout}s")
```

### Web UI Implementation

```python
class WebUIApprovalSystem:
    """Web-based approval with HTTP callbacks."""

    async def request_approval(self, prompt, options, timeout, default):
        """Show modal dialog in web UI."""
        # Send approval request via WebSocket
        await self.websocket.send({
            "type": "approval_request",
            "prompt": prompt,
            "options": options,
            "timeout": timeout
        })

        # Wait for user response
        try:
            async with asyncio.timeout(timeout):
                response = await self.approval_queue.get()
                return response["choice"]
        except asyncio.TimeoutError:
            raise ApprovalTimeout(f"User approval timeout")
```

---

## Approval Cache

### Session-Scoped Cache

**Scope**: In-memory, cleared on session end

**Key**: `f"{approval_prompt}:{','.join(options)}"`

**Value**: User's decision

**Behavior**:
- "Allow once" → Execute once, not cached
- "Allow always" → Execute and cache as "Allow once" for this session
- "Deny" → Block this time, not cached (user can approve next time)

**Implementation**:
```python
class ApprovalCache:
    """Session-scoped approval decision cache."""

    def __init__(self):
        self._cache: dict[str, str] = {}

    def check(self, prompt: str, options: list[str]) -> str | None:
        """Check if decision exists for this prompt."""
        key = f"{prompt}:{','.join(options)}"
        return self._cache.get(key)

    def store(self, prompt: str, options: list[str], decision: str):
        """Store decision for this session."""
        if decision == "Allow always":
            key = f"{prompt}:{','.join(options)}"
            self._cache[key] = "Allow once"  # Cache as simple "allow"
```

**Rationale**: YAGNI - persistent cache adds complexity without proven need. Session scope is sufficient for v1.

---

## Timeout Behavior

### Default: Deny on Timeout

```python
HookResult(
    action="ask_user",
    approval_timeout=300.0,     # 5 minutes
    approval_default="deny"     # Safe default
)
```

**Security principle**: Default to denying operations when user doesn't respond. Safer for sensitive operations.

### Allow on Timeout (Rare)

```python
HookResult(
    action="ask_user",
    approval_timeout=60.0,      # 1 minute
    approval_default="allow"    # Allow low-risk operations
)
```

**Use case**: Low-risk operations where blocking is more disruptive than allowing.

**Example**: Approval for logging, non-destructive reads, caching operations.

---

## Audit Trail

Every approval request and decision is logged.

### Request Log

```json
{
  "event": "approval:requested",
  "session_id": "abc-123",
  "hook_name": "production_protection",
  "prompt": "Allow write to production/config.py?",
  "options": ["Allow once", "Allow always", "Deny"],
  "timeout": 300.0,
  "default": "deny",
  "timestamp": "2025-11-07T12:34:56Z"
}
```

### Decision Log

```json
{
  "event": "approval:decision",
  "session_id": "abc-123",
  "hook_name": "production_protection",
  "prompt": "Allow write to production/config.py?",
  "decision": "Allow once",
  "decision_time_ms": 2500,
  "cached": false,
  "timestamp": "2025-11-07T12:34:58Z"
}
```

### Timeout Log

```json
{
  "event": "approval:timeout",
  "session_id": "abc-123",
  "hook_name": "production_protection",
  "prompt": "Allow write to production/config.py?",
  "default_taken": "deny",
  "timeout": 300.0,
  "timestamp": "2025-11-07T12:39:56Z"
}
```

---

## Integration with Session Coordinator

```python
async def handle_approval_request(self, result: HookResult, hook_name: str) -> HookResult:
    """Process approval request from hook."""

    prompt = result.approval_prompt or "Allow this operation?"
    options = result.approval_options or ["Allow", "Deny"]
    timeout = result.approval_timeout
    default = result.approval_default

    # Log request
    logger.info("approval_requested",
        hook=hook_name,
        prompt=prompt,
        timeout=timeout,
        default=default)

    try:
        # Request approval from user
        decision = await self.approval_system.request_approval(
            prompt=prompt,
            options=options,
            timeout=timeout,
            default=default
        )

        # Log decision
        logger.info("approval_decision",
            hook=hook_name,
            decision=decision,
            cached=False)

        # Process decision
        if decision == "Deny":
            return HookResult(
                action="deny",
                reason=f"User denied approval: {prompt}"
            )

        # "Allow once" or "Allow always" → proceed
        return HookResult(action="continue")

    except ApprovalTimeout:
        # Log timeout
        logger.warning("approval_timeout",
            hook=hook_name,
            default=default)

        # Apply default
        if default == "deny":
            return HookResult(
                action="deny",
                reason=f"Approval timeout - denied by default: {prompt}"
            )
        else:
            return HookResult(action="continue")
```

---

## Error Scenarios

### Scenario 1: Approval System Unavailable

```python
try:
    decision = await self.approval_system.request_approval(...)
except Exception as e:
    logger.error("Approval system failed", error=str(e))

    # Safe default: deny
    return HookResult(
        action="deny",
        reason=f"Approval system error - denied by default"
    )
```

**Principle**: Fail closed. If can't ask user, default to deny.

### Scenario 2: User Interrupts

```python
try:
    decision = await self.approval_system.request_approval(...)
except KeyboardInterrupt:
    # User pressed Ctrl+C during approval
    logger.info("Approval interrupted by user")

    return HookResult(
        action="deny",
        reason="Approval cancelled by user"
    )
```

### Scenario 3: Invalid Option Returned

```python
decision = await self.approval_system.request_approval(
    options=["Allow once", "Allow always", "Deny"]
)

if decision not in options:
    logger.error("Invalid approval decision", decision=decision, options=options)
    # Treat as deny
    return HookResult(
        action="deny",
        reason="Invalid approval response"
    )
```

---

## Use Cases

### Use Case 1: Production File Protection

**Scenario**: Prevent accidental writes to production files without explicit user approval.

**Implementation**: See `examples/hooks/production_protection.py`

**Flow**:
1. Agent uses Write tool on `production/config.py`
2. Pre-tool hook detects production path
3. Hook requests approval: "Allow write to production/config.py?"
4. User sees prompt with options: ["Allow once", "Allow always", "Deny"]
5. User selects "Allow once"
6. Write proceeds

**If user selects "Deny"**: Write blocked, agent receives denial reason, can try alternative approach.

**If user selects "Allow always"**: Write proceeds, decision cached for session (future writes to production files auto-allowed).

### Use Case 2: Cost Control

**Scenario**: Ask approval before expensive API calls.

**Flow**:
1. Agent about to call expensive LLM with 50K context
2. Pre-provider hook calculates estimated cost ($5)
3. Hook requests approval: "This call will cost ~$5. Continue?"
4. User approves or denies
5. Call proceeds or is blocked

### Use Case 3: Destructive Operations

**Scenario**: Require approval for irreversible operations.

**Flow**:
1. Agent uses Bash tool: `rm -rf /tmp/important-cache/`
2. Pre-tool hook detects destructive command
3. Hook requests approval: "Allow destructive operation: rm -rf /tmp/important-cache/?"
4. User reviews and decides
5. Command executes or is blocked

---

## Security Considerations

### 1. Approval Bypass Prevention

**Risk**: Hook claims approval without actually asking user.

**Mitigation**: Approval system is kernel-controlled. Hooks can only REQUEST approval, not grant it.

```python
# Hook CANNOT do this:
return HookResult(action="continue")  # Bypasses approval

# Hook MUST do this:
return HookResult(action="ask_user", ...)  # Kernel handles approval
```

### 2. Cache Poisoning

**Risk**: Malicious hook creates fake cached approvals.

**Mitigation**: Cache is managed by ApprovalSystem, not accessible to hooks.

### 3. Approval Prompt Manipulation

**Risk**: Misleading prompts trick user into approving dangerous operations.

**Mitigation**: Audit trail logs all approval prompts. Review logs for suspicious patterns.

**Best practice**: Make prompts explicit and honest.

```python
# ❌ BAD: Vague or misleading
approval_prompt="Update file?"  # Which file? What changes?

# ✅ GOOD: Specific and clear
approval_prompt="Allow write to production/database.py with schema migration?"
```

---

## Performance

### Blocking Nature

Approval requests are **blocking operations** - orchestrator waits for user response.

**Latency**:
- Minimum: User decision time (1-10 seconds)
- Maximum: Timeout (default 5 minutes)
- Typical: 5-15 seconds

**Recommendation**: Use approval gates sparingly for high-risk operations only. Don't use for routine operations.

### Timeout Strategies

**Short timeout** (60s): Low-risk operations
```python
approval_timeout=60.0,          # 1 minute
approval_default="allow"        # Safe to proceed if user doesn't respond
```

**Long timeout** (300s): High-risk operations
```python
approval_timeout=300.0,         # 5 minutes
approval_default="deny"         # Must wait for explicit approval
```

**Very long timeout** (3600s): Critical operations
```python
approval_timeout=3600.0,        # 1 hour
approval_default="deny"         # User has time to review carefully
```

---

## Implementation Details

### In Session Coordinator

```python
async def execute_with_hooks(self, event: str, data: dict):
    """Execute hooks and handle approval requests."""

    # Emit event to hooks
    result = await self.hooks.emit(event, data)

    # Handle approval request
    if result.action == "ask_user":
        approval_result = await self.handle_approval(result, hook_name)
        if approval_result.action == "deny":
            # User denied or timeout with deny default
            return approval_result

    # Handle other actions (inject_context, modify, etc.)
    # ...
```

### Approval Handler

```python
async def handle_approval(self, result: HookResult, hook_name: str) -> HookResult:
    """Process approval request."""

    prompt = result.approval_prompt or "Allow this operation?"
    options = result.approval_options or ["Allow", "Deny"]

    # Generate cache key
    cache_key = f"{hook_name}:{prompt}"

    # Check cache
    if cache_key in self.approval_cache:
        cached = self.approval_cache[cache_key]
        logger.info("approval_cached", hook=hook_name, decision=cached)
        return HookResult(action="continue")

    # Log request
    logger.info("approval_requested",
        hook=hook_name,
        prompt=prompt,
        options=options,
        timeout=result.approval_timeout)

    try:
        # Request approval
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

        # Handle "Allow always"
        if decision == "Allow always":
            self.approval_cache[cache_key] = decision
            return HookResult(action="continue")

        # Handle "Deny"
        if decision == "Deny":
            return HookResult(
                action="deny",
                reason=f"User denied: {prompt}"
            )

        # "Allow once"
        return HookResult(action="continue")

    except ApprovalTimeout:
        # Log timeout
        logger.warning("approval_timeout",
            hook=hook_name,
            default=result.approval_default)

        # Apply default
        if result.approval_default == "deny":
            return HookResult(
                action="deny",
                reason=f"Approval timeout - denied: {prompt}"
            )

        return HookResult(action="continue")
```

---

## Best Practices

### 1. Clear Prompts

Make approval prompts explicit about what's being approved:

```python
# ❌ Vague
"Allow operation?"

# ✅ Specific
"Allow write to production/config.py (12 lines changed)?"
```

### 2. Appropriate Options

Choose options that match the operation:

```python
# For file writes
options=["Allow once", "Allow always", "Deny"]

# For destructive operations
options=["Confirm deletion", "Cancel"]

# For cost decisions
options=["Approve ($5 estimated)", "Deny"]
```

### 3. Reasonable Timeouts

Match timeout to operation urgency:

```python
# Routine operations
approval_timeout=60.0  # 1 minute

# Important operations
approval_timeout=300.0  # 5 minutes (default)

# Critical operations
approval_timeout=3600.0  # 1 hour
```

### 4. Safe Defaults

Use "deny" default for security-sensitive operations:

```python
# Production writes
approval_default="deny"  # Must have explicit approval

# Read operations
approval_default="allow"  # Safe to proceed on timeout
```

---

## Testing

### Unit Tests

```python
@pytest.mark.asyncio
async def test_approval_flow():
    """Test approval gate flow."""
    # Arrange
    approval_system = CLIApprovalSystem()
    result = HookResult(
        action="ask_user",
        approval_prompt="Allow test operation?",
        approval_options=["Allow", "Deny"],
        approval_timeout=1.0,  # Short for testing
        approval_default="deny"
    )

    # Mock user input
    with patch('builtins.input', return_value="Allow"):
        decision = await approval_system.request_approval(
            prompt=result.approval_prompt,
            options=result.approval_options,
            timeout=result.approval_timeout,
            default=result.approval_default
        )

    # Assert
    assert decision == "Allow"
```

### Integration Tests

Test complete approval flow with hooks:

```python
@pytest.mark.asyncio
async def test_production_protection():
    """Test production file protection with approval."""
    # Register hook
    registry.register("tool:pre", production_protection_hook)

    # Simulate write to production file
    event_data = {
        "tool_name": "Write",
        "tool_input": {"file_path": "/production/config.py"}
    }

    # Mock approval (user denies)
    with patch.object(approval_system, 'request_approval',
                     side_effect=ApprovalTimeout()):
        result = await registry.emit("tool:pre", event_data)

    # Assert blocked
    assert result.action == "deny"
    assert "denied" in result.reason.lower()
```

---

## See Also

- [Hooks API Reference](../../amplifier-core/docs/HOOKS_API.md) - Complete HookResult API
- [Hook Security Guide](../guides/HOOK_SECURITY.md) - Security best practices
- [Example: Production Protection](../../examples/hooks/production_protection.py) - Working implementation
