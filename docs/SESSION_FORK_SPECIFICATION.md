# Session Fork Specification

_Version: 1.0.0_
_Layer: Kernel Mechanism_
_Status: Specification_

---

## Purpose

Session forking enables creating child sessions linked to a parent session. The kernel provides the mechanism for forking and lineage tracking. How forked sessions are configured and used is app-layer policy.

---

## Kernel API

### parent_id Parameter

```python
class AmplifierSession:
    def __init__(
        self,
        config: dict,
        loader: ModuleLoader | None = None,
        session_id: str | None = None,
        parent_id: str | None = None  # NEW: Links child to parent
    ):
        """
        Initialize an Amplifier session.

        Args:
            config: Required mount plan configuration
            loader: Optional module loader
            session_id: Optional session ID (generates UUID if None)
            parent_id: Optional parent session ID (None for top-level, UUID for child)

        When parent_id is set:
            - Session is a child (forked from parent)
            - Emits session:fork event during initialize()
            - All events include parent_id for lineage tracking
        """
        self.session_id = session_id or str(uuid.uuid4())
        self.parent_id = parent_id
        # ...
```

**Creating child session**:
```python
# App layer creates child with parent_id
child = AmplifierSession(
    config=child_config,  # Merged from parent + agent overlay
    loader=parent.loader,
    session_id=f"{parent.session_id}-{suffix}",
    parent_id=parent.session_id  # Links to parent
)
```

**Kernel responsibilities**:
- Accept parent_id parameter
- Track parent_id in session
- Emit session:fork event when parent_id is set
- Include parent_id in all child events

**Kernel does NOT**:
- Provide child's configuration (app layer merges configs)
- Create child session directly (app layer calls constructor)
- Manage session hierarchy (app layer tracks relationships)

---

## Session Lineage

### Parent-Child Relationship

```python
# Parent session
parent = AmplifierSession(config=parent_config, session_id="abc-123")

# Create child session with parent_id
child = AmplifierSession(
    config=child_config,
    session_id="abc-123-child-1",
    parent_id="abc-123"  # Links to parent
)

# Child knows its parent
assert child.session_id == "abc-123-child-1"
assert child.parent_id == "abc-123"  # Linked to parent
```

### ID Convention (Recommended, Not Enforced)

**Hierarchical naming**:
```
parent: abc-123
child:  abc-123-{suffix}

Examples:
  abc-123-fork-1
  abc-123-fork-2
  abc-123-specialist-a1b2
```

**Benefits**:
- Visual hierarchy
- Easy parent lookup (parse ID)
- Supports nested forking

**Note**: Kernel doesn't enforce this. App layer can use any ID scheme.

---

## Event Schema

### session:fork

**Event name**: `session:fork`

**Required fields** (from greenfield spec):
```json
{
  "event": "session:fork",
  "session_id": "abc-123-child-1",  // Child's ID
  "parent_id": "abc-123",            // Parent's ID (explicit in fork events)
  "data": {
    "parent": "abc-123"              // Also in data for compatibility
  },
  "ts": "2025-10-14T12:00:00Z"
}
```

**Emitted**: When `session.fork()` is called

**Purpose**: Track session lineage for debugging, auditing, analysis

### All Child Events Include parent_id

**Every event emitted by child session**:
```json
{
  "event": "provider:request",
  "session_id": "abc-123-child-1",
  "parent_id": "abc-123",  // Child's parent
  // ...
}
```

**This enables**:
- Filtering events by parent (show all child sessions)
- Tracing delegation trees
- Understanding multi-session workflows

---

## Forked Session Lifecycle

### Creation

```python
# 1. Create child config (app-layer policy)
child_config = merge_configs(parent.config, agent_overlay)

# 2. Create child session with parent_id (kernel mechanism)
child = AmplifierSession(
    config=child_config,
    loader=parent.loader,
    session_id=f"{parent.session_id}-specialized",
    parent_id=parent.session_id  # Kernel tracks lineage
)

# 3. Initialize (standard session init)
await child.initialize()  # Mounts modules per config, emits session:fork

# 4. Use child session
response = await child.execute("Some task")
```

