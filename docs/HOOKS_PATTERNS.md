# Hook Patterns Guide

Common patterns and anti-patterns for building effective hooks.

---

## Pattern Categories

1. [Context Injection Patterns](#context-injection-patterns) - Guiding agent with feedback
2. [Approval Gate Patterns](#approval-gate-patterns) - Dynamic permission control
3. [Output Control Patterns](#output-control-patterns) - Clean UX
4. [Combination Patterns](#combination-patterns) - Multiple capabilities together
5. [Anti-Patterns](#anti-patterns) - What to avoid

---

## Context Injection Patterns

### Pattern: Automated Linter Feedback Loop

**Problem**: Agent writes code with style violations

**Solution**: Hook runs linter, injects errors, agent fixes automatically

```python
async def linter_feedback(event: str, data: dict) -> HookResult:
    """Post-tool hook for automated linting feedback."""
    import subprocess

    # Only process file writes/edits
    if data.get("tool_name") not in ["Write", "Edit", "MultiEdit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]

    # Only Python files
    if not file_path.endswith(".py"):
        return HookResult(action="continue")

    # Run linter
    result = subprocess.run(
        ["ruff", "check", file_path],
        capture_output=True,
        text=True
    )

    if result.returncode != 0:
        # Parse errors
        errors = result.stderr.strip()

        return HookResult(
            action="inject_context",
            context_injection=f"Linter found issues in {file_path}:\n{errors}",
            context_injection_role="system",
            user_message=f"Found {len(errors.splitlines())} linting issues",
            user_message_level="warning",
            suppress_output=True
        )

    # All clean
    return HookResult(
        action="continue",
        user_message=f"✓ Linting passed: {file_path}",
        user_message_level="info",
        suppress_output=True
    )
```

**Benefits**:
- Autonomous correction (agent fixes without user intervention)
- Immediate feedback (same turn)
- Clean UX (verbose linter output hidden, summary shown)

---

### Pattern: Type Checker Integration

**Problem**: Agent creates code with type errors

**Solution**: Run type checker, inject errors, agent adds annotations

```python
async def type_checker(event: str, data: dict) -> HookResult:
    """Run pyright and inject type errors."""
    import subprocess
    import json

    if data.get("tool_name") not in ["Write", "Edit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]
    if not file_path.endswith(".py"):
        return HookResult(action="continue")

    # Run pyright
    result = subprocess.run(
        ["pyright", "--outputjson", file_path],
        capture_output=True,
        text=True
    )

    if result.returncode != 0:
        # Parse JSON output
        output = json.loads(result.stdout)
        diagnostics = output.get("generalDiagnostics", [])

        if diagnostics:
            # Format errors for agent
            errors = [
                f"Line {d['range']['start']['line']}: {d['message']}"
                for d in diagnostics
            ]

            return HookResult(
                action="inject_context",
                context_injection=f"Type errors in {file_path}:\n" + "\n".join(errors),
                user_message=f"Found {len(errors)} type errors",
                user_message_level="warning",
                suppress_output=True
            )

    return HookResult(action="continue")
```

---

### Pattern: Test Runner with Failure Details

**Problem**: Agent breaks tests, doesn't know what failed

**Solution**: Run tests, inject detailed failure info

```python
async def test_runner(event: str, data: dict) -> HookResult:
    """Run tests and inject failure details."""
    import subprocess

    # Only run after code changes
    if data.get("tool_name") not in ["Write", "Edit"]:
        return HookResult(action="continue")

    # Run tests
    result = subprocess.run(
        ["pytest", "-v", "--tb=short"],
        capture_output=True,
        text=True
    )

    if result.returncode != 0:
        # Parse failures (example parsing)
        failures = parse_pytest_output(result.stdout)

        return HookResult(
            action="inject_context",
            context_injection=f"Test failures:\n{format_failures(failures)}",
            user_message=f"{len(failures)} tests failing",
            user_message_level="error",
            suppress_output=True
        )

    return HookResult(
        action="continue",
        user_message="All tests passing",
        user_message_level="info",
        suppress_output=True
    )
```

---

### Pattern: Git Status on Session Start

**Problem**: Agent unaware of repo state

**Solution**: Inject git status on session start

```python
async def git_status_injector(event: str, data: dict) -> HookResult:
    """Inject git repository status on session start."""
    import subprocess

    # Run git status
    result = subprocess.run(
        ["git", "status", "--short"],
        capture_output=True,
        text=True,
        cwd=data.get("cwd", ".")
    )

    if result.stdout.strip():
        return HookResult(
            action="inject_context",
            context_injection=f"Git repository status:\n{result.stdout.strip()}",
            suppress_output=True
        )

    # Clean repo
    return HookResult(
        action="inject_context",
        context_injection="Git repository: Working tree clean",
        suppress_output=True
    )
```

**Register**:
```python
registry.register("session:start", git_status_injector)
```

---

## Approval Gate Patterns

### Pattern: Production File Protection

**Problem**: Don't want agent modifying production without approval

**Solution**: Pre-tool hook requests approval for production paths

```python
async def production_protection(event: str, data: dict) -> HookResult:
    """Require approval for production file writes."""

    if data.get("tool_name") not in ["Write", "Edit", "MultiEdit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]

    # Check for production paths
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

---

### Pattern: Cost Control

**Problem**: Don't want expensive LLM calls without approval

**Solution**: Pre-provider hook estimates cost, asks approval if high

```python
async def cost_control(event: str, data: dict) -> HookResult:
    """Ask approval for expensive LLM calls."""

    # Estimate cost
    model = data.get("model", "")
    tokens = data.get("estimated_tokens", 0)

    cost = estimate_cost(model, tokens)

    if cost > 1.0:  # Over $1
        return HookResult(
            action="ask_user",
            approval_prompt=f"This call will cost ~${cost:.2f} ({model}, {tokens:,} tokens). Continue?",
            approval_options=["Allow", "Deny"],
            approval_timeout=60.0,
            approval_default="deny"
        )

    return HookResult(action="continue")
```

---

### Pattern: Destructive Operation Approval

**Problem**: Agent might run destructive bash commands

**Solution**: Pre-tool hook detects destructive patterns, asks approval

```python
async def destructive_command_guard(event: str, data: dict) -> HookResult:
    """Require approval for destructive bash commands."""

    if data.get("tool_name") != "Bash":
        return HookResult(action="continue")

    command = data["tool_input"]["command"]

    # Detect destructive patterns
    destructive_patterns = [
        "rm -rf",
        "DROP DATABASE",
        "> /dev/",
        "mkfs",
        "dd if="
    ]

    for pattern in destructive_patterns:
        if pattern in command:
            return HookResult(
                action="ask_user",
                approval_prompt=f"Allow destructive command:\n{command}\n\nPattern detected: {pattern}",
                approval_options=["Confirm", "Cancel"],
                approval_timeout=120.0,
                approval_default="deny",
                reason=f"Destructive command requires confirmation: {pattern}"
            )

    return HookResult(action="continue")
```

---

## Output Control Patterns

### Pattern: Clean Progress Reports

**Problem**: Verbose hook processing clutters transcript

**Solution**: Show clean progress message, hide details

```python
async def progress_reporter(event: str, data: dict) -> HookResult:
    """Show clean progress, hide verbose logs."""

    # Do complex processing
    result = do_complex_analysis(data)  # Generates lots of logs

    # Show clean summary to user
    return HookResult(
        action="continue",
        user_message=f"Processed {result.count} items ({result.success_rate:.0%} success)",
        user_message_level="info",
        suppress_output=True  # Hide verbose processing logs
    )
```

**User sees**: "Processed 42 items (95% success)"
**User doesn't see**: Detailed processing logs

---

### Pattern: Severity-Appropriate Messages

**Problem**: All messages look the same

**Solution**: Use appropriate severity levels

```python
async def smart_messenger(event: str, data: dict) -> HookResult:
    """Show messages with appropriate severity."""

    issues = validate(data)

    if issues["critical"]:
        return HookResult(
            action="inject_context",  # Critical → inject + error message
            context_injection=f"Critical issues:\n{format(issues['critical'])}",
            user_message=f"CRITICAL: {len(issues['critical'])} issues found",
            user_message_level="error",  # Red/error styling
            suppress_output=True
        )
    elif issues["warnings"]:
        return HookResult(
            action="continue",  # Warnings → just inform
            user_message=f"Warning: {len(issues['warnings'])} minor issues",
            user_message_level="warning",  # Yellow/warning styling
            suppress_output=True
        )

    # All good
    return HookResult(
        action="continue",
        user_message="Validation passed",
        user_message_level="info",  # Green/info styling
        suppress_output=True
    )
```

---

## Combination Patterns

### Pattern: Validate → Inject → Inform

Combine validation, context injection, and user messaging:

```python
async def comprehensive_code_check(event: str, data: dict) -> HookResult:
    """Run linter + type checker + tests, provide comprehensive feedback."""

    if data.get("tool_name") not in ["Write", "Edit"]:
        return HookResult(action="continue")

    file_path = data["tool_input"]["file_path"]

    # Run all checks
    lint_issues = run_linter(file_path)
    type_issues = run_type_checker(file_path)
    test_failures = run_tests()

    # Combine results
    all_issues = lint_issues + type_issues + test_failures

    if all_issues:
        # Inject detailed feedback to agent
        feedback = format_all_issues(lint_issues, type_issues, test_failures)

        return HookResult(
            action="inject_context",
            context_injection=f"Code quality issues in {file_path}:\n{feedback}",
            user_message=f"Found {len(all_issues)} issues (lint: {len(lint_issues)}, types: {len(type_issues)}, tests: {len(test_failures)})",
            user_message_level="warning",
            suppress_output=True
        )

    # All checks passed
    return HookResult(
        action="continue",
        user_message=f"✓ All checks passed: {file_path}",
        user_message_level="info",
        suppress_output=True
    )
```

---

### Pattern: Approve → Log → Inject

Request approval, log decision, inject result:

```python
async def audit_and_approve(event: str, data: dict) -> HookResult:
    """Request approval with comprehensive audit trail."""

    operation = describe_operation(data)

    # High-risk operation
    if is_high_risk(operation):
        result = HookResult(
            action="ask_user",
            approval_prompt=f"Approve high-risk operation:\n{operation}",
            approval_options=["Allow with audit", "Deny"],
            approval_timeout=300.0,
            approval_default="deny"
        )

        # Note: Approval logging is automatic
        # Session coordinator logs all approval requests/decisions

        return result

    return HookResult(action="continue")
```

---

## Anti-Patterns

### Anti-Pattern 1: Injecting Too Much

**Problem**: Injecting massive content overwhelms agent

**Bad**:
```python
# Injecting entire file content
return HookResult(
    action="inject_context",
    context_injection=f"File content:\n{file.read_text()}"  # Could be 100KB!
)
```

**Good**:
```python
# Inject summary only
return HookResult(
    action="inject_context",
    context_injection=f"File has {line_count} lines, {error_count} linting errors"
)
```

**Rule**: Keep injections focused (< 1KB typical, 10KB max).

---

### Anti-Pattern 2: Vague Approval Prompts

**Problem**: User doesn't understand what they're approving

**Bad**:
```python
approval_prompt="Allow operation?"  # What operation?!
```

**Good**:
```python
approval_prompt=f"Allow write to production/database.py (12 lines changed, affects user schema)?"
```

**Rule**: Be specific - what, where, why, impact.

---

### Anti-Pattern 3: Blocking in Post-Tool Hooks

**Problem**: Can't block operation that already happened

**Bad**:
```python
# In tool:post hook
return HookResult(action="deny", ...)  # Tool already executed!
```

**Good**:
```python
# In tool:post hook
return HookResult(
    action="inject_context",
    context_injection="Tool execution violated constraint X. Please revert changes.",
    user_message="Constraint violation detected",
    user_message_level="error"
)
```

**Rule**: Use `tool:pre` for blocking, `tool:post` for feedback.

---

### Anti-Pattern 4: Forgetting Suppress Output

**Problem**: Verbose hook logs clutter transcript

**Bad**:
```python
# Prints 50 lines of processing logs
process_verbose_operation()
return HookResult(action="continue")  # User sees all 50 lines!
```

**Good**:
```python
# Prints 50 lines of processing logs
process_verbose_operation()
return HookResult(
    action="continue",
    user_message="Processing complete",  # User sees this clean message
    suppress_output=True  # Hide the 50 lines
)
```

**Rule**: If hook outputs verbose logs, use `suppress_output=True`.

---

### Anti-Pattern 5: Wrong Injection Role

**Problem**: Using "user" role confuses agent

**Bad**:
```python
return HookResult(
    action="inject_context",
    context_injection="Fix the linting errors",
    context_injection_role="user"  # Agent thinks user said this!
)
```

**Good**:
```python
return HookResult(
    action="inject_context",
    context_injection="Linter found errors: Line 42 too long",
    context_injection_role="system"  # Environmental feedback
)
```

**Rule**: Use "system" role for feedback (99% of cases).

---

### Anti-Pattern 6: Approval Without Timeout

**Problem**: Hook blocks forever if user doesn't respond

**Bad**:
```python
approval_timeout=float('inf')  # Blocks forever!
```

**Good**:
```python
approval_timeout=300.0,  # 5 minutes
approval_default="deny"  # Safe fallback
```

**Rule**: Always set reasonable timeout with safe default.

---

## Advanced Patterns

### Pattern: Conditional Injection

Only inject when relevant:

```python
async def smart_context_injector(event: str, data: dict) -> HookResult:
    """Inject context only when user asks about specific topics."""

    prompt = data.get("prompt", "").lower()

    # Inject git status only if user asks about changes
    if "change" in prompt or "modified" in prompt or "git" in prompt:
        status = get_git_status()
        return HookResult(
            action="inject_context",
            context_injection=f"Git status:\n{status}",
            suppress_output=True
        )

    # Inject date/time only if user asks about time
    if "date" in prompt or "time" in prompt or "when" in prompt:
        now = datetime.now()
        return HookResult(
            action="inject_context",
            context_injection=f"Current: {now.isoformat()}",
            suppress_output=True
        )

    return HookResult(action="continue")
```

---

### Pattern: Multi-Stage Validation

Run multiple validators, inject all feedback:

```python
async def multi_stage_validator(event: str, data: dict) -> HookResult:
    """Run multiple validators, combine feedback."""

    # Stage 1: Syntax
    syntax_errors = check_syntax(data)

    # Stage 2: Linting
    lint_issues = check_linting(data)

    # Stage 3: Type checking
    type_errors = check_types(data)

    # Combine all issues
    all_issues = []
    if syntax_errors:
        all_issues.append(f"Syntax errors:\n{format(syntax_errors)}")
    if lint_issues:
        all_issues.append(f"Linting issues:\n{format(lint_issues)}")
    if type_errors:
        all_issues.append(f"Type errors:\n{format(type_errors)}")

    if all_issues:
        return HookResult(
            action="inject_context",
            context_injection="\n\n".join(all_issues),
            user_message=f"Found {len(all_issues)} categories of issues",
            user_message_level="warning",
            suppress_output=True
        )

    return HookResult(action="continue")
```

---

### Pattern: Tiered Approval

Different approval policies for different risk levels:

```python
async def tiered_approval(event: str, data: dict) -> HookResult:
    """Different approval requirements by risk level."""

    file_path = data["tool_input"]["file_path"]
    risk_level = assess_risk(file_path)

    if risk_level == "critical":
        # Critical: Always ask, long timeout, no caching
        return HookResult(
            action="ask_user",
            approval_prompt=f"CRITICAL WRITE: {file_path}\nImpact: {describe_impact(file_path)}",
            approval_options=["Confirm", "Cancel"],  # No "Allow always"
            approval_timeout=600.0,  # 10 minutes
            approval_default="deny"
        )
    elif risk_level == "high":
        # High: Ask with caching
        return HookResult(
            action="ask_user",
            approval_prompt=f"High-risk write: {file_path}",
            approval_options=["Allow once", "Allow always", "Deny"],
            approval_timeout=300.0,
            approval_default="deny"
        )
    elif risk_level == "medium":
        # Medium: Inject warning but allow
        return HookResult(
            action="inject_context",
            context_injection=f"Note: Writing to {file_path} (medium risk - please review)",
            user_message=f"Medium-risk write: {file_path}",
            user_message_level="warning"
        )

    # Low risk: Allow silently
    return HookResult(action="continue")
```

---

## Performance Patterns

### Pattern: Fast Pre-Tool Validation

Pre-tool hooks should be fast (they block tool execution):

```python
async def fast_validator(event: str, data: dict) -> HookResult:
    """Quick validation without expensive operations."""

    file_path = data["tool_input"]["file_path"]

    # Fast checks only
    if file_path.startswith("/"):  # Absolute path
        return HookResult(action="deny", reason="Use relative paths only")

    if len(file_path) > 255:  # Path too long
        return HookResult(action="deny", reason="Path exceeds 255 characters")

    # Don't run expensive validations here!
    # Use tool:post for slow checks (linting, type checking)

    return HookResult(action="continue")
```

**Rule**: Pre-tool hooks fast (<10ms), post-tool hooks can be slow.

---

### Pattern: Async External Calls

Use asyncio for external calls:

```python
async def async_linter(event: str, data: dict) -> HookResult:
    """Run linter asynchronously."""
    import asyncio

    file_path = data["tool_input"]["file_path"]

    # Run linter asynchronously
    process = await asyncio.create_subprocess_exec(
        "ruff", "check", file_path,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE
    )

    stdout, stderr = await process.communicate()

    if process.returncode != 0:
        return HookResult(
            action="inject_context",
            context_injection=f"Linter issues:\n{stderr.decode()}",
            suppress_output=True
        )

    return HookResult(action="continue")
```

---

## Testing Patterns

### Pattern: Mock Event Data

Test hooks with fake event data:

```python
async def test_production_protection():
    """Test production protection hook."""

    # Mock event data
    event = "tool:pre"
    data = {
        "tool_name": "Write",
        "tool_input": {"file_path": "/production/config.py"}
    }

    # Execute hook
    result = await production_protection(event, data)

    # Verify approval requested
    assert result.action == "ask_user"
    assert "production" in result.approval_prompt.lower()
    assert result.approval_default == "deny"
```

---

## When to Use Each Capability

### Use Context Injection When:
- ✅ Agent needs automated feedback (linter, tests, type checker)
- ✅ Agent should be aware of system state (git status, time, resources)
- ✅ You want autonomous correction (agent fixes without user intervention)
- ✅ Feedback is actionable (agent can do something with it)

### Use Approval Gates When:
- ✅ Operation is high-risk (production files, destructive commands)
- ✅ Operation is expensive (costly API calls)
- ✅ User should make the decision (business logic, policy)
- ✅ Different contexts require different approvals (dev vs prod)

### Use Output Control When:
- ✅ Hook generates verbose logs (hide with `suppress_output`)
- ✅ User needs summary not details (show clean `user_message`)
- ✅ Severity matters (use `user_message_level` for color coding)
- ✅ Context injection and user message serve different purposes (agent sees context, user sees message)

### Use Simple "Continue" When:
- ✅ Just observing for logs (no action needed)
- ✅ Metrics collection (passive monitoring)
- ✅ Audit trails (record without interfering)

---

## See Also

- [Hooks API Reference](./HOOKS_API.md) - Complete API documentation
- [Hook Security Guide](./HOOK_SECURITY.md) - Security best practices
- [Example Hooks](../../examples/hooks/) - Working implementations
