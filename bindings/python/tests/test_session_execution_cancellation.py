"""Native execution ownership; each case has an external eight-second deadline.

No models, token polling, mocked engine, or cancel-all teardown. Settlement is
asserted at public await return, not after a grace period. Run on GIL CPython
3.11-3.14 with the parent's prepared native build.
"""

import asyncio
import gc
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace
import warnings

import pytest

from amplifier_core import AmplifierSession, _engine
from amplifier_core.events import SESSION_END, SESSION_START
from amplifier_core.models import ToolResult


class FakeTool:
    name, description = "block", "Offline cancellable tool"

    def __init__(self, drain=False):
        self.entered, self.finalizing, self.finished = (
            asyncio.Event(), asyncio.Event(), asyncio.Event()
        )
        self.release, self.drain = asyncio.Event(), asyncio.Event()
        if not drain:
            self.drain.set()
        self.task, self.cancelled = None, False

    async def execute(self, input):
        self.task = asyncio.current_task()
        self.entered.set()
        try:
            await self.release.wait()
            return ToolResult(success=True, output="tool complete")
        except asyncio.CancelledError:
            self.cancelled = True
            raise
        finally:
            self.finalizing.set()
            await self.drain.wait()
            self.finished.set()


class OfflineLoader:
    """Only module discovery is synthetic; init, mounts and dispatch are native."""

    def __init__(self, tool):
        self.tool, self.calls = tool, []

    async def load(self, module_id, config, **kwargs):
        async def mount(coordinator):
            if module_id == "offline-loop":
                await coordinator.mount("orchestrator", self)
            elif module_id == "offline-context":
                await coordinator.mount("context", SimpleNamespace(messages=[]))
            else:
                assert module_id == "offline-tool"
                await coordinator.mount("tools", self.tool, name="block")
        return mount

    def get_on_session_ready_queue(self):
        return []

    def clear_on_session_ready_queue(self):
        pass

    async def execute(self, prompt, tools, **kwargs):
        self.calls.append(prompt)
        if prompt == "error":
            raise RuntimeError("fixture execution failure")
        if prompt == "block":
            try:
                return (await tools["block"].execute({})).output
            except asyncio.CancelledError:
                if getattr(self, "cancel_error", False):
                    raise RuntimeError("fixture cancellation finalizer failure") from None
                raise
        return f"result:{prompt}"


async def make_session(raw=False):
    tool = FakeTool()
    loader = OfflineLoader(tool)
    session = AmplifierSession({
        "session": {
            "orchestrator": "offline-loop", "context": "offline-context", "raw": raw,
        },
        "tools": [{"module": "offline-tool"}],
        "providers": [],
    })
    assert isinstance(session.coordinator, _engine.RustCoordinator)
    session.coordinator.loader = loader
    events = []
    for event in (SESSION_START, SESSION_END, "session:config"):
        session.coordinator.hooks.register(
            event, lambda event, data: events.append(event), name=event
        )
    await session.initialize()
    return session, loader, tool, events


def assert_no_owned_task(session):
    for task in asyncio.all_tasks():
        coro = task.get_coro()
        frame = getattr(coro, "cr_frame", None)
        if frame and frame.f_code.co_name in (
            "run_orchestrator", "emit_raw_field_if_configured",
        ):
            assert frame.f_locals.get("coordinator") is not session.coordinator, (
                f"public execute returned with owned task pending: {task!r}"
            )


async def cancelled(task):
    with pytest.raises(asyncio.CancelledError):
        await task


