"""Module source resolution system.

Provides protocols for flexible module sourcing. Actual implementations
live in app-layer modules, keeping the kernel pure mechanism-only.

Architecture:
- ModuleSource: Protocol for source types
- ModuleSourceResolver: Protocol for resolution strategies

The kernel only defines the contracts. All policy implementations
(file paths, git, packages, layered resolution) live at app layer.
"""

import logging
from abc import ABC
from abc import abstractmethod
from pathlib import Path
from typing import Protocol

logger = logging.getLogger(__name__)


# ============================================================================
# Exceptions
# ============================================================================


class ModuleNotFoundError(Exception):
    """Raised when a module cannot be found in any resolution layer."""

    pass


class ModuleLoadError(Exception):
    """Raised when a module is found but cannot be loaded."""

    pass


# ============================================================================
# ModuleSource Protocol
# ============================================================================


class ModuleSource(ABC):
    """Base class for module sources.

    Implementations resolve to filesystem paths where modules can be imported.
    """

    @abstractmethod
    def resolve(self) -> Path:
        """Resolve source to filesystem path.

        Returns:
            Path to directory containing importable Python module

        Raises:
            ModuleNotFoundError: Source cannot be resolved
            OSError: Filesystem access error
        """
        pass


# ============================================================================
# ModuleSourceResolver Protocol
# ============================================================================


class ModuleSourceResolver(Protocol):
    """Protocol for module source resolution strategies.

    Implementations decide WHERE to find modules based on module ID.
    This is app-layer policy - different apps can use different strategies.
    """

    def resolve(self, module_id: str, source_hint=None, profile_hint=None) -> ModuleSource:
        """Resolve module ID to a source.

        Args:
            module_id: Module identifier (e.g., "tool-bash")
            source_hint: Optional hint from bundle config (app-defined format)
            profile_hint: DEPRECATED - use source_hint instead (for backward compat only)

        Returns:
            ModuleSource that can be resolved to a path

        Raises:
            ModuleNotFoundError: Module cannot be found
            
        FIXME: The profile_hint parameter exists only for backward compatibility
        with implementations that haven't migrated yet. All callers should use
        source_hint. Remove profile_hint after all downstream repos are updated
        (target: v2.0 release).
        """
        ...
