# ModuleCoordinator - Infrastructure Context Specification

_Version: 1.0.0_
_Layer: Kernel Mechanism_
_Status: Authoritative Specification_

---

## Purpose

The ModuleCoordinator provides **infrastructure context** to all modules. It is the single source of truth for session identity, configuration, and execution context that modules need to function.

**Core Principle**: Coordinator is not just a module registry - it's the execution context that provides infrastructure (IDs, config, references) to modules.

---

## Infrastructure Provided

### Identity Infrastructure

**Session identity**:
```python
coordinator.session_id: str       # Current session ID
coordinator.parent_id: str | None # Parent session ID (for children)
```

**Future identity** (when implemented):
```python
coordinator.turn_id: str | None   # Current turn/request ID
coordinator.span_id: str | None   # Current operation span ID
```

**Use cases**:
- Context manager: Persist to `sessions/{session_id}/`
- Backup hook: Organize backups by session_id
- Cost tracker: Track costs per session_id
- Event correlation: All events include session_id

### Configuration Infrastructure

**Mount plan access**:
```python
coordinator.config: dict  # Session's mount plan (includes agents, etc.)
```

**Use cases**:
- Tools: Read agent configurations
- Context: Get storage paths, token limits
- Hooks: Read logging paths, retention policies
- Providers: Access API keys, model settings

### Session Infrastructure

**Session reference**:
```python
coordinator.session: AmplifierSession  # Parent session for spawning children
coordinator.loader: ModuleLoader       # Shared module loader
```

**Use cases**:
- Task tool: Spawn child sessions
- Dynamic loading: Load modules on demand
- Session hierarchy: Access parent properties

---

## Kernel Philosophy Alignment

### "Minimal Context Plumbing" (KERNEL_PHILOSOPHY.md)

> "Passing identifiers and basic state necessary to make boundaries work"

**Coordinator IS that minimal context plumbing.**

**What it provides**:
- Identifiers: session_id, parent_id, (future: turn_id, span_id)
- Basic state: config, session reference, loader

**What it does NOT provide**:
- Business logic
- Policy decisions
- Module implementations

**Perfect boundary** - infrastructure only.

### "Causality IDs" (KERNEL_PHILOSOPHY.md)

> "Provide session/request/span identifiers so edges can correlate activity end-to-end"

**Coordinator provides these IDs** via:
- Direct properties (coordinator.session_id)
- Event default fields (all events include session_id, parent_id)

**Enables**:
- Event correlation across modules
- Session lineage tracking
- Debugging and observability

### "Capability-Scoped" (KERNEL_PHILOSOPHY.md)

> "Pass only the minimum capability a module needs"

**Coordinator scopes access**:
- Modules get coordinator (not full kernel)
- Coordinator provides only infrastructure (not full session API)
- Clean, minimal interface

---

## API

### Properties (Infrastructure Access)

```python
class ModuleCoordinator:
    """Execution context providing infrastructure to modules."""

    @property
    def session_id(self) -> str:
        """Current session ID (infrastructure for persistence/correlation)."""
        return self.session.session_id

    @property
    def parent_id(self) -> str | None:
        """Parent session ID for child sessions (infrastructure for lineage)."""
        return self.session.parent_id

    @property
    def config(self) -> dict:
        """Session configuration/mount plan (infrastructure for module config)."""
        return self.session.config

    @property
    def loader(self) -> ModuleLoader:
        """Module loader (infrastructure for dynamic loading)."""
        return self.session.loader

    @property
    def session(self) -> AmplifierSession:
        """Parent session reference (infrastructure for spawning children)."""
        return self._session
```

### Existing Methods (Module Management)

```python
    async def mount(self, mount_point: str, module, name: str | None = None):
        """Mount a module at a mount point."""

    def get(self, mount_point: str, name: str | None = None):
        """Get mounted module(s)."""

    def register_capability(self, name: str, handler):
        """Register a capability for cross-module communication."""

    def get_capability(self, name: str):
        """Get a registered capability."""
```

---

## Module Usage Pattern

### Standard Module Pattern

**All modules receive coordinator**:

```python
async def mount(coordinator: ModuleCoordinator, config: dict):
    """Mount function - receives coordinator with infrastructure."""
    module = MyModule(coordinator, config)
    await coordinator.mount("tools", module, name="my-tool")
```

**Module accesses infrastructure**:

```python
class MyModule:
    def __init__(self, coordinator: ModuleCoordinator, config: dict):
        self.coordinator = coordinator  # Infrastructure context
        self.config = config  # Module-specific config

    async def do_work(self):
        # Access infrastructure
        session_id = self.coordinator.session_id  # For persistence
        parent_id = self.coordinator.parent_id    # For lineage
        mount_plan = self.coordinator.config      # For reading agents, etc.
        loader = self.coordinator.loader          # For dynamic loading
```

---

## Common Use Cases

### Context Manager (Persistence)

