"""Cancellation token for cooperative session cancellation.

The cancellation implementation lives in the Rust kernel. This module
re-exports for backward compatibility with:
    from amplifier_core.cancellation import CancellationToken
    from amplifier_core.cancellation import CancellationState
"""

from enum import Enum

from amplifier_core._engine import RustCancellationToken as CancellationToken


class CancellationState(Enum):
    """Cancellation state machine states."""

    NONE = "none"  # Running normally
    GRACEFUL = "graceful"  # Waiting for current tools to complete
    IMMEDIATE = "immediate"  # Stop now, synthesize results


__all__ = ["CancellationToken", "CancellationState"]
