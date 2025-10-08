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

```bash
# Install from PyPI (when published)
pip install amplifier-core

# Install from source
pip install -e .

# Or use with uvx
uvx --from git+https://github.com/microsoft/amplifier-core amplifier-core --help
```

## Architecture

```
amplifier-core/
├── amplifier_core/
│   ├── __init__.py       # Public API
│   ├── interfaces.py     # Stable contracts for modules
│   ├── session.py        # Session lifecycle
│   ├── coordinator.py    # Module coordination
│   ├── loader.py         # Module discovery
│   ├── hooks.py          # Hook system
│   └── models.py         # Core data models
└── tests/                # Core functionality tests
```

## Core Interfaces

### Module Types

All modules implement one of these base interfaces:

**Tool** - Capabilities for agents (filesystem, bash, web, etc.)
```python
from amplifier_core import Tool

class MyTool(Tool):
    async def execute(self, **kwargs) -> Any:
        # Tool implementation
        pass
```

**Provider** - LLM integrations (Anthropic, OpenAI, etc.)
```python
from amplifier_core import Provider

class MyProvider(Provider):
    async def generate(self, prompt: str, **kwargs) -> str:
        # Provider implementation
        pass
```

**Context** - Conversation state management
```python
from amplifier_core import Context

class MyContext(Context):
    async def add_message(self, message: Message) -> None:
        # Context implementation
        pass
```

**Orchestrator** - Agent loop implementations
```python
from amplifier_core import Orchestrator

class MyOrchestrator(Orchestrator):
    async def run(self, session: Session) -> Result:
        # Orchestration implementation
        pass
```

**Hook** - Lifecycle event handlers
```python
from amplifier_core import Hook

class MyHook(Hook):
    async def on_tool_call(self, tool: str, args: dict) -> None:
        # Hook implementation
        pass
```

## Module Discovery

Modules are discovered via Python entry points:

```toml
# In module's pyproject.toml
[project.entry-points."amplifier.tools"]
my-tool = "my_package:mount"

[project.entry-points."amplifier.providers"]
my-provider = "my_package:mount"
```

The kernel automatically discovers and loads all installed modules.

## Usage Example

```python
from amplifier_core import AmplifierSession, SessionConfig

# Create session
config = SessionConfig(
    provider={"name": "anthropic", "model": "claude-sonnet-4.5"},
    tools=["filesystem", "bash"],
    orchestrator="loop-basic"
)

session = AmplifierSession(config)
await session.initialize()

# Execute with loaded modules
result = await session.execute("Your prompt here")
```

## For Module Developers

Key principles:
- Modules are self-contained packages
- Implement one of the core interfaces
- Register via entry points
- Follow semantic versioning
- Include tests and documentation

## Dependencies

Minimal by design:
- `pydantic>=2.0` - Data validation
- `tomli>=2.0` - TOML configuration
- `typing-extensions>=4.0` - Type hints

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