```python
class SimpleContextManager:
    def __init__(self, coordinator):
        self.coordinator = coordinator

    async def add_message(self, message):
        # Use session_id for persistence location
        session_id = self.coordinator.session_id
        storage_path = Path("~/.amplifier/sessions") / session_id
        storage_path.mkdir(parents=True, exist_ok=True)

        with open(storage_path / "messages.jsonl", "a") as f:
            f.write(json.dumps(message) + "\n")
```

### Hook (Logging)

```python
class LoggingHook:
    def __init__(self, coordinator):
        self.coordinator = coordinator

    async def on_event(self, event, data):
        # session_id already in data (from coordinator.hooks.set_default_fields)
        # But can also access directly
        session_id = self.coordinator.session_id

        log_entry = {
            "session_id": session_id,
            "event": event,
            "data": data
        }
        # Write to session-specific log
```

### Tool (Child Session Spawning)

```python
class TaskTool:
    def __init__(self, coordinator, config):
        self.coordinator = coordinator

    async def execute(self, input):
        # Access parent session for spawning
        parent_session = self.coordinator.session
        agents = self.coordinator.config.get("agents", {})

        # Merge configs
        merged = merge_configs(parent_session.config, agents[agent_name])

        # Spawn child
        child = AmplifierSession(
            config=merged,
            loader=self.coordinator.loader,
            parent_id=parent_session.session_id
        )
```

### Provider (API Correlation)

```python
class AnthropicProvider:
    def __init__(self, coordinator, config):
        self.coordinator = coordinator
        self.api_key = config.get("api_key")

    async def complete(self, messages, **kwargs):
        # Include session_id in API metadata for tracking
        session_id = self.coordinator.session_id

        response = await anthropic.messages.create(
            model=self.model,
            messages=messages,
            metadata={"session_id": session_id}  # Correlate API calls to sessions
        )
```

---

## What Coordinator Is NOT

**Not a service locator**:
- Doesn't provide arbitrary services
- Only infrastructure (IDs, config, session, loader)

**Not a god object**:
- Focused responsibility (execution context)
- Doesn't contain business logic

**Not a workaround**:
- This is its designed purpose per kernel philosophy
- "Minimal context plumbing" = coordinator

---

## Initialization Pattern

### Session Creates Coordinator

```python
class AmplifierSession:
    def __init__(self, config, loader=None, session_id=None, parent_id=None):
        self.session_id = session_id or str(uuid.uuid4())
        self.parent_id = parent_id
        self.config = config
        self.loader = loader or ModuleLoader()

        # Create coordinator with infrastructure context
        self.coordinator = ModuleCoordinator(session=self)

        # Set ID defaults for events
        self.coordinator.hooks.set_default_fields(
            session_id=self.session_id,
            parent_id=self.parent_id
        )
```

### Coordinator Stores Infrastructure

```python
class ModuleCoordinator:
    def __init__(self, session: AmplifierSession):
        """Initialize with session providing infrastructure context."""
        self._session = session  # Infrastructure reference
        self.hooks = HookRegistry()
        self.mount_points = {
            "orchestrator": None,
            "context": None,
            "providers": {},
            "tools": {},
            "hooks": self.hooks
        }
        self._capabilities = {}
        self._cleanup_functions = []

    # Provide infrastructure via properties
    @property
    def session(self) -> AmplifierSession:
        """Parent session (infrastructure for spawning)."""
        return self._session

    @property
    def session_id(self) -> str:
        """Current session ID (infrastructure for correlation)."""
        return self._session.session_id

    @property
    def parent_id(self) -> str | None:
        """Parent session ID if child (infrastructure for lineage)."""
        return self._session.parent_id

    @property
    def config(self) -> dict:
        """Session configuration/mount plan (infrastructure for module config)."""
        return self._session.config

    @property
    def loader(self) -> ModuleLoader:
        """Module loader (infrastructure for dynamic loading)."""
        return self._session.loader
```

---

## Philosophy Score

| Tenet | Score | Alignment |
|-------|-------|-----------|
| Mechanism, not policy | 10/10 | ✅ Provides infrastructure (mechanism), modules decide usage (policy) |
| Minimal context plumbing | 10/10 | ✅ Coordinator IS the minimal context plumbing |
| Causality IDs | 10/10 | ✅ Provides all IDs for correlation |
| Capability-scoped | 10/10 | ✅ Modules get coordinator (scoped), not full kernel |
| Separation of concerns | 10/10 | ✅ Infrastructure separate from business logic |
| Explicit boundaries | 10/10 | ✅ Coordinator is explicit boundary |

**Perfect alignment: 10/10**

---

## Summary

**Coordinator's role**:
- Infrastructure context for modules
- Provides: IDs, config, session reference, loader
- Enables: Persistence, correlation, spawning, observability

**Not an afterthought** - this is kernel philosophy realized.

**Universal pattern** - ALL modules needing infrastructure use coordinator.

**Clean, minimal, correct** - 10/10 philosophy compliance.

---

_This specification establishes coordinator as the infrastructure context pattern for all modules, per kernel philosophy._
