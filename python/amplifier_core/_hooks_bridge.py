"""
Hook registry bridge for the Rust PyO3 session.

When the Rust-backed Session is active, hooks are dispatched via the
Python HookRegistry (which handles async handlers natively) rather than
the Rust kernel's HookRegistry (which requires PyO3 async bridging).

This approach avoids the complexity of calling async Python handlers
from inside a tokio runtime via run_coroutine_threadsafe.
"""

from .hooks import HookRegistry


def create_hook_registry():
    """Create a Python HookRegistry for use with the Rust session.
    
    Returns a real Python HookRegistry instance that handles async
    handlers correctly via Python's native async/await.
    """
    return HookRegistry()
