"""Async compatibility layer for PyO3 future_into_py results.

PyO3's future_into_py() returns an awaitable Future, not a Python coroutine.
asyncio.create_task() and inspect.iscoroutine() require actual coroutines.
This wrapper converts PyO3 awaitables into proper coroutines so RustSession
methods are drop-in compatible with the old pure-Python async def methods.
"""


async def _wrap(awaitable):
    """Wrap a PyO3 awaitable in a proper Python coroutine."""
    return await awaitable
