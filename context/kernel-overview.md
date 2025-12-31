# Amplifier Kernel Overview

## What is the Kernel?

The Amplifier kernel (`amplifier-core`) is an ultra-thin layer (~2,600 lines) that provides MECHANISMS only. It follows the Linux kernel philosophy:

> "The center stays still so the edges can move fast."

## Core Tenets

### 1. Mechanism, Not Policy
The kernel exposes **capabilities** and **stable contracts**. **Decisions about behavior** belong outside the kernel.

**If something can plausibly be a policy, it should live in a module, not in core.**

### 2. Small, Stable, and Boring
The kernel is intentionally minimal and changes rarely. It's easy to reason about. Favor deletion over accretion. Prefer saying "no" to keep the center still.

### 3. Don't Break Modules
Backward compatibility in kernel interfaces is sacred. Breaking changes to core contracts are an absolute last resort.

### 4. Policy Lives at the Edges
Scheduling strategies, orchestration styles, provider choices, safety policies - all belong in modules. The kernel provides only hook points and contracts.

## What the Kernel Provides

### Session Lifecycle
- `create_session()` - Create execution context
- Module mounting/unmounting
- Lifecycle events

### Coordinator
Infrastructure context carrying:
- session_id
- hooks reference
- mount points
- capability checks

### Event System
- Canonical event emission
- Hook registration and dispatch
- Non-blocking observability

### Module Loading
- Protocol validation
- Entry point discovery
- Capability enforcement

### Stable Contracts
- Provider protocol: `complete(request: ChatRequest) → ChatResponse`
- Tool protocol: `execute(input: dict) → ToolResult`
- Orchestrator protocol: `execute(prompt, context, providers, tools, hooks) → str`
- Hook protocol: `__call__(event, data) → HookResult`
- ContextManager protocol: `add_message/get_messages_for_request/get_messages/set_messages/clear`

## What the Kernel Does NOT Provide

- ❌ Which modules to use (app layer decides)
- ❌ Which provider to call (policy)
- ❌ How to orchestrate execution (orchestrator module)
- ❌ What to log or how (hooks module)
- ❌ Security policies (hook modules)
- ❌ Response formatting (policy)

## The Litmus Test

Before adding anything to the kernel, ask:

> "Could two reasonable teams want different behavior here?"

- **YES** → It's POLICY → Keep it out of kernel
- **NO** → It might be mechanism (but prove it with ≥2 modules first)

## Evolution Rules

1. **Additive first** - Extend contracts without breaking them
2. **Two-implementation rule** - Don't promote to kernel until ≥2 modules converge on the need
3. **Deprecation discipline** - Clear migration paths when removal needed
4. **Spec before code** - Kernel changes begin with a spec
5. **No policy leaks** - If a change drifts toward policy, move it to a module

## Module Protocols

### Provider Protocol
```python
@property
def name(self) -> str

def get_info(self) -> ProviderInfo

async def list_models(self) -> list[ModelInfo]

async def complete(self, request: ChatRequest, **kwargs) -> ChatResponse

def parse_tool_calls(self, response: ChatResponse) -> list[ToolCall]
```

### Tool Protocol
```python
@property
def name(self) -> str

@property
def description(self) -> str

async def execute(self, input: dict[str, Any]) -> ToolResult
```

### Orchestrator Protocol
```python
async def execute(
    self,
    prompt: str,
    context: ContextManager,
    providers: dict[str, Provider],
    tools: dict[str, Tool],
    hooks: HookRegistry,
) -> str
```

### Hook Protocol
```python
async def __call__(self, event: str, data: dict[str, Any]) -> HookResult
```

### ContextManager Protocol
```python
async def add_message(self, message: dict[str, Any]) -> None

async def get_messages_for_request(
    self,
    token_budget: int | None = None,
    provider: Any | None = None,
) -> list[dict[str, Any]]

async def get_messages(self) -> list[dict[str, Any]]

async def set_messages(self, messages: list[dict[str, Any]]) -> None

async def clear(self) -> None
```

## For Module Developers

When building a module:

1. **Understand the protocol** - Read the relevant specification
2. **Stay in your lane** - Implement the protocol, don't try to be clever
3. **Emit events** - Use the event system for observability
4. **Fail gracefully** - Your failures shouldn't crash the kernel
5. **Test in isolation** - Modules should work independently
