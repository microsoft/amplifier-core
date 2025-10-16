"""
Module coordination system - the heart of amplifier-core.

Coordinator provides infrastructure context to all modules including:
- Identity: session_id, parent_id (and future: turn_id, span_id)
- Configuration: mount plan access
- Session reference: for spawning child sessions
- Module loader: for dynamic loading

This embodies kernel philosophy's "minimal context plumbing" - providing
identifiers and basic state necessary to make module boundaries work.
"""

import inspect
import logging
from typing import TYPE_CHECKING
from typing import Any

from .hooks import HookRegistry

if TYPE_CHECKING:
    from .loader import ModuleLoader
    from .session import AmplifierSession

logger = logging.getLogger(__name__)


class ModuleCoordinator:
    """
    Central coordination and infrastructure context for all modules.

    Provides:
    - Mount points for module attachment
    - Infrastructure context (IDs, config, session reference)
    - Capability registry for inter-module communication
    - Event system with default field injection
    """

    def __init__(self: "ModuleCoordinator", session: "AmplifierSession"):
        """
        Initialize coordinator with session providing infrastructure context.

        Args:
            session: Parent AmplifierSession providing infrastructure
        """
        self._session = session  # Infrastructure reference

        self.mount_points = {
            "orchestrator": None,  # Single orchestrator
            "providers": {},  # Multiple providers by name
            "tools": {},  # Multiple tools by name
            "context": None,  # Single context manager
            "hooks": HookRegistry(),  # Hook registry (built-in)
            "module-source-resolver": None,  # Optional custom source resolver (kernel extension point)
        }
        self._cleanup_functions = []
        self._capabilities = {}  # Capability registry for inter-module communication

        # Make hooks accessible as an attribute for backward compatibility
        self.hooks = self.mount_points["hooks"]

    @property
    def session(self) -> "AmplifierSession":
        """Parent session reference (infrastructure for spawning children)."""
        return self._session

    @property
    def session_id(self) -> str:
        """Current session ID (infrastructure for persistence/correlation)."""
        return self._session.session_id

    @property
    def parent_id(self) -> str | None:
        """Parent session ID for child sessions (infrastructure for lineage tracking)."""
        return self._session.parent_id

    @property
    def config(self) -> dict:
        """
        Session configuration/mount plan (infrastructure).

        Includes:
        - session: orchestrator and context settings
        - providers, tools, hooks: module configurations
        - agents: config overlays for sub-session spawning (app-layer data)
        """
        return self._session.config

    @property
    def loader(self) -> "ModuleLoader":
        """Module loader (infrastructure for dynamic module loading)."""
        return self._session.loader

    async def mount(self, mount_point: str, module: Any, name: str | None = None) -> None:
        """
        Mount a module at a specific mount point.

        Args:
            mount_point: Where to mount ('orchestrator', 'providers', 'tools', etc.)
            module: The module instance to mount
            name: Optional name for multi-module mount points
        """
        if mount_point not in self.mount_points:
            raise ValueError(f"Unknown mount point: {mount_point}")

        if mount_point in ["orchestrator", "context", "module-source-resolver"]:
            # Single module mount points
            if self.mount_points[mount_point] is not None:
                logger.warning(f"Replacing existing {mount_point}")
            self.mount_points[mount_point] = module
            logger.info(f"Mounted {module.__class__.__name__} at {mount_point}")

        elif mount_point in ["providers", "tools", "agents"]:
            # Multi-module mount points
            if name is None:
                # Try to get name from module
                if hasattr(module, "name"):
                    name = module.name
                else:
                    raise ValueError(f"Name required for {mount_point}")

            self.mount_points[mount_point][name] = module
            logger.info(f"Mounted {module.__class__.__name__} '{name}' at {mount_point}")

        elif mount_point == "hooks":
            raise ValueError("Hooks should be registered directly with the HookRegistry")

    async def unmount(self, mount_point: str, name: str | None = None) -> None:
        """
        Unmount a module from a mount point.

        Args:
            mount_point: Where to unmount from
            name: Name for multi-module mount points
        """
        if mount_point not in self.mount_points:
            raise ValueError(f"Unknown mount point: {mount_point}")

        if mount_point in ["orchestrator", "context", "module-source-resolver"]:
            self.mount_points[mount_point] = None
            logger.info(f"Unmounted {mount_point}")

        elif mount_point in ["providers", "tools", "agents"]:
            if name is None:
                raise ValueError(f"Name required to unmount from {mount_point}")
            if name in self.mount_points[mount_point]:
                del self.mount_points[mount_point][name]
                logger.info(f"Unmounted '{name}' from {mount_point}")

    def get(self, mount_point: str, name: str | None = None) -> Any:
        """
        Get a mounted module.

        Args:
            mount_point: Mount point to get from
            name: Name for multi-module mount points

        Returns:
            The mounted module or dict of modules
        """
        if mount_point not in self.mount_points:
            raise ValueError(f"Unknown mount point: {mount_point}")

        if mount_point in ["orchestrator", "context", "hooks", "module-source-resolver"]:
            return self.mount_points[mount_point]

        if mount_point in ["providers", "tools", "agents"]:
            if name is None:
                # Return all modules at this mount point
                return self.mount_points[mount_point]
            return self.mount_points[mount_point].get(name)
        return None

    def register_cleanup(self, cleanup_fn):
        """Register a cleanup function to be called on shutdown."""
        self._cleanup_functions.append(cleanup_fn)

    def register_capability(self, name: str, value: Any) -> None:
        """
        Register a capability that other modules can access.

        Capabilities provide a mechanism for inter-module communication
        without direct dependencies.

        Args:
            name: Capability name (e.g., 'agents.list', 'agents.get')
            value: The capability (typically a callable)
        """
        self._capabilities[name] = value
        logger.debug(f"Registered capability: {name}")

    def get_capability(self, name: str) -> Any | None:
        """
        Get a registered capability.

        Args:
            name: Capability name

        Returns:
            The capability if registered, None otherwise
        """
        return self._capabilities.get(name)

    async def cleanup(self):
        """Call all registered cleanup functions."""
        for cleanup_fn in reversed(self._cleanup_functions):
            try:
                if callable(cleanup_fn):
                    if inspect.iscoroutinefunction(cleanup_fn):
                        await cleanup_fn()
                    else:
                        result = cleanup_fn()
                        if inspect.iscoroutine(result):
                            await result
            except Exception as e:
                logger.error(f"Error during cleanup: {e}")
