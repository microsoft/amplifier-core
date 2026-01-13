# Amplifier Core

**The ultra-thin kernel of the Amplifier modular AI agent system.**

## Purpose

Amplifier Core provides the **mechanisms** for building modular AI agent systems. Following the Linux kernel model, it's a tiny, stable center (~2,600 lines) that rarely changes, with all policies and features implemented as replaceable modules at the edges.

**Core responsibilities**:

- Module discovery and loading
- Lifecycle coordination
- Hook system and events
- Session management
- Stable contracts and APIs

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ KERNEL (amplifier-core)                                     │
│ • Module loading          • Event system                    │
│ • Session lifecycle       • Coordinator                     │
│ • Minimal dependencies    • Stable contracts                │
└──────────────────┬──────────────────────────────────────────┘
                   │ protocols (Tool, Provider, etc.)
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ MODULES (Userspace - Swappable)                             │
│ • Providers: LLM backends (Anthropic, OpenAI, Azure, Ollama)│
│ • Tools: Capabilities (filesystem, bash, web, search)       │
│ • Orchestrators: Execution loops (basic, streaming, events) │
│ • Contexts: Memory management (simple, persistent)          │
│ • Hooks: Observability (logging, redaction, approval)       │
└─────────────────────────────────────────────────────────────┘
```

## Design Philosophy

### Mechanisms, Not Policies

The kernel provides **capabilities** without **decisions**:

| Kernel Provides (Mechanism) | Modules Decide (Policy) |
| --------------------------- | ----------------------- |
| Module loading              | Which modules to load   |
| Event emission              | What to log, where      |
| Session lifecycle           | Orchestration strategy  |
| Hook registration           | Security policies       |

**Litmus test**: "Could two teams want different behavior?" → If yes, it's policy → Module, not kernel.

### Stability Guarantees

- **Backward compatible**: Existing modules continue working across kernel updates
- **Minimal dependencies**: Only pydantic, tomli, pyyaml, typing-extensions
- **Single maintainer scope**: Can be understood by one person
- **Additive evolution**: Changes extend, don't break

## Installation

For complete Amplifier installation and usage:

**→ https://github.com/microsoft/amplifier**

## Core Concepts

### Session

Execution context with mounted modules and conversation state. Lifespan: `initialize()` → `execute()` → `cleanup()`.

### Mount Plan

Configuration dictionary specifying which modules to load and their configuration. Apps/bundles compile to Mount Plans.

### Coordinator

Infrastructure context providing session_id, config access, hooks, and mount points. Injected into all modules.

### Module Types

All modules use Python `Protocol` (structural typing, no inheritance required):

- **Provider** - LLM backends (name, complete(), parse_tool_calls(), get_info(), list_models())
- **Tool** - Agent capabilities (name, description, execute())
- **Orchestrator** - Execution loops (execute())
- **ContextManager** - Memory (add_message(), get_messages(), compact())
- **Hook** - Observability (__call__(event, data) → HookResult)

## API Example

```python
from amplifier_core import AmplifierSession

# Define mount plan (modules must be installed or discoverable)
config = {
    "session": {
        "orchestrator": "loop-basic",
        "context": "context-simple"
    },
    "providers": [
        {"module": "provider-anthropic"}
    ],
    "tools": [
        {"module": "tool-filesystem"},
        {"module": "tool-bash"}
    ]
}

# Create and use session
async with AmplifierSession(config) as session:
    response = await session.execute("List files in current directory")
```

## Module Development

Modules implement protocols via structural typing (duck typing):

```python
from amplifier_core.interfaces import Tool
from amplifier_core.models import ToolResult

class MyTool:
    """Implements Tool protocol without inheritance."""

    @property
    def name(self) -> str:
        return "my_tool"

    @property
    def description(self) -> str:
        return "Does something useful"

    async def execute(self, input: dict) -> ToolResult:
        """Execute tool with input dict."""
        return ToolResult(
            output=f"Processed: {input.get('param')}",
            error=None
        )

# Mount function (entry point)
async def mount(coordinator, config):
    tool = MyTool()
    await coordinator.mount("tools", tool, name="my_tool")

    async def cleanup():
        pass  # Cleanup resources

    return cleanup
```

**Entry point** (`pyproject.toml`):

```toml
[project.entry-points."amplifier.modules"]
my-tool = "amplifier_module_my_tool:mount"
```

For complete module development guide:
**→ https://github.com/microsoft/amplifier**

## Documentation

**Module Contracts** (Entry Point for Developers):

- [Contracts Index](docs/contracts/README.md) - Start here for module development
- [Provider Contract](docs/contracts/PROVIDER_CONTRACT.md) - LLM backend protocol
- [Tool Contract](docs/contracts/TOOL_CONTRACT.md) - Agent capability protocol
- [Hook Contract](docs/contracts/HOOK_CONTRACT.md) - Observability protocol
- [Orchestrator Contract](docs/contracts/ORCHESTRATOR_CONTRACT.md) - Execution loop protocol
- [Context Contract](docs/contracts/CONTEXT_CONTRACT.md) - Memory manager protocol

**Specifications** (Detailed Design):

- [Mount Plan Specification](docs/specs/MOUNT_PLAN_SPECIFICATION.md) - Configuration format
- [Provider Specification](docs/specs/PROVIDER_SPECIFICATION.md) - LLM provider details
- [Contribution Channels](docs/specs/CONTRIBUTION_CHANNELS.md) - Module contribution protocol

**Detailed Guides**:

- [Hooks API](docs/HOOKS_API.md) - Complete hook system reference
- [Session Forking](docs/SESSION_FORK_SPECIFICATION.md) - Child sessions for delegation
- [Module Source Protocol](docs/MODULE_SOURCE_PROTOCOL.md) - Custom module loading

**Philosophy**:

- [Design Philosophy](docs/DESIGN_PHILOSOPHY.md) - Kernel principles and patterns

## Testing

```bash
cd amplifier-core
uv run pytest
uv run pytest --cov
```

## Contributing

> [!NOTE]
> This project is not currently accepting external contributions, but we're actively working toward opening this up. We value community input and look forward to collaborating in the future. For now, feel free to fork and experiment!

Most contributions require you to agree to a
Contributor License Agreement (CLA) declaring that you have the right to, and actually do, grant us
the rights to use your contribution. For details, visit [Contributor License Agreements](https://cla.opensource.microsoft.com).

When you submit a pull request, a CLA bot will automatically determine whether you need to provide
a CLA and decorate the PR appropriately (e.g., status check, comment). Simply follow the instructions
provided by the bot. You will only need to do this once across all repos using our CLA.

This project has adopted the [Microsoft Open Source Code of Conduct](https://opensource.microsoft.com/codeofconduct/).
For more information see the [Code of Conduct FAQ](https://opensource.microsoft.com/codeofconduct/faq/) or
contact [opencode@microsoft.com](mailto:opencode@microsoft.com) with any additional questions or comments.

## Trademarks

This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft
trademarks or logos is subject to and must follow
[Microsoft's Trademark & Brand Guidelines](https://www.microsoft.com/legal/intellectualproperty/trademarks/usage/general).
Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion or imply Microsoft sponsorship.
Any use of third-party trademarks or logos are subject to those third-party's policies.