### Cleanup

```python
# Child session cleanup
await child.cleanup()

# Optionally cascade to parent cleanup
await parent.cleanup(cascade_children=True)
```

---

## Nested Forking

**Child sessions can have children**:

```python
# Parent
parent = AmplifierSession(config=config, session_id="abc-123")

# Child
child = AmplifierSession(config=config, session_id="abc-123-level1", parent_id="abc-123")

# Grandchild
grandchild = AmplifierSession(config=config, session_id="abc-123-level1-level2", parent_id="abc-123-level1")

# Lineage
assert grandchild.parent_id == "abc-123-level1"
assert child.parent_id == "abc-123"
```

**Depth limits**: Not enforced by kernel. App layer implements recursion depth limits if desired.

---

## What Kernel Does NOT Provide

### Configuration Policy

Kernel does not:
- Define what configuration forked sessions should have
- Merge parent and child configs
- Enforce inheritance rules
- Validate configuration overlays

**App layer responsibility**: Decide child session configuration.

### Multi-Turn Management

Kernel does not:
- Track which forked sessions are active
- Manage resumption of forked sessions
- Implement multi-turn delegation patterns

**App layer responsibility**: Session persistence and resumption.

### Discovery

Kernel does not:
- Discover available agents
- Advertise agent capabilities
- Maintain agent registry

**App layer responsibility**: Agent discovery and advertisement.

---

## Kernel Guarantees

1. **Lineage tracking**: All child events include parent_id
2. **Unique IDs**: Generated session_ids are unique
3. **Event emission**: session:fork always emitted
4. **Independence**: Parent and child sessions are independent after fork
5. **Determinism**: Forking is deterministic given same parent_id and child_id

---

## Non-Goals (From Kernel Perspective)

The kernel explicitly does NOT:
- Interpret agent configurations (app layer policy)
- Automatically discover agents (app layer feature)
- Enforce agent-specific restrictions (app layer policy)
- Manage agent lifecycles beyond session forking (app layer)
- Provide default configurations for forks (app layer policy)
- Implement delegation patterns (app layer feature)

---

## Example Usage

### Simple Child Session

```python
# Create parent
parent = AmplifierSession(config={
    "session": {"orchestrator": "loop-basic", "context": "context-simple"},
    "providers": [{"module": "provider-anthropic"}]
})
await parent.initialize()

# Create child with parent_id
child = AmplifierSession(
    config={
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [{"module": "provider-anthropic", "config": {"default_model": "claude-sonnet-4-5"}}]
    },
    loader=parent.loader,
    session_id="parent-123-child",
    parent_id="parent-123"  # Links to parent
)

# Initialize and use
await child.initialize()  # Emits session:fork automatically
response = await child.execute("Child's task")
```

### Child with Config Inheritance (App Layer Pattern)

```python
# App layer merges configs (policy)
child_config = merge_configs(parent.config, agent_overlay)

# Create child with merged config (mechanism)
child = AmplifierSession(
    config=child_config,
    loader=parent.loader,
    parent_id=parent.session_id  # Kernel mechanism
)

# Standard init
await child.initialize()
```

**Config merging is app-layer logic**, not kernel mechanism.

---

## Summary

**Kernel provides**:
- `session.fork()` method
- Parent-child ID tracking
- `session:fork` event emission
- `parent_id` in all child events

**Kernel does NOT provide**:
- Configuration for forked sessions
- Config merging logic
- Agent discovery
- Multi-turn management
- Delegation patterns

**Philosophy**: Pure mechanism. Policy at edges.

---

_This specification defines the kernel-level session forking mechanism without any app-layer policy concerns._