async def cancellation_case(repeated=False, child_error=False):
    session, loader, tool, events = await make_session()
    loader.cancel_error = child_error
    second, _, other_tool, _ = await make_session()
    sibling_release = asyncio.Event()
    sibling = asyncio.create_task(sibling_release.wait())
    other = asyncio.create_task(second.execute("block"))
    await asyncio.wait_for(other_tool.entered.wait(), 2)
    for _ in range(3 if repeated else 1):
        tool = FakeTool(drain=repeated)
        await session.coordinator.mount("tools", tool, name="block")
        outer = asyncio.create_task(session.execute("block"))
        await asyncio.wait_for(tool.entered.wait(), 2)
        outer.cancel()
        if repeated:
            await asyncio.wait_for(tool.finalizing.wait(), 2)
            assert not outer.done(), "public cancellation did not join async finally"
            outer.cancel()  # A second request must not abandon the same child.
            tool.drain.set()
        await cancelled(outer)
        # Deliberately no await between the public return and these assertions.
        assert tool.finished.is_set(), "public cancellation returned before inner finally"
        assert tool.cancelled, "hard cancellation never reached the tool"
        assert tool.task.done(), "owned run_orchestrator is still pending"
        assert_no_owned_task(session)
        assert not sibling.done() and not other.done()
        assert not other_tool.cancelled and not other_tool.finished.is_set()
    sibling_release.set()
    other_tool.release.set()
    assert await sibling
    assert await other == "tool complete"
    assert loader.calls == ["block"] * (3 if repeated else 1)
    await session.cleanup()
    await session.cleanup()
    await second.cleanup()
    assert events == [SESSION_START, SESSION_END]


async def raw_case():
    session, loader, _, _ = await make_session(raw=True)
    hook = FakeTool()

    async def blocking_config(event, data):
        await hook.execute({})

    session.coordinator.hooks.register(
        "session:config", blocking_config, name="blocking-config"
    )
    outer = asyncio.create_task(session.execute("ok"))
    await asyncio.wait_for(hook.entered.wait(), 2)
    outer.cancel()
    await cancelled(outer)
    assert hook.finished.is_set(), "public return left session.raw hook running"
    assert hook.cancelled and hook.task.done()
    assert_no_owned_task(session)
    assert loader.calls == []
    await session.cleanup()
    assert loader.calls == [], "orchestrator started after cancellation during debug"


async def startup_case():
    session, loader, _, _ = await make_session()
    loop = asyncio.get_running_loop()
    original = loop.call_soon_threadsafe
    queued, held = asyncio.Event(), []

    def hold_python_start(callback, *args, **kwargs):
        # Same PyO3 scheduling seam used by the hook-ownership tests. Hold the
        # first conversion, before the debug helper or orchestrator can run.
        if type(callback).__name__ == "PyEnsureFuture" and not held:
            held.append((callback, args, kwargs))
            return original(queued.set)
        return original(callback, *args, **kwargs)

    loop.call_soon_threadsafe = hold_python_start
    try:
        outer = asyncio.create_task(session.execute("ok"))
        await asyncio.wait_for(queued.wait(), 2)
        outer.cancel()
        await cancelled(outer)
        assert loader.calls == []
    finally:
        loop.call_soon_threadsafe = original
        for callback, args, kwargs in held:
            original(callback, *args, **kwargs)
    # A completed second native execution flushes the released startup callback.
    second, _, _, _ = await make_session()
    assert await second.execute("barrier") == "result:barrier"
    assert loader.calls == [], "cancel-before-start allowed late orchestration"
    assert_no_owned_task(session)
    await session.cleanup()
    await second.cleanup()


async def normal_case():
    session, loader, _, events = await make_session(raw=True)
    for _ in range(3):
        assert await session.execute("ok") == "result:ok"
        with pytest.raises(RuntimeError, match="fixture execution failure"):
            await session.execute("error")
    assert loader.calls == ["ok", "error"] * 3
    assert events == [SESSION_START] + ["session:config"] * 6
    assert_no_owned_task(session)
    await session.cleanup()
    await session.cleanup()
    assert events == [SESSION_START] + ["session:config"] * 6 + [SESSION_END]
    assert not session.initialized


