"""Async compatibility layer for PyO3 future_into_py results.

PyO3's future_into_py() returns an awaitable Future, not a Python coroutine.
asyncio.create_task() and inspect.iscoroutine() require actual coroutines.
This wrapper converts PyO3 awaitables into proper coroutines so RustSession
methods are drop-in compatible with the old pure-Python async def methods.
"""

import asyncio
import contextvars

_execution_owner = contextvars.ContextVar("amplifier_execution_owner", default=None)


class _ExecutionOwner:
    """One native execute scope, its lazy Python stages, and its settlement.

    Rust calls ``start`` under the GIL; all task operations run on the captured
    loop. No stage coroutine exists until that loop accepts the work. The Rust
    scope's Drop callback and the last observed task's done callback jointly
    acknowledge settlement; requesting cancellation alone never acknowledges it.
    """

    def __init__(self, loop, context):
        self._loop = loop
        self.context = (
            context.copy() if context is not None else contextvars.copy_context()
        )
        self.context.run(_execution_owner.set, self)
        self._settled = loop.create_future()
        self._tasks = set()
        self._hooks = set()
        self._stage = None
        self._cancel_requested = False
        self._rust_released = False

    def start(self, factory, args):
        result = self._loop.create_future()
        # A conversion failure may leave no Rust receiver. Always observe errors.
        result.add_done_callback(self._observe_result)
        self._loop.call_soon_threadsafe(
            self._start, result, factory, args, context=self.context
        )
        return result

    @staticmethod
    def _observe_result(result):
        if not result.cancelled():
            result.exception()

    def _start(self, result, factory, args):
        if self._cancel_requested or self._rust_released:
            result.cancel()
            return
        coroutine = None
        # A task factory can raise even a fatal exception before adoption. Close
        # the coroutine and deliver that same exception to the native receiver.
        try:
            coroutine = factory(*args)
            task = self._loop.create_task(coroutine)
        except BaseException as error:  # noqa: BLE001
            if coroutine is not None:
                coroutine.close()
            result.set_exception(error)
            return
        self._stage = task
        self.observe(task)

        def deliver(task):
            if result.done():
                return
            if task.cancelled():
                result.cancel()
            else:
                error = task.exception()
                if error is None:
                    result.set_result(task.result())
                else:
                    result.set_exception(error)

        task.add_done_callback(deliver)

    def observe(self, task):
        self._tasks.add(task)
        task.add_done_callback(self._task_done)

    def hook_submitted(self, hook):
        # Enqueued from captured TaskLocals before PyEnsureFuture is submitted.
        # Stale context on a detached descendant must not reopen a settled join.
        if not self._settled.done() and not hook._finished:
            self._hooks.add(hook)

    def hook_finished(self, hook):
        self._hooks.discard(hook)
        self._maybe_settle()

    def _task_done(self, task):
        self._observe_result(task)
        self._tasks.discard(task)
        if self._stage is task:
            self._stage = None
        self._maybe_settle()

    def cancel(self):
        if not self._cancel_requested:
            self._cancel_requested = True
            if self._stage is not None:
                self._stage.cancel()

    def scope_released(self, cancel):
        # A failed into_future conversion can leave an accepted stage without
        # a receiver, even though Rust returned an ordinary setup error.
        if cancel or self._stage is not None:
            self.cancel()
        self._rust_released = True
        self._maybe_settle()

    def _maybe_settle(self):
        if (
            self._rust_released
            and not self._tasks
            and not self._hooks
            and not self._settled.done()
        ):
            self._settled.set_result(None)

    async def _join(self):
        while not self._settled.done():
            try:
                await asyncio.shield(self._settled)
            except asyncio.CancelledError:
                # Repeated Ctrl-C must not detach a finally block already draining.
                continue

    async def wait(self, start, prompt):
        native = None
        cancelled = False
        try:
            # No native future exists before this protected coroutine is entered.
            native = start(prompt, self)
            return await native
        except asyncio.CancelledError:
            cancelled = True
            self.cancel()
            raise
        finally:
            if native is None:
                # A synchronous native-start error cannot own scheduled work.
                # Any constructed guard is dropped by the failed conversion.
                pass
            elif cancelled:
                await self._join()
            else:
                try:
                    await asyncio.shield(self._settled)
                except asyncio.CancelledError:
                    self.cancel()
                    await self._join()
                    raise


async def _execute(start, prompt):
    owner = _ExecutionOwner(asyncio.get_running_loop(), contextvars.copy_context())
    return await owner.wait(start, prompt)


class _OwnedHookTask:
    """Let a dropped Rust hook waiter cancel only its scheduled Python task.

    ``run`` is scheduled by into_future_with_locals, retaining the emitting
    task's context. ``cancel`` must be called on that same event loop. Remember
    cancellation before startup too: a dropped waiter must not start its hook
    later merely because the loop has not yet run the scheduled coroutine.
    """

    def __init__(self, coroutine):
        self._coroutine = coroutine
        self._runner = None
        self._task = None
        self._cancel_requested = False
        self._execution = None
        self._finished = False

    def bind_execution(self, context):
        # Called under the GIL from Rust. Read the emitting TaskLocals context,
        # never the ambient Tokio thread's context, and mutate joins on the loop.
        execution = context.get(_execution_owner) if context is not None else None
        if execution is not None:
            self._execution = execution
            try:
                execution._loop.call_soon_threadsafe(execution.hook_submitted, self)
            except BaseException:
                # No runner was submitted: close before propagating any failure.
                self._coroutine.close()
                self._execution = None
                raise

    def run(self):
        self._runner = self._run()
        return self._runner

    async def _run(self):
        if self._cancel_requested:
            self._coroutine.close()
            raise asyncio.CancelledError
        self._task = asyncio.current_task()
        self._task.add_done_callback(self._done)
        try:
            return await self._coroutine
        finally:
            self._task = None

    def _done(self, task):
        if not task.cancelled():
            task.exception()
        self._finish()

    def _finish(self):
        if not self._finished:
            self._finished = True
            execution, self._execution = self._execution, None
            if execution is not None:
                execution._loop.call_soon_threadsafe(execution.hook_finished, self)

    def cancel(self):
        self._cancel_requested = True
        if self._task is not None:
            self._task.cancel()
        else:
            self._coroutine.close()
            if self._runner is not None:
                self._runner.close()
            self._finish()


async def _wrap(awaitable):
    """Wrap a PyO3 awaitable in a proper Python coroutine."""
    return await awaitable
