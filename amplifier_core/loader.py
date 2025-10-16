"""
Module loader for discovering and loading Amplifier modules.
Supports both entry points and filesystem discovery.

With module source resolution:
- Uses ModuleSourceResolver if mounted in coordinator
- Falls back to EntryPointResolver if no resolver provided
- Supports flexible module sourcing (git, local, packages)
"""

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


class ModuleLoader:
    """
    Discovers and loads Amplifier modules.

    Supports source resolution:
    - Uses ModuleSourceResolver from coordinator if available
    - Falls back to EntryPointResolver if no resolver mounted
    - Backward compatible with existing entry point discovery

    Legacy discovery (if no resolver):
    1. Python entry points (installed packages)
    2. Environment variables (AMPLIFIER_MODULES)
    3. Filesystem paths
    """

    def __init__(self, coordinator: ModuleCoordinator | None = None, search_paths: list[Path] | None = None):
        """
        Initialize module loader.

        Args:
            coordinator: Optional coordinator (for resolver injection)
            search_paths: Optional list of filesystem paths (legacy)
        """
        self._loaded_modules: dict[str, Any] = {}
        self._module_info: dict[str, ModuleInfo] = {}
        self._search_paths = search_paths
        self._coordinator = coordinator

        # Get source resolver from coordinator (if provided)
        self._source_resolver = None
        if coordinator:
            self._source_resolver = coordinator.get("module-source-resolver")

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
                    # Extract module info from entry point metadata
                    module_info = ModuleInfo(
                        id=ep.name,
                        name=ep.name.replace("-", " ").title(),
                        version="1.0.0",  # Would need to get from package metadata
                        type=self._guess_module_type(ep.name),
                        mount_point=self._guess_mount_point(ep.name),
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
                    module_info = ModuleInfo(
                        id=module_id,
                        name=module_id.replace("-", " ").title(),
                        version="1.0.0",
                        type=self._guess_module_type(module_id),
                        mount_point=self._guess_mount_point(module_id),
                        description=f"Module: {module_id}",
                    )
                    modules.append(module_info)
                    self._module_info[module_id] = module_info

                    logger.debug(f"Discovered module '{module_id}' from filesystem")

                except Exception as e:
                    logger.error(f"Error discovering module {item.name}: {e}")

        return modules

    async def load(
        self, module_id: str, config: dict[str, Any] | None = None, profile_source: str | dict | None = None
    ) -> Callable[[ModuleCoordinator], Awaitable[Callable | None]]:
        """
        Load a specific module using source resolution.

        Args:
            module_id: Module identifier
            config: Optional module configuration
            profile_source: Optional source URI/object from profile

        Returns:
            Mount function for the module

        Raises:
            ValueError: Module not found or failed to load
        """
        if module_id in self._loaded_modules:
            logger.debug(f"Module '{module_id}' already loaded")
            return self._loaded_modules[module_id]

        try:
            # Resolve module source
            try:
                if self._source_resolver is None:
                    # No resolver mounted - app layer should mount one before loading modules
                    raise ValueError(
                        f"No module source resolver mounted. App layer should mount a resolver "
                        f"before loading module '{module_id}'"
                    )
                source = self._source_resolver.resolve(module_id, profile_source)
                module_path = source.resolve()
                logger.info(f"[module:mount] {module_id} from {source}")
            except Exception as resolve_error:
                # Import here to avoid circular dependency
                from .module_sources import ModuleNotFoundError as SourceNotFoundError

                if isinstance(resolve_error, SourceNotFoundError):
                    # Fall back to legacy discovery
                    logger.debug(f"Source resolution failed for '{module_id}', trying legacy discovery")
                    mount_fn = await self._legacy_load(module_id, config)
                    if mount_fn:
                        return mount_fn
                raise resolve_error

            # Add module path to sys.path temporarily if needed
            path_str = str(module_path)
            path_added = False
            if path_str not in sys.path:
                sys.path.insert(0, path_str)
                path_added = True

            try:
                # Try to load via entry point first
                mount_fn = self._load_entry_point(module_id, config)
                if mount_fn:
                    self._loaded_modules[module_id] = mount_fn
                    return mount_fn

                # Try filesystem loading
                mount_fn = self._load_filesystem(module_id, config)
                if mount_fn:
                    self._loaded_modules[module_id] = mount_fn
                    return mount_fn

                raise ValueError(f"Module '{module_id}' found at {module_path} but failed to load")

            finally:
                # Clean up sys.path
                if path_added and path_str in sys.path:
                    sys.path.remove(path_str)

        except Exception as e:
            logger.error(f"Failed to load module '{module_id}': {e}")
            raise

    async def _legacy_load(self, module_id: str, config: dict[str, Any] | None = None) -> Callable | None:
        """Legacy loading without source resolution (backward compat).

        Args:
            module_id: Module identifier
            config: Optional module configuration

        Returns:
            Mount function if found, None otherwise
        """
        # Try entry point
        mount_fn = self._load_entry_point(module_id, config)
        if mount_fn:
            self._loaded_modules[module_id] = mount_fn
            return mount_fn

        # Try filesystem
        mount_fn = self._load_filesystem(module_id, config)
        if mount_fn:
            self._loaded_modules[module_id] = mount_fn
            return mount_fn

        return None

    def _load_entry_point(self, module_id: str, config: dict[str, Any] | None = None) -> Callable | None:
        """Load module via entry point."""
        try:
            eps = importlib.metadata.entry_points(group="amplifier.modules")

            for ep in eps:
                if ep.name == module_id:
                    # Load the mount function
                    mount_fn = ep.load()
                    logger.info(f"Loaded module '{module_id}' via entry point")

                    # Return a wrapper that passes config
                    async def mount_with_config(coordinator: ModuleCoordinator, fn=mount_fn):
                        return await fn(coordinator, config or {})

                    return mount_with_config

        except Exception as e:
            logger.debug(f"Could not load '{module_id}' via entry point: {e}")

        return None

    def _load_filesystem(self, module_id: str, config: dict[str, Any] | None = None) -> Callable | None:
        """Load module from filesystem."""
        try:
            # Try to import the module
            module_name = f"amplifier_module_{module_id.replace('-', '_')}"
            module = importlib.import_module(module_name)

            # Get the mount function
            if hasattr(module, "mount"):
                mount_fn = module.mount
                logger.info(f"Loaded module '{module_id}' from filesystem")

                # Return a wrapper that passes config
                async def mount_with_config(coordinator: ModuleCoordinator):
                    return await mount_fn(coordinator, config or {})

                return mount_with_config

        except Exception as e:
            logger.debug(f"Could not load '{module_id}' from filesystem: {e}")

        return None

    def _guess_module_type(  # type: ignore[return]
        self, module_id: str
    ) -> Literal["orchestrator", "provider", "tool", "agent", "context", "hook"]:
        """Guess module type from its ID."""
        if "loop" in module_id or "orchestrat" in module_id:
            return "orchestrator"
        if "provider" in module_id:
            return "provider"
        if "tool" in module_id:
            return "tool"
        if "agent" in module_id:
            return "agent"
        if "context" in module_id:
            return "context"
        if "hook" in module_id:
            return "hook"
        # Default to tool if unknown
        return "tool"

    def _guess_mount_point(self, module_id: str) -> str:
        """Guess mount point from module type."""
        module_type = self._guess_module_type(module_id)

        if module_type == "orchestrator":
            return "orchestrator"
        if module_type == "provider":
            return "providers"
        if module_type == "tool":
            return "tools"
        if module_type == "agent":
            return "agents"
        if module_type == "context":
            return "context"
        if module_type == "hook":
            return "hooks"
        return "unknown"

    async def initialize(self, module: Any, coordinator: ModuleCoordinator) -> Callable[[], Awaitable[None]] | None:
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
