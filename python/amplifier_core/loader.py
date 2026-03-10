"""
Module loader for discovering and loading Amplifier modules.
Supports both entry points and filesystem discovery.

With module source resolution:
- Uses ModuleSourceResolver if mounted in coordinator
- Falls back to direct entry-point discovery if no resolver provided
- Supports flexible module sourcing (git, local, packages)
"""

import contextlib
import importlib
import importlib.metadata
import logging
import os
import sys
from collections.abc import Awaitable
from collections.abc import Callable
from pathlib import Path
from typing import Any
from typing import Literal

from .coordinator import ModuleCoordinator
from .models import ModuleInfo

logger = logging.getLogger(__name__)


# Type → Mount Point mapping (kernel mechanism, not policy)
# Modules declare type, kernel derives mount point from this stable mapping
TYPE_TO_MOUNT_POINT = {
    "orchestrator": "orchestrator",
    "provider": "providers",
    "tool": "tools",
    "hook": "hooks",
    "context": "context",
    "resolver": "module-source-resolver",
}


class ModuleValidationError(Exception):
    """Raised when a module fails validation at load time."""

    pass


class ModuleLoader:
    """
    Discovers and loads Amplifier modules.

    Supports source resolution:
    - Uses ModuleSourceResolver from coordinator if available
    - Falls back to direct entry-point discovery if no resolver mounted
    - Backward compatible with existing entry point discovery

    Direct discovery (when no source resolver available):
    1. Python entry points (installed packages)
    2. Environment variables (AMPLIFIER_MODULES)
    3. Filesystem paths
    """

    def __init__(
        self,
        coordinator: ModuleCoordinator | None = None,
        search_paths: list[Path] | None = None,
    ):
        """
        Initialize module loader.

        Args:
            coordinator: Optional coordinator (for resolver injection)
            search_paths: Optional list of filesystem paths for direct discovery
        """
        self._loaded_modules: dict[str, Any] = {}
        self._module_info: dict[str, ModuleInfo] = {}
        self._search_paths = search_paths
        self._coordinator = coordinator
        self._added_paths: list[str] = []  # Track sys.path additions for cleanup

    async def discover(self) -> list[ModuleInfo]:
        """
        Discover all available modules using configured search strategy.

        Returns:
            List of module information
        """
        modules = []

        # Always discover from entry points first
        modules.extend(self._discover_entry_points())

        # Use provided search_paths if available
        if self._search_paths:
            for path in self._search_paths:
                modules.extend(self._discover_filesystem(path))
        # Otherwise fall back to environment variable
        elif env_modules := os.environ.get("AMPLIFIER_MODULES"):
            for path in env_modules.split(":"):
                modules.extend(self._discover_filesystem(Path(path)))

        return modules

    def _discover_entry_points(self) -> list[ModuleInfo]:
        """Discover modules via Python entry points."""
        modules = []

        try:
            # Look for amplifier.modules entry points
            eps = importlib.metadata.entry_points(group="amplifier.modules")

            for ep in eps:
                try:
                    # For entry points, we don't have module_path yet, use naming fallback
                    module_type, mount_point = self._guess_from_naming(ep.name)

                    # Extract module info from entry point metadata
                    module_info = ModuleInfo(
                        id=ep.name,
                        name=ep.name.replace("-", " ").title(),
                        version="1.0.0",  # Would need to get from package metadata
                        type=module_type,  # type: ignore[arg-type]
                        mount_point=mount_point,
                        description=f"Module: {ep.name}",
                    )
                    modules.append(module_info)
                    self._module_info[ep.name] = module_info

                    logger.debug(f"Discovered module '{ep.name}' via entry point")

                except Exception as e:
                    logger.error(f"Error discovering module {ep.name}: {e}")

        except Exception as e:
            logger.warning(f"Could not discover entry points: {e}")

        return modules

    def _discover_filesystem(self, path: Path) -> list[ModuleInfo]:
        """Discover modules from filesystem path."""
        modules = []

        if not path.exists():
            logger.warning(f"Module path does not exist: {path}")
            return modules

        # Look for module directories (amplifier-module-*)
        for item in path.iterdir():
            if item.is_dir() and item.name.startswith("amplifier-module-"):
                try:
                    # Try to load module info
                    module_id = item.name.replace("amplifier-module-", "")

                    # Get metadata (inspect if possible, fallback to naming)
                    module_type, mount_point = self._get_module_metadata(
                        module_id, item
                    )

                    module_info = ModuleInfo(
                        id=module_id,
                        name=module_id.replace("-", " ").title(),
                        version="1.0.0",
                        type=module_type,  # type: ignore[arg-type]
                        mount_point=mount_point,
                        description=f"Module: {module_id}",
                    )
                    modules.append(module_info)
                    self._module_info[module_id] = module_info

                    logger.debug(f"Discovered module '{module_id}' from filesystem")

                except Exception as e:
                    logger.error(f"Error discovering module {item.name}: {e}")

        return modules

    async def load(
        self,
        module_id: str,
        config: dict[str, Any] | None = None,
        source_hint: str | dict | None = None,
        coordinator: ModuleCoordinator | None = None,
    ) -> Callable[[ModuleCoordinator], Awaitable[Callable | None]]:
        """
        Load a specific module using source resolution.

        Args:
            module_id: Module identifier
            config: Optional module configuration
            source_hint: Optional source URI/object from bundle config
            coordinator: Optional coordinator for polyglot dispatch.
                When provided and the resolved module is non-Python,
                dispatch routes to the appropriate polyglot loader
                (WASM or gRPC). When None, all modules load via the
                Python path (backward compatible).

        Returns:
            Mount function for the module

        Raises:
            ValueError: Module not found or failed to load
        """
        if module_id in self._loaded_modules:
            logger.debug(f"Module '{module_id}' already loaded, creating fresh closure")
            raw_fn = self._loaded_modules[module_id]

            async def mount_with_config_cached(
                coordinator: ModuleCoordinator, fn=raw_fn
            ):
                return await fn(coordinator, config or {})

            return mount_with_config_cached

        try:
            # Resolve module source
            try:
                # Get source resolver from coordinator when needed (lazy loading)
                source_resolver = None
                if self._coordinator:
                    # Mount point doesn't exist or nothing mounted - suppress ValueError
                    with contextlib.suppress(ValueError):
                        source_resolver = self._coordinator.get(
                            "module-source-resolver"
                        )

                if source_resolver is None:
                    # No resolver mounted - use direct entry-point discovery
                    logger.debug(
                        f"No source resolver mounted, using direct discovery for '{module_id}'"
                    )
                    mount_closure = await self._load_direct(module_id, config)
                    if mount_closure:
                        return mount_closure
                    raise ValueError(
                        f"Module '{module_id}' not found via entry points or filesystem"
                    )

                # Try async resolution first (supports lazy activation)
                # FIXME: Passing both source_hint and profile_hint for backward compat
                # Remove profile_hint after v2.0 when all downstream repos are updated
                if hasattr(source_resolver, "async_resolve"):
                    source = await source_resolver.async_resolve(
                        module_id, source_hint=source_hint, profile_hint=source_hint
                    )
                else:
                    source = source_resolver.resolve(
                        module_id, source_hint=source_hint, profile_hint=source_hint
                    )
                module_path = source.resolve()
                logger.info(f"[module:mount] {module_id} from {source}")

                # Add module path to sys.path BEFORE validation
                # This makes the module's dependencies (installed by uv pip install --target)
                # available for import during validation
                path_str = str(module_path)
                if path_str not in sys.path:
                    sys.path.insert(0, path_str)
                    self._added_paths.append(path_str)  # Track for cleanup
                    logger.debug(
                        f"Added '{path_str}' to sys.path for module '{module_id}'"
                    )

                # --- Transport dispatch (polyglot) ---
                # Check transport BEFORE validation: non-Python modules
                # (WASM, gRPC) don't have Python packages to validate.
                if coordinator is not None:
                    try:
                        from amplifier_core._engine import resolve_module

                        manifest = resolve_module(str(module_path))
                        transport = manifest.get("transport", "python")

                        if transport == "wasm":
                            return self._make_wasm_mount(module_path, coordinator)

                        if transport == "grpc":
                            return await self._make_grpc_mount(
                                module_path, module_id, config, coordinator
                            )

                        # transport == "python" or unknown → fall through
                    except ImportError:
                        logger.debug(
                            "Rust engine not available, falling through to Python loader"
                        )
                    except Exception as engine_err:
                        logger.warning(
                            f"resolve_module failed for '{module_id}': {engine_err}, "
                            "falling through to Python loader"
                        )

                # Validate module before loading (Python modules only at this point)
                await self._validate_module(module_id, module_path, config=config)

            except Exception as resolve_error:
                # Import here to avoid circular dependency
                from .module_sources import ModuleNotFoundError as SourceNotFoundError

                if isinstance(resolve_error, SourceNotFoundError):
                    # Fall back to direct entry-point discovery
                    logger.debug(
                        f"Source resolution failed for '{module_id}', trying direct discovery"
                    )
                    mount_fn = await self._load_direct(module_id, config)
                    if mount_fn:
                        return mount_fn
                raise resolve_error

            # Try to load via entry point first
            raw_fn = self._load_entry_point(module_id)
            if raw_fn:
                self._loaded_modules[module_id] = raw_fn

                async def mount_with_config_ep(
                    coordinator: ModuleCoordinator, fn=raw_fn
                ):
                    return await fn(coordinator, config or {})

                return mount_with_config_ep

            # Try filesystem loading
            raw_fn = self._load_filesystem(module_id)
            if raw_fn:
                self._loaded_modules[module_id] = raw_fn

                async def mount_with_config_fs(
                    coordinator: ModuleCoordinator, fn=raw_fn
                ):
                    return await fn(coordinator, config or {})

                return mount_with_config_fs

            raise ValueError(
                f"Module '{module_id}' found at {module_path} but failed to load"
            )

        except Exception as e:
            logger.error(f"Failed to load module '{module_id}': {e}")
            raise

    async def _load_direct(
        self, module_id: str, config: dict[str, Any] | None = None
    ) -> Callable | None:
        """Direct loading via entry points and filesystem discovery.

        Used when no source resolver is available (standalone tools, simple cases).
        This is a permanent, first-class mechanism - not deprecated.

        Args:
            module_id: Module identifier
            config: Optional module configuration

        Returns:
            Mount closure (with config bound) if found, None otherwise
        """
        # Try entry point — returns raw mount function (no config bound)
        raw_fn = self._load_entry_point(module_id)
        if raw_fn:
            self._loaded_modules[module_id] = raw_fn  # cache raw fn, not closure

            async def mount_with_config_direct_ep(
                coordinator: ModuleCoordinator, fn=raw_fn
            ):
                return await fn(coordinator, config or {})

            return mount_with_config_direct_ep

        # Try filesystem — returns raw mount function (no config bound)
        raw_fn = self._load_filesystem(module_id)
        if raw_fn:
            self._loaded_modules[module_id] = raw_fn  # cache raw fn, not closure

            async def mount_with_config_direct_fs(
                coordinator: ModuleCoordinator, fn=raw_fn
            ):
                return await fn(coordinator, config or {})

            return mount_with_config_direct_fs

        return None

    def _load_entry_point(self, module_id: str) -> Callable | None:
        """Resolve module entry point and return the raw mount function.

        Returns the raw (un-configured) mount function so callers can cache it
        and wrap it in a fresh closure with the correct config on each use.
        """
        try:
            eps = importlib.metadata.entry_points(group="amplifier.modules")

            for ep in eps:
                if ep.name == module_id:
                    # Load the raw mount function (no config binding here)
                    mount_fn = ep.load()
                    logger.info(f"Loaded module '{module_id}' via entry point")
                    return mount_fn

        except Exception as e:
            logger.error(
                f"Could not load '{module_id}' via entry point: {e}", exc_info=True
            )

        return None

    def _load_filesystem(self, module_id: str) -> Callable | None:
        """Resolve module from filesystem and return the raw mount function.

        Returns the raw (un-configured) mount function so callers can cache it
        and wrap it in a fresh closure with the correct config on each use.
        """
        try:
            # Try to import the module
            module_name = f"amplifier_module_{module_id.replace('-', '_')}"
            module = importlib.import_module(module_name)

            # Get the raw mount function (no config binding here)
            if hasattr(module, "mount"):
                logger.info(f"Loaded module '{module_id}' from filesystem")
                return module.mount

        except Exception as e:
            logger.debug(f"Could not load '{module_id}' from filesystem: {e}")

        return None

    def _get_module_metadata(
        self, module_id: str, module_path: Path
    ) -> tuple[
        Literal["orchestrator", "provider", "tool", "context", "hook", "resolver"], str
    ]:
        """
        Get module type and derive mount point.

        Tries explicit declaration first, falls back to naming convention.

        Args:
            module_id: Module identifier
            module_path: Resolved path to module

        Returns:
            tuple: (module_type, mount_point)
        """
        # Try to import module to read metadata
        try:
            # Find package directory
            package_path = self._find_package_dir(module_id, module_path)
            if package_path:
                # Import the module temporarily
                module_name = f"amplifier_module_{module_id.replace('-', '_')}"

                # Add to sys.path temporarily for import
                path_str = str(module_path)
                added = False
                if path_str not in sys.path:
                    sys.path.insert(0, path_str)
                    added = True

                try:
                    module = importlib.import_module(module_name)

                    # Read ONLY type (simplified!)
                    module_type = getattr(module, "__amplifier_module_type__", None)

                    if module_type:
                        # Derive mount point from type (kernel mechanism)
                        mount_point = TYPE_TO_MOUNT_POINT.get(module_type)
                        if not mount_point:
                            raise ModuleValidationError(
                                f"Module '{module_id}' has unknown type '{module_type}'. "
                                f"Valid types: {list(TYPE_TO_MOUNT_POINT.keys())}"
                            )

                        logger.debug(
                            f"Module '{module_id}' declares type='{module_type}', derived mount_point='{mount_point}'"
                        )
                        return module_type, mount_point

                finally:
                    # Clean up sys.path
                    if added:
                        sys.path.remove(path_str)

        except Exception as e:
            logger.debug(f"Could not inspect module '{module_id}': {e}")

        # Fallback to naming convention (Phase 1-2 only)
        logger.debug(f"Module '{module_id}' has no metadata, using naming convention")
        return self._guess_from_naming(module_id)

    def _guess_from_naming(
        self, module_id: str
    ) -> tuple[
        Literal["orchestrator", "provider", "tool", "context", "hook", "resolver"], str
    ]:
        """
        Guess module type and mount point from naming convention.

        FALLBACK ONLY: For modules without explicit metadata.
        Prefer __amplifier_module_type__ attribute (mount point derived).

        Args:
            module_id: Module identifier

        Returns:
            tuple: (module_type, mount_point)
        """
        # Single mapping (consolidates both old methods)
        type_mapping = {
            "orchestrat": ("orchestrator", "orchestrator"),
            "loop": ("orchestrator", "orchestrator"),
            "provider": ("provider", "providers"),
            "tool": ("tool", "tools"),
            "hook": ("hook", "hooks"),
            "context": ("context", "context"),
            # Note: No "agent" - agents are config data, not modules
        }

        module_id_lower = module_id.lower()
        for keyword, (mod_type, mount_pt) in type_mapping.items():
            if keyword in module_id_lower:
                return mod_type, mount_pt  # type: ignore[return-value]

        # Default to tool
        return "tool", "tools"  # type: ignore[return-value]

    async def _validate_module(
        self, module_id: str, module_path: Path, config: dict[str, Any] | None = None
    ) -> None:
        """
        Validate a module before loading.

        Runs the appropriate validator based on module type inferred from module_id.
        Raises ModuleValidationError if validation fails.

        Args:
            module_id: Module identifier (e.g., "provider-anthropic", "tool-filesystem")
            module_path: Resolved filesystem path to the module
            config: Optional module configuration to use during validation

        Raises:
            ModuleValidationError: If module fails validation
        """
        # Import validators here to avoid circular imports at module level
        from .validation import ContextValidator
        from .validation import HookValidator
        from .validation import OrchestratorValidator
        from .validation import ProviderValidator
        from .validation import ToolValidator

        # Get module type (inspect if possible, fallback to naming)
        module_type, _ = self._get_module_metadata(module_id, module_path)

        # Select appropriate validator
        validators = {
            "provider": ProviderValidator,
            "tool": ToolValidator,
            "hook": HookValidator,
            "orchestrator": OrchestratorValidator,
            "context": ContextValidator,
        }

        validator_class = validators.get(module_type)
        if validator_class is None:
            # Unknown module type - skip validation with warning
            logger.warning(
                f"Unknown module type '{module_type}' for '{module_id}', skipping validation"
            )
            return

        # Find the actual Python package directory within the module root
        # Module structure: amplifier-module-xyz/ contains amplifier_module_xyz/
        package_path = self._find_package_dir(module_id, module_path)
        if package_path is None:
            raise ModuleValidationError(
                f"Module '{module_id}' has no valid Python package at {module_path}"
            )

        # Run validation
        validator = validator_class()
        result = await validator.validate(package_path, config=config)

        if not result.passed:
            error_details = "; ".join(f"{e.name}: {e.message}" for e in result.errors)
            raise ModuleValidationError(
                f"Module '{module_id}' failed validation: {result.summary()}. Errors: {error_details}"
            )

        logger.info(f"[module:validated] {module_id} - {result.summary()}")

    def _find_package_dir(self, module_id: str, module_path: Path) -> Path | None:
        """
        Find the Python package directory within a module root.

        Module structure is typically:
            amplifier-module-xyz/
                amplifier_module_xyz/
                    __init__.py
                    (other module files)

        Args:
            module_id: Module identifier (e.g., "provider-anthropic")
            module_path: Path to module root directory

        Returns:
            Path to the Python package directory, or None if not found
        """
        # If the path itself has __init__.py, it's already a package
        if (module_path / "__init__.py").exists():
            return module_path

        # Look for amplifier_module_* directory
        module_name = f"amplifier_module_{module_id.replace('-', '_')}"
        package_dir = module_path / module_name
        if package_dir.exists() and (package_dir / "__init__.py").exists():
            return package_dir

        # Fallback: search for any amplifier_module_* directory
        for item in module_path.iterdir():
            if (
                item.is_dir()
                and item.name.startswith("amplifier_module_")
                and (item / "__init__.py").exists()
            ):
                return item

        return None

    def _make_wasm_mount(
        self, module_path: Path, coordinator: ModuleCoordinator
    ) -> Callable[[ModuleCoordinator], Awaitable[Callable | None]]:
        """Return a mount function that loads a WASM module via Rust ``load_and_mount_wasm()``.

        Calls the Rust ``load_and_mount_wasm()`` binding which resolves the
        module manifest, instantiates a WASM engine, and mounts the loaded
        module directly into the coordinator's ``mount_points`` dict (e.g.
        ``mount_points["tools"]`` for tool modules).

        Args:
            module_path: Path to the .wasm file or directory containing it.
            coordinator: Reserved for future WASM lifecycle management.
                Currently unused — the inner closure receives its own
                ``coord`` argument at mount time.  Kept for signature
                parity with ``_make_grpc_mount``.

        Returns:
            Async mount function that loads and mounts the WASM module.
        """
        # Re-import from _engine: the dispatch block already proved the module
        # exists (resolve_module succeeded), but load_and_mount_wasm could be
        # absent in a version-mismatch scenario.  That ImportError propagates
        # to the caller's outer try/except, which is intentional.
        from amplifier_core._engine import load_and_mount_wasm

        async def wasm_mount(coord: ModuleCoordinator) -> Callable | None:
            result = load_and_mount_wasm(coord, str(module_path))
            logger.info(f"[module:mount] WASM mounted: {result}")
            return None  # No cleanup function for WASM modules

        return wasm_mount

    async def _make_grpc_mount(
        self,
        module_path: Path,
        module_id: str,
        config: dict[str, Any] | None,
        coordinator: ModuleCoordinator,
    ) -> Callable[[ModuleCoordinator], Awaitable[Callable | None]]:
        """Return a mount function that loads a gRPC module via the gRPC loader bridge.

        Reads ``amplifier.toml`` from the module directory for endpoint and
        service configuration, then delegates to the gRPC loader bridge
        (``loader_grpc.load_grpc_module``) which handles channel setup,
        protobuf negotiation, and adapter wrapping.

        Args:
            module_path: Path to the module directory containing amplifier.toml.
            module_id: Module identifier.
            config: Optional module configuration.
            coordinator: The coordinator instance.

        Returns:
            Async mount function from the gRPC loader bridge.
        """
        from .loader_grpc import load_grpc_module

        # Read amplifier.toml for gRPC config
        try:
            import tomli
        except ImportError:
            import tomllib as tomli  # type: ignore[no-redef]

        toml_path = module_path / "amplifier.toml"
        meta: dict[str, Any] = {}
        if toml_path.exists():
            with open(toml_path, "rb") as f:
                meta = tomli.load(f)

        return await load_grpc_module(module_id, config, meta, coordinator)

    async def initialize(
        self, module: Any, coordinator: ModuleCoordinator
    ) -> Callable[[], Awaitable[None]] | None:
        """
        Initialize a loaded module with the coordinator.

        Args:
            module: Module mount function
            coordinator: Module coordinator

        Returns:
            Optional cleanup function
        """
        try:
            cleanup = await module(coordinator)
            return cleanup
        except Exception as e:
            logger.error(f"Failed to initialize module: {e}")
            raise

    def cleanup(self) -> None:
        """Remove all sys.path entries added by this loader."""
        for path in reversed(self._added_paths):
            try:
                sys.path.remove(path)
                logger.debug(f"Removed '{path}' from sys.path")
            except ValueError:
                # Path already removed or never existed
                logger.debug(f"Path '{path}' already removed from sys.path")
        self._added_paths.clear()
