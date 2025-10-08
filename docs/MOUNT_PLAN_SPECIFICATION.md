# Mount Plan Specification

The **Mount Plan** is the contract between the application layer (CLI) and the Amplifier kernel (amplifier-core). It defines exactly what modules should be loaded and how they should be configured.

## Purpose

The Mount Plan serves as the "resolved configuration" that the kernel understands. The app layer is responsible for:
- Reading various config sources (profiles, user config, project config, CLI flags, env vars)
- Merging them with proper precedence
- Producing a single, complete Mount Plan dictionary

The kernel is responsible for:
- Validating the Mount Plan
- Loading the specified modules
- Mounting them at the correct mount points
- Managing their lifecycle

## Schema

The Mount Plan is a Python dictionary with the following structure:

```python
{
    "session": {
        "orchestrator": str,  # Required: orchestrator module ID
        "context": str        # Required: context manager module ID
    },
    "context": {
        "config": dict        # Optional: context-specific configuration
    },
    "providers": [            # Optional: list of provider configurations
        {
            "module": str,     # Required: provider module ID
            "config": dict     # Optional: provider-specific config
        }
    ],
    "tools": [                # Optional: list of tool configurations
        {
            "module": str,     # Required: tool module ID
            "config": dict     # Optional: tool-specific config
        }
    ],
    "agents": [               # Optional: list of agent configurations
        {
            "module": str,     # Required: agent module ID
            "config": dict     # Optional: agent-specific config
        }
    ],
    "hooks": [                # Optional: list of hook configurations
        {
            "module": str,     # Required: hook module ID
            "config": dict     # Optional: hook-specific config
        }
    ]
}
```

## Module IDs

Module IDs are strings that identify which module to load. The ModuleLoader will:
1. First try to load via Python entry points (group: `amplifier.modules`)
2. Then try filesystem discovery (directories matching `amplifier-module-<module-id>`)

Common module ID formats:
- Orchestrators: `loop-basic`, `loop-streaming`, `loop-events`
- Context managers: `context-simple`, `context-persistent`
- Providers: `provider-mock`, `provider-anthropic`, `provider-openai`
- Tools: `tool-filesystem`, `tool-bash`, `tool-web`, `tool-search`, `tool-task`
- Hooks: `hooks-logging`, `hooks-backup`, `hooks-scheduler-heuristic`
- Agents: `agent-architect`

## Configuration Dictionaries

Each module can have an optional `config` dictionary. The structure of this dictionary is module-specific and defined by each module's documentation.

### Common Patterns

**Environment Variables**: Config values can reference environment variables using `${VAR_NAME}` syntax:
```python
{
    "module": "provider-anthropic",
    "config": {
        "api_key": "${ANTHROPIC_API_KEY}",
        "model": "claude-sonnet-4-5"
    }
}
```

**Context Config**: The context manager gets its config from a top-level `context.config` key:
```python
{
    "context": {
        "config": {
            "max_tokens": 200000,
            "compact_threshold": 0.92,
            "auto_compact": True
        }
    }
}
```

## Examples

### Minimal Mount Plan

The absolute minimum Mount Plan that will work:

```python
{
    "session": {
        "orchestrator": "loop-basic",
        "context": "context-simple"
    },
    "providers": [
        {"module": "provider-mock"}
    ]
}
```

This creates a basic agent session with:
- Simple orchestrator loop
- In-memory context (no persistence)
- Mock provider (for testing)

### Development Mount Plan

A typical development configuration:

```python
{
    "session": {
        "orchestrator": "loop-streaming",
        "context": "context-persistent"
    },
    "context": {
        "config": {
            "max_tokens": 200000,
            "compact_threshold": 0.92
        }
    },
    "providers": [
        {
            "module": "provider-anthropic",
            "config": {
                "model": "claude-sonnet-4-5",
                "api_key": "${ANTHROPIC_API_KEY}"
            }
        }
    ],
    "tools": [
        {
            "module": "tool-filesystem",
            "config": {
                "allowed_paths": ["."],
                "require_approval": False
            }
        },
        {"module": "tool-bash"},
        {"module": "tool-web"}
    ],
    "hooks": [
        {
            "module": "hooks-logging",
            "config": {
                "output_dir": ".amplifier/logs"
            }
        },
        {"module": "hooks-backup"}
    ]
}
```

### Production Mount Plan

A production configuration with cost controls and safety:

```python
{
    "session": {
        "orchestrator": "loop-events",
        "context": "context-persistent"
    },
    "context": {
        "config": {
            "max_tokens": 200000,
            "compact_threshold": 0.95,
            "auto_compact": True
        }
    },
    "providers": [
        {
            "module": "provider-anthropic",
            "config": {
                "model": "claude-sonnet-4",
                "api_key": "${ANTHROPIC_API_KEY}",
                "max_tokens": 4096
            }
        }
    ],
    "tools": [
        {
            "module": "tool-filesystem",
            "config": {
                "allowed_paths": ["/app/data"],
                "require_approval": True
            }
        }
    ],
    "hooks": [
        {
            "module": "hooks-scheduler-cost-aware",
            "config": {
                "budget_limit": 10.0,
                "alert_threshold": 8.0
            }
        },
        {"module": "hooks-logging"},
        {"module": "hooks-backup"}
    ]
}
```

## Validation

`AmplifierSession` validates the Mount Plan on initialization:

### Required Fields
- `session.orchestrator` must be present and loadable
- `session.context` must be present and loadable
- At least one provider must be configured (required for agent loops)

### Module Loading
- All specified module IDs must be discoverable
- Module loading failures are logged but non-fatal (except orchestrator and context)
- Invalid config for a module causes that module to fail loading

### Error Handling
- Missing required fields: `ValueError` raised immediately
- Module not found: Logged as warning, session continues
- Invalid module config: Logged as warning, module skipped

## Creating Mount Plans

Application code should never manually construct Mount Plans. Instead:
1. Use the Profile system to define configurations
2. Let the CLI's `resolve_app_config()` merge all sources
3. Pass the resulting dictionary to `AmplifierSession`

Example usage:

```python
from amplifier_core import AmplifierSession, ModuleLoader

# Mount Plan from app layer
mount_plan = {
    "session": {
        "orchestrator": "loop-basic",
        "context": "context-simple"
    },
    "providers": [
        {"module": "provider-mock"}
    ]
}

# Create session with Mount Plan
loader = ModuleLoader()
session = AmplifierSession(mount_plan, loader=loader)

# Initialize and execute
await session.initialize()
response = await session.execute("Hello, world!")
await session.cleanup()
```

## Philosophy

The Mount Plan embodies the kernel philosophy:

- **Mechanism, not policy**: The Mount Plan is pure mechanism - it says *what* to load, not *why*
- **Policy at edges**: All decisions about *which* modules to use live in the app layer
- **Stable contract**: The Mount Plan schema is the stable boundary between app and kernel
- **Text-first**: Mount Plans are simple dictionaries, easily serializable and inspectable
- **Deterministic**: Same Mount Plan always produces same module configuration

## Related Documentation

- Profile System Design: `/docs/PROFILES.md` - How to create reusable configurations
- Kernel Philosophy: `/docs/KERNEL_PHILOSOPHY.md` - Why the kernel is designed this way
- Module Development: `/docs/MODULE_DEVELOPMENT.md` - How to create new modules
