# Amplifier Module Catalog

Community-maintained catalog of Amplifier modules. This list is for **human discovery only** - modules are resolved directly from Git URLs, not from this file.

> **Note**: A module does not need to be listed here to work. This catalog helps people find useful modules.

## Official Modules (microsoft/)

### Orchestrators

| Module | Description | Source |
|--------|-------------|--------|
| `loop-basic` | Simple request/response loop | [microsoft/amplifier-module-loop-basic](https://github.com/microsoft/amplifier-module-loop-basic) |
| `loop-streaming` | Streaming with extended thinking | [microsoft/amplifier-module-loop-streaming](https://github.com/microsoft/amplifier-module-loop-streaming) |

### Context Managers

| Module | Description | Source |
|--------|-------------|--------|
| `context-simple` | Basic context with auto-compaction | [microsoft/amplifier-module-context-simple](https://github.com/microsoft/amplifier-module-context-simple) |

### Tools

| Module | Description | Source |
|--------|-------------|--------|
| `tool-filesystem` | File read/write/search operations | [microsoft/amplifier-module-tool-filesystem](https://github.com/microsoft/amplifier-module-tool-filesystem) |
| `tool-bash` | Shell command execution | [microsoft/amplifier-module-tool-bash](https://github.com/microsoft/amplifier-module-tool-bash) |
| `tool-web` | HTTP requests and web fetching | [microsoft/amplifier-module-tool-web](https://github.com/microsoft/amplifier-module-tool-web) |
| `tool-search` | Web search capabilities | [microsoft/amplifier-module-tool-search](https://github.com/microsoft/amplifier-module-tool-search) |
| `tool-task` | Task/agent spawning | [microsoft/amplifier-module-tool-task](https://github.com/microsoft/amplifier-module-tool-task) |
| `tool-mcp` | MCP server bridge | [microsoft/amplifier-module-tool-mcp](https://github.com/microsoft/amplifier-module-tool-mcp) |

### Providers

| Module | Description | Source |
|--------|-------------|--------|
| `provider-anthropic` | Claude models via Anthropic API | [microsoft/amplifier-module-provider-anthropic](https://github.com/microsoft/amplifier-module-provider-anthropic) |
| `provider-openai` | GPT models via OpenAI API | [microsoft/amplifier-module-provider-openai](https://github.com/microsoft/amplifier-module-provider-openai) |

---

## Community Modules

### Tools

| Module | Description | Source |
|--------|-------------|--------|
| `tool-memory` | Persistent memory across sessions (SQLite) | [michaeljabbour/amplifier-module-tool-memory](https://github.com/michaeljabbour/amplifier-module-tool-memory) |

### Hooks

| Module | Description | Source |
|--------|-------------|--------|
| `hooks-event-broadcast` | Transport-agnostic event broadcast via capability injection | [michaeljabbour/amplifier-module-hooks-event-broadcast](https://github.com/michaeljabbour/amplifier-module-hooks-event-broadcast) |

---

## Adding Your Module

To add a module to this catalog:

1. Ensure your module follows the [module protocol](docs/MODULE_SOURCE_PROTOCOL.md)
2. Tag a release (e.g., `v1.0.0`)
3. Open a PR adding your module to the appropriate section

**Requirements for listing:**
- Valid `pyproject.toml` with entry point
- Working `mount()` function
- Basic documentation (README)
- At least one tagged release

**Not required:**
- Approval from maintainers
- Specific test coverage
- Any particular license

This catalog is for discovery - resolution happens directly from Git.

---

## Using Modules

In a bundle or profile:

```yaml
tools:
  - module: tool-memory
    source: git+https://github.com/michaeljabbour/amplifier-module-tool-memory@v0.1.0
    config:
      max_memories: 1000
```

Or via environment override for development:

```bash
export AMPLIFIER_MODULE_TOOL_MEMORY=/path/to/local/module
```
