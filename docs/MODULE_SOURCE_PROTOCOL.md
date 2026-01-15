# Module Source Protocol

_Version: 1.0.0_
_Layer: Kernel Mechanism_
_Status: Specification_

---

## Purpose

The kernel provides a mechanism for custom module source resolution. The loader accepts an optional `ModuleSourceResolver` via mount point injection. If no resolver is provided, the kernel falls back to standard Python entry point discovery.

**How modules are discovered and from where is app-layer policy.**

---

## Kernel Contracts

### ModuleSource Protocol

```python
class ModuleSource(Protocol):
    """Contract for module sources.

    Implementations must resolve to a filesystem path where a Python module
    can be imported.
    """

    def resolve(self) -> Path:
        """
        Resolve source to filesystem path.

        Returns:
            Path: Directory containing importable Python module

        Raises:
            ModuleNotFoundError: Source cannot be resolved
            OSError: Filesystem access error
        """
```

**Examples of conforming implementations (app-layer):**

- FileSource: Resolves local filesystem paths
- GitSource: Clones git repos, caches, returns cache path
- PackageSource: Finds installed Python packages

**Kernel does NOT define these implementations.** They are app-layer policy.

### ModuleSourceResolver Protocol

```python
class ModuleSourceResolver(Protocol):
    """Contract for module source resolution strategies.

    Implementations decide WHERE to find modules based on module ID and
    optional source hints.
    """

    def resolve(self, module_id: str, source_hint: Any = None) -> ModuleSource:
        """
        Resolve module ID to a source.

        Args:
            module_id: Module identifier (e.g., "tool-bash")
            source_hint: Optional source hint from bundle configuration
                         (format defined by app layer)

        Returns:
            ModuleSource that can be resolved to a path

        Raises:
            ModuleNotFoundError: Module cannot be found by this resolver
        """
```

**The resolver is app-layer policy.** Different apps may use different resolution strategies:

- Development app: Check workspace, then configs, then packages
- Production app: Only use verified packages
- Testing app: Use mock implementations

**Kernel does NOT define resolution strategy.** It only provides the injection mechanism.

---

## Loader Injection Contract

### Module Loader API

```python
class ModuleLoader:
    """Kernel mechanism for loading modules.

    Accepts optional ModuleSourceResolver via coordinator mount point.
    Falls back to direct entry-point discovery if no resolver provided.
    """

    def __init__(self, coordinator):
        """Initialize loader with coordinator."""
        self.coordinator = coordinator

    async def load(self, module_id: str, config: dict = None, source_hint = None):
        """
        Load module using resolver or fallback to direct discovery.

        Args:
            module_id: Module identifier
            config: Optional module configuration
            source_hint: Optional source hint from bundle/config

        Raises:
            ModuleNotFoundError: Module not found
            ModuleLoadError: Module found but failed to load
        """
        # Try to get resolver from mount point
        source_resolver = None
        if self.coordinator:
            try:
                source_resolver = self.coordinator.get("module-source-resolver")
            except ValueError:
                pass  # No resolver mounted

        if source_resolver is None:
            # No resolver - use direct entry-point discovery
            return await self._load_direct(module_id, config)

        # Use resolver
        source = source_resolver.resolve(module_id, source_hint)
        module_path = source.resolve()

        # Load from resolved path
        # ... import and mount logic ...
```

### Mounting a Custom Resolver (App-Layer)

```python
# App layer creates resolver (policy)
resolver = CustomModuleSourceResolver()

# Mount it before creating loader
coordinator.mount("module-source-resolver", resolver)

# Loader will use custom resolver
loader = AmplifierModuleLoader(coordinator)
```

**Kernel provides the mount point and fallback. App layer provides the resolver.**

---

## Kernel Responsibilities

**The kernel:**

- ✅ Defines ModuleSource and ModuleSourceResolver protocols
- ✅ Accepts resolver via "module-source-resolver" mount point
- ✅ Falls back to entry point discovery if no resolver
- ✅ Loads module from resolved path
- ✅ Handles module import and mounting

**The kernel does NOT:**

- ❌ Define specific resolution strategies (6-layer, configs, etc.)
- ❌ Parse configuration files (YAML, TOML, JSON, etc.)
- ❌ Know about workspace conventions, git caching, or URIs
- ❌ Provide CLI commands for source management
- ❌ Define bundle config schemas or source field formats

---

## Error Contracts

### ModuleNotFoundError

```python
class ModuleNotFoundError(Exception):
    """Raised when a module cannot be found.

    Resolvers MUST raise this when all resolution attempts fail.
    Loaders MUST propagate this to callers.

    Message SHOULD be helpful, indicating:
    - What module was requested
    - What resolution attempts were made (if applicable)
    - Suggestions for resolution (if applicable)
    """
```

### ModuleLoadError

```python
class ModuleLoadError(Exception):
    """Raised when a module is found but cannot be loaded.

    Examples:
    - Module path exists but isn't valid Python
    - Import fails due to missing dependencies
    - Module doesn't implement required protocol
    """
```

---

## Fallback Behavior

### Direct Entry Point Discovery (Kernel Default)

When no ModuleSourceResolver is mounted, the kernel falls back to direct entry point discovery via the `_load_direct()` method:

1. Searches Python entry points (group="amplifier.modules")
2. Falls back to filesystem discovery (if search paths configured)
3. Uses standard Python import mechanisms

**Implementation**: The `_load_direct()` method directly calls `_load_entry_point()` and `_load_filesystem()` without creating a resolver wrapper object.

**This ensures the kernel works without any app-layer resolver.**

---

## Example: Custom Resolver (App-Layer)

**Not in kernel, but shown for clarity:**

```python
# App layer defines custom resolution strategy
class MyCustomResolver:
    """Example custom resolver (app-layer policy)."""

    def resolve(self, module_id: str, source_hint: Any = None) -> ModuleSource:
        # App-specific logic
        if module_id in self.overrides:
            return FileSource(self.overrides[module_id])

        # Fall back to source hint
        if source_hint:
            return self.parse_source_hint(source_hint)

        # Fall back to some default
        return PackageSource(f"myapp-module-{module_id}")
```

This is **policy, not kernel.** Different apps can implement different strategies.

---

## Kernel Invariants

When implementing custom resolvers:

1. **Must return ModuleSource**: Conforming to protocol
2. **Must raise ModuleNotFoundError**: On failure
3. **Must not interfere with kernel**: No side effects beyond resolution
4. **Must be deterministic**: Same inputs → same output

---

## Related Documentation

**Kernel specifications:**

- [SESSION_FORK_SPECIFICATION.md](./SESSION_FORK_SPECIFICATION.md) - Session forking contracts
- [COORDINATOR_INFRASTRUCTURE_CONTEXT.md](./COORDINATOR_INFRASTRUCTURE_CONTEXT.md) - Mount point system

**Related Specifications:**

- [DESIGN_PHILOSOPHY.md](./DESIGN_PHILOSOPHY.md) - Kernel design principles
- [MOUNT_PLAN_SPECIFICATION.md](./specs/MOUNT_PLAN_SPECIFICATION.md) - Mount plan format

**Note**: Module source resolution implementation is application-layer policy. Applications may implement custom resolution strategies using the protocols defined above.