def worker(case):
    assert _engine.RUST_AVAILABLE, "native Core is required; never skip this test"
    assert AmplifierSession is _engine.RustSession
    assert Path(_engine.__file__).suffix in (".so", ".pyd"), "not a native engine"
    loop, errors = asyncio.new_event_loop(), []
    asyncio.set_event_loop(loop)
    loop.set_exception_handler(lambda loop, context: errors.append(context))
    print(f"NATIVE:{case}", flush=True)
    with warnings.catch_warnings(record=True) as captured:
        warnings.simplefilter("always", RuntimeWarning)
        try:
            scenarios = {
                "cancel": lambda: cancellation_case(),
                "repeat": lambda: cancellation_case(repeated=True),
                "cancel-error": lambda: cancellation_case(child_error=True),
                "raw": raw_case, "startup": startup_case, "normal": normal_case,
            }
            loop.run_until_complete(scenarios[case]())
            loop.run_until_complete(asyncio.sleep(0))  # Flush done callbacks only.
        finally:
            # Never use asyncio.run's cancel-all shutdown to manufacture settlement.
            loop.close()
            asyncio.set_event_loop(None)
        gc.collect()
        assert not errors, errors
        assert not captured, [str(item.message) for item in captured]
    print(f"SETTLED:{case}", flush=True)


@pytest.mark.parametrize(
    "case", ["cancel", "repeat", "cancel-error", "raw", "startup", "normal"]
)
def test_native_execute_cancellation(case):
    try:
        result = subprocess.run(
            [sys.executable, "-W", "error::RuntimeWarning", str(Path(__file__).resolve()), case],
            capture_output=True, text=True, timeout=8,
        )
    except subprocess.TimeoutExpired as exc:
        pytest.fail(f"{case}: external 8s deadline exceeded; {exc.stdout!r} {exc.stderr!r}")
    assert result.returncode == 0, result.stdout + result.stderr
    assert f"SETTLED:{case}" in result.stdout
    assert not result.stderr, result.stderr


if __name__ == "__main__":
    worker(sys.argv[1])


# Expert-review regressions are appended: the original six-case prefix is frozen.
async def prestart_cancel_case():
    session, loader, tool, events = await make_session()
    barrier, _, _, _ = await make_session()
    for _ in range(3):
        outer = asyncio.create_task(session.execute("block"))
        outer.cancel()  # No yield: the returned coroutine has never been entered.
        await cancelled(outer)
        assert await barrier.execute("barrier") == "result:barrier"
        assert loader.calls == [], "unentered public coroutine started native execution"
        assert not tool.entered.is_set()
        assert events == [], "unentered public coroutine emitted lifecycle events"
        assert_no_owned_task(session)
    await session.cleanup()
    await barrier.cleanup()


async def finalization_hook_case():
    import contextvars

    session, _, tool, _ = await make_session()
    entered, release, late_release = asyncio.Event(), asyncio.Event(), asyncio.Event()
    calls, descendants = [], []
    value = contextvars.ContextVar("finalization-context", default="missing")
    value.set("caller")

    async def finalized(event, data):
        calls.append(("entered", value.get()))
        entered.set()
        try:
            await release.wait()
        finally:
            await asyncio.sleep(0)
            calls.append(("finished", value.get()))

    session.coordinator.hooks.register("tool:finalized", finalized, name="finalized")

    async def late_hook(hooks):
        await late_release.wait()
        await hooks.emit("tool:finalized", {})

    class FinalizingLoop:
        async def execute(self, tools, hooks, **kwargs):
            # Inherits this execution's context but is intentionally not owned.
            descendants.append(asyncio.create_task(late_hook(hooks)))
            try:
                await tools["block"].execute({})
            finally:
                await hooks.emit("tool:finalized", {})

    await session.coordinator.mount("orchestrator", FinalizingLoop())
    outer = asyncio.create_task(session.execute("block"))
    await asyncio.wait_for(tool.entered.wait(), 2)
    outer.cancel()
    await asyncio.wait_for(entered.wait(), 2)
    assert not outer.done(), "cancellation abandoned the async finalization hook"
    release.set()
    await cancelled(outer)
    assert tool.finished.is_set() and tool.task.done()
    assert calls == [("entered", "caller"), ("finished", "caller")]
    assert_no_owned_task(session)
    # A stale inherited execution context cannot veto later legitimate hooks.
    late_release.set()
    await descendants[0]
    assert calls == [("entered", "caller"), ("finished", "caller")] * 2
    await session.cleanup()


