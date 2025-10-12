"""
Module coordination system - the heart of amplifier-core.
This is the ONLY business logic in core - how modules connect.
"""

import inspect
import logging
from typing import Any

from .hooks import HookRegistry

logger = logging.getLogger(__name__)


class ModuleCoordinator:
    """
    Central coordination point for all modules.
    Provides mount points where modules can attach themselves.
    """

    def __init__(self):
        """Initialize mount points for different module types."""
        self.mount_points = {
            "orchestrator": None,  # Single orchestrator
            "providers": {},  # Multiple providers by name
            "tools": {},  # Multiple tools by name
            "agents": {},  # Multiple agents by name
            "context": None,  # Single context manager
            "hooks": HookRegistry(),  # Hook registry (built-in)
        }
        self._cleanup_functions = []

        # Make hooks accessible as an attribute for backward compatibility
        self.hooks = self.mount_points["hooks"]

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

        if mount_point in ["orchestrator", "context"]:
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

        if mount_point in ["orchestrator", "context"]:
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

        if mount_point in ["orchestrator", "context", "hooks"]:
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
