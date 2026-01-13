---
spec_type: kernel_contract
last_modified: 2025-01-29
related_contracts:
  - path: ../contracts/PROVIDER_CONTRACT.md
    relationship: referenced_by
  - path: ../contracts/TOOL_CONTRACT.md
    relationship: referenced_by
  - path: ../contracts/HOOK_CONTRACT.md
    relationship: referenced_by
  - path: ../contracts/ORCHESTRATOR_CONTRACT.md
    relationship: referenced_by
  - path: ../contracts/CONTEXT_CONTRACT.md
    relationship: referenced_by
---

# Mount Plan Specification

The **Mount Plan** is the contract between the application layer (CLI) and the Amplifier kernel (amplifier-core). It defines exactly what modules should be loaded and how they should be configured.

## Purpose

The Mount Plan serves as the "resolved configuration" that the kernel understands. The app layer is responsible for:

- Reading various config sources (bundles, user config, project config, CLI flags, env vars)
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
        "orchestrator": str,           # Required: orchestrator module ID
        "orchestrator_source": str,    # Optional: orchestrator source URI
        "context": str,                # Required: context manager module ID
        "context_source": str,         # Optional: context source URI
        "injection_budget_per_turn": int | None,  # Optional: max tokens hooks can inject per turn (default: 10000, None for unlimited)
        "injection_size_limit": int | None        # Optional: max bytes per hook injection (default: 10240, None for unlimited)
    },
    "orchestrator": {
        "config": dict        # Optional: orchestrator-specific configuration
    },
    "context": {
        "config": dict        # Optional: context-specific configuration
    },
    "providers": [            # Optional: list of provider configurations
        {
            "module": str,     # Required: provider module ID
            "source": str,     # Optional: source URI (git, file, package)
            "config": dict     # Optional: provider-specific config
        }
    ],
    "tools": [                # Optional: list of tool configurations
        {
            "module": str,     # Required: tool module ID
            "source": str,     # Optional: source URI (git, file, package)
            "config": dict     # Optional: tool-specific config
        }
    ],
    "agents": {               # Optional: agent configuration overlays (app-layer data)
        "<agent-name>": {
            "description": str,         # Agent description (for task tool display)
            "session": dict,            # Optional: override orchestrator/context
            "providers": list,          # Optional: override providers
            "tools": list,              # Optional: override tools
            "hooks": list,              # Optional: override hooks
            "system": {"instruction": str}  # System instruction for agent persona
        }
    },
    "hooks": [                # Optional: list of hook configurations
        {
            "module": str,     # Required: hook module ID
            "source": str,     # Optional: source URI (git, file, package)
            "config": dict     # Optional: hook-specific config
        }
    ]
}
```

## Module Sources (Added in v2)

All module references now support an optional `source` field specifying where to load the module from:

**Source URI formats:**

- Git: `git+https://github.com/org/repo@ref`
- File: `file:///absolute/path` or `/absolute/path` or `./relative/path`
- Package: `package-name` (or omit source to use installed package)

**Resolution:** If `source` is provided, the ModuleSourceResolver resolves it. If omitted, falls back to installed packages via entry points.

See [MODULE_SOURCE_SPECIFICATION.md](../../MODULE_SOURCE_SPECIFICATION.md) for complete details.

````

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

### agents Section (Special Semantics)

The `agents` section has different semantics from other mount plan sections:

**Other sections** (`providers`, `tools`, `hooks`):
- Lists of modules to load and mount NOW during session initialization
- Kernel loads and mounts these modules

**agents section**:
- Dict of named configuration overlays (app-layer data for future use)
- NOT modules to mount during initialization
- Used by app layer (task tool) for spawning child sessions
- Kernel passes through without interpretation

Agent configurations are partial mount plans that get merged with a parent session's config when creating a child session with `parent_id` parameter.

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
````

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
        "context": "context-persistent",
        "injection_budget_per_turn": 500,  # Conservative limit for production
        "injection_size_limit": 8192       # Cap each injection to 8 KB
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
                "model": "claude-sonnet-4-5",
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

Validation happens in two phases: structural validation (before loading) and runtime validation (during initialization).

### Pre-Load Structural Validation

Use `MountPlanValidator` to validate mount plan structure before attempting to load modules:

```python
from amplifier_core.validation import MountPlanValidator

validator = MountPlanValidator()
result = validator.validate(mount_plan)

if not result.passed:
    print(result.format_errors())
    sys.exit(1)

# Safe to proceed with session creation
session = AmplifierSession(mount_plan)
```

`MountPlanValidator` checks:
- Root structure is a dict with required `session` section
- Session section has required `orchestrator` and `context` fields
- Module specs have required `module` field
- Config and source fields are correct types when present
- Unknown sections generate warnings (not errors)

### Runtime Validation

`AmplifierSession` performs additional validation on initialization:

- `session.orchestrator` must be loadable
- `session.context` must be loadable
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

1. Use bundles to define configurations
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

- **Mechanism, not policy**: The Mount Plan is pure mechanism - it says _what_ to load, not _why_
- **Policy at edges**: All decisions about _which_ modules to use live in the app layer
- **Stable contract**: The Mount Plan schema is the stable boundary between app and kernel
- **Text-first**: Mount Plans are simple dictionaries, easily serializable and inspectable
- **Deterministic**: Same Mount Plan always produces same module configuration

## Related Documentation

- **[DESIGN_PHILOSOPHY.md](../DESIGN_PHILOSOPHY.md)** - Kernel design principles
- **[Getting Started](https://github.com/microsoft/amplifier)** - User documentation

**Note**: Applications compile Mount Plans from various sources (bundles, configuration files, CLI flags, etc.). The kernel validates the compiled plan structure.