async def held_hook_submission_case():
    import inspect
    from amplifier_core import _async_compat

    session, loader, _, _ = await make_session(raw=True)
    loop = asyncio.get_running_loop()
    original_schedule, original_owner = loop.call_soon_threadsafe, _async_compat._OwnedHookTask
    queued, held, coroutines, effects = asyncio.Event(), [], [], []

    class ObservedOwner(original_owner):
        def __init__(self, coroutine):
            coroutines.append(coroutine)
            super().__init__(coroutine)

        def run(self):
            runner = super().run()
            coroutines.append(runner)
            return runner

    def hold_hook(callback, *args, **kwargs):
        if type(callback).__name__ == "PyEnsureFuture" and coroutines and not held:
            held.append((callback, args, kwargs))
            return original_schedule(queued.set)
        return original_schedule(callback, *args, **kwargs)

    async def config_hook(event, data):
        effects.append("started")
        await asyncio.sleep(0)

    session.coordinator.hooks.register("session:config", config_hook, name="held")
    _async_compat._OwnedHookTask = ObservedOwner
    loop.call_soon_threadsafe = hold_hook
    try:
        outer = asyncio.create_task(session.execute("ok"))
        await asyncio.wait_for(queued.wait(), 2)
        outer.cancel()
        # Joining until exact-owner cancellation closes unstarted work is valid;
        # the external process deadline guards a broken join, not a grace period.
        await cancelled(outer)
        assert len(coroutines) == 2
        assert all(inspect.getcoroutinestate(c) == inspect.CORO_CLOSED for c in coroutines), (
            "public cancellation returned with a submitted hook coroutine still open"
        )
        assert effects == [] and loader.calls == []
    finally:
        loop.call_soon_threadsafe = original_schedule
        _async_compat._OwnedHookTask = original_owner
        for callback, args, kwargs in held:
            original_schedule(callback, *args, **kwargs)
    barrier, _, _, _ = await make_session()
    assert await barrier.execute("barrier") == "result:barrier"
    assert effects == [], "released hook callback executed after public cancellation"
    assert loader.calls == []
    assert_no_owned_task(session)
    await session.cleanup()
    await barrier.cleanup()


def expert_worker(case):
    assert _engine.RUST_AVAILABLE and AmplifierSession is _engine.RustSession
    assert Path(_engine.__file__).suffix in (".so", ".pyd")
    loop, errors = asyncio.new_event_loop(), []
    asyncio.set_event_loop(loop)
    loop.set_exception_handler(lambda loop, context: errors.append(context))
    with warnings.catch_warnings(record=True) as captured:
        warnings.simplefilter("always", RuntimeWarning)
        try:
            scenarios = {
                "prestart": prestart_cancel_case,
                "finalization-hook": finalization_hook_case,
                "held-hook": held_hook_submission_case,
            }
            loop.run_until_complete(scenarios[case]())
            loop.run_until_complete(asyncio.sleep(0))
        finally:
            loop.close()  # No cancel-all cleanup, even on assertion failure.
            asyncio.set_event_loop(None)
        gc.collect()
        assert not errors, errors
        assert not captured, [str(item.message) for item in captured]
    print(f"EXPERT-SETTLED:{case}", flush=True)


@pytest.mark.parametrize("case", ["prestart", "finalization-hook", "held-hook"])
def test_native_execute_expert_cancellation(case):
    # run_path loads appended definitions without entering the frozen main guard.
    entry = (
        "import runpy, sys; "
        "ns = runpy.run_path(sys.argv[1]); ns['expert_worker'](sys.argv[2])"
    )
    try:
        result = subprocess.run(
            [sys.executable, "-W", "error::RuntimeWarning", "-c", entry,
             str(Path(__file__).resolve()), case],
            capture_output=True, text=True, timeout=8,
        )
    except subprocess.TimeoutExpired as exc:
        pytest.fail(f"{case}: external 8s deadline exceeded; {exc.stdout!r} {exc.stderr!r}")
    assert result.returncode == 0, result.stdout + result.stderr
    assert f"EXPERT-SETTLED:{case}" in result.stdout
    assert not result.stderr, result.stderr