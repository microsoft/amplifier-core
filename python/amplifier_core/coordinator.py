"""Module coordinator for mount points and capabilities.

The coordinator implementation lives in the Rust kernel. This module
re-exports for backward compatibility with:
    from amplifier_core.coordinator import ModuleCoordinator
"""

from amplifier_core._engine import RustCoordinator as ModuleCoordinator

__all__ = ["ModuleCoordinator"]
