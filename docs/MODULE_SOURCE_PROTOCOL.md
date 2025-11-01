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
    optional profile hints.
    """

    def resolve(self, module_id: str, profile_hint: Any = None) -> ModuleSource:
        """
        Resolve module ID to a source.

        Args:
            module_id: Module identifier (e.g., "tool-bash")
            profile_hint: Optional hint from profile configuration
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
class AmplifierModuleLoader:
    """Kernel mechanism for loading modules.

    Accepts optional ModuleSourceResolver via coordinator mount point.
    Falls back to entry point discovery if no resolver provided.
    """

    def __init__(self, coordinator):
        """Initialize loader with coordinator."""
        self.coordinator = coordinator

        # Get resolver from mount point (if app provided one)
        self.resolver = coordinator.get("module-source-resolver")

        # Fallback: If no resolver, use entry points (kernel mechanism)
        if not self.resolver:
            self.resolver = EntryPointResolver()  # Kernel-provided default

    def load_module(self, module_id: str, profile_hint: Any = None):
        """
        Load module using resolver.

        Args:
            module_id: Module identifier
            profile_hint: Optional hint passed to resolver (app-defined)

        Raises:
            ModuleNotFoundError: Module not found by resolver
            ModuleLoadError: Module found but failed to load
        """
        # Resolve source (policy decides WHERE)
        source = self.resolver.resolve(module_id, profile_hint)

        # Get module path (source decides HOW to get path)
        module_path = source.resolve()

        # Load module (kernel mechanism for mounting)
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
- ❌ Define profile schemas or source field formats

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

### EntryPointResolver (Kernel Default)

The kernel provides a minimal default resolver using Python entry points:

```python
class EntryPointResolver:
    """Kernel-provided default resolver using entry points."""

    def resolve(self, module_id: str, profile_hint: Any = None) -> ModuleSource:
        """Resolve using Python entry points."""
        # Look up entry point for module_id
        entry_point = self._find_entry_point(module_id)

        if not entry_point:
            raise ModuleNotFoundError(f"Module '{module_id}' not found in entry points")

        # Return package source
        return PackageSource(entry_point.dist.name)
```

**This ensures the kernel works without any app-layer resolver.**

---

## Example: Custom Resolver (App-Layer)

**Not in kernel, but shown for clarity:**

```python
# App layer defines custom resolution strategy
class MyCustomResolver:
    """Example custom resolver (app-layer policy)."""

    def resolve(self, module_id: str, profile_hint: Any = None) -> ModuleSource:
        # App-specific logic
        if module_id in self.overrides:
            return FileSource(self.overrides[module_id])

        # Fall back to profile hint
        if profile_hint:
            return self.parse_profile_hint(profile_hint)

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

**Note**: Module source resolution implementation is application-layer policy. Applications may use libraries like amplifier-module-resolution or implement custom resolution strategies.
