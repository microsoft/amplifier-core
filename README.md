# Amplifier Core

**The ultra-thin kernel of the Amplifier modular AI agent system.**

## Purpose

Amplifier Core is the stable, minimal kernel that provides mechanisms for:

- Module discovery and loading
- Lifecycle coordination
- Hook system and events
- Session and context management
- Stable public APIs and interfaces

Following the **Linux kernel model**: a tiny, stable center that rarely changes, with all policies and features implemented as replaceable modules at the edges.

## Design Philosophy

### Mechanisms, Not Policies

The kernel provides:

- ✅ **Mechanisms**: Module loading, event dispatch, capability enforcement
- ❌ **Not Policies**: Orchestration strategies, provider selection, tool behavior

### Stability Guarantees

- **Backward compatible**: Modules continue to work across kernel updates
- **Minimal dependencies**: Only essential libraries (pydantic, tomli, typing-extensions)
- **Single maintainer**: Can be understood and maintained by one person
- **Rare updates**: Changes are additive, not breaking

### What Belongs Here

Only code that **must** be in the kernel:

- Module discovery and loading from entry points
- Core interfaces (Tool, Provider, Context, Orchestrator, Hook)
- Session lifecycle management
- Event system for observability
- Minimal coordination logic

Everything else lives in modules.

## Installation

For complete Amplifier installation and usage, see the main repository:
**https://github.com/microsoft/amplifier** (branch: `next`)

## Core Concepts

### Modules

Everything is a module:

- **Providers** - AI service integrations (Anthropic Claude, OpenAI, Azure OpenAI, Ollama)
- **Tools** - Capabilities (filesystem, bash, web, search)
- **Orchestrators** - Execution loops (basic, streaming, events)
- **Contexts** - Memory management (simple, persistent)
- **Hooks** - Observability (logging, redaction, approval)

### Mount Plans

Configuration that defines what modules to load and how to configure them. Profiles compile to Mount Plans.

### Coordinator

Central registry where modules are mounted and discovered. Provides dependency injection for modules.

### Session

Execution context with mounted modules, manages conversation lifecycle.

## API Example

```python
from amplifier_core import AmplifierSession, ModuleLoader

# Load modules
loader = ModuleLoader()
mount_plan = {
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

# Create session
session = AmplifierSession(mount_plan, loader=loader)

# Use session
await session.initialize()
response = await session.execute("Hello!")
await session.cleanup()
```

## Module Development

**Quick example:**

```python
from amplifier_core.protocols import Tool

class MyTool(Tool):
    def get_schema(self):
        return {
            "name": "my_tool",
            "description": "Does something useful",
            "input_schema": {
                "type": "object",
                "properties": {
                    "param": {"type": "string"}
                }
            }
        }

    async def execute(self, **kwargs):
        return {"result": f"Processed: {kwargs['param']}"}
```

For complete module development guide, see:
**https://github.com/microsoft/amplifier** (branch: `next`)

## Documentation

- [Module Source Protocol](docs/MODULE_SOURCE_PROTOCOL.md) - Module loading specification
- [Session Fork Specification](docs/SESSION_FORK_SPECIFICATION.md) - Agent delegation
- [Coordinator Infrastructure](docs/COORDINATOR_INFRASTRUCTURE_CONTEXT.md) - Core architecture

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
