"""Async compatibility layer for PyO3 future_into_py results.

PyO3's future_into_py() returns an awaitable Future, not a Python coroutine.
asyncio.create_task() and inspect.iscoroutine() require actual coroutines.
This wrapper converts PyO3 awaitables into proper coroutines so RustSession
methods are drop-in compatible with the old pure-Python async def methods.
"""

import asyncio


class _OwnedHookTask:
    """Let a dropped Rust hook waiter cancel only its scheduled Python task.

    ``run`` is scheduled by into_future_with_locals, retaining the emitting
    task's context. ``cancel`` must be called on that same event loop. Remember
    cancellation before startup too: a dropped waiter must not start its hook
    later merely because the loop has not yet run the scheduled coroutine.
    """

    def __init__(self, coroutine):
        self._coroutine = coroutine
        self._task = None
        self._cancel_requested = False

    async def run(self):
        if self._cancel_requested:
            self._coroutine.close()
            raise asyncio.CancelledError
        self._task = asyncio.current_task()
        try:
            return await self._coroutine
        finally:
            self._task = None

    def cancel(self):
        self._cancel_requested = True
        if self._task is not None:
            self._task.cancel()
        else:
            self._coroutine.close()


async def _wrap(awaitable):
    """Wrap a PyO3 awaitable in a proper Python coroutine."""
    return await awaitable
