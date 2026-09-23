"""Lifecycle ownership through the public, compiled Rust session and registry.

The tiny filesystem modules only make initialization possible without providers.
Module loading, hook dispatch, session cleanup and unregister are all real Core.
"""

import asyncio
import sys

import pytest

from amplifier_core import AmplifierSession, HookRegistry
from amplifier_core._engine import RustHookRegistry, RustSession


@pytest.fixture
def session_factory(tmp_path, monkeypatch):
    names = []
    for module_type, mount_point in (("orchestrator", "orchestrator"), ("context", "context")):
        name = f"amplifier_module_cleanup_fixture_{module_type}"
        names.append(name)
        package = tmp_path / name
        package.mkdir()
        (package / "__init__.py").write_text(
            f'__amplifier_module_type__ = "{module_type}"\n'
            "class FixtureModule:\n"
            "    async def execute(self, *args, **kwargs):\n"
            "        raise AssertionError('cleanup fixture must never execute a turn')\n"
            "async def mount(coordinator, config=None):\n"
            "    if config and config.get('fail'):\n"
            "        raise RuntimeError('fixture mount failed')\n"
            f'    await coordinator.mount("{mount_point}", FixtureModule())\n'
            "    cleanup_log = coordinator.get_capability('fixture.cleanup')\n"
            "    if cleanup_log is not None:\n"
            f"        return lambda: cleanup_log.append('{module_type}')\n"
        )
    monkeypatch.syspath_prepend(str(tmp_path))

    def create():
        assert AmplifierSession is RustSession
        return AmplifierSession(
            config={
                "session": {
                    "orchestrator": "cleanup-fixture-orchestrator",
                    "context": "cleanup-fixture-context",
                },
                "providers": [],
            }
        )

    yield create
    for name in names:
        sys.modules.pop(name, None)


@pytest.mark.asyncio
async def test_end_is_awaited_before_module_cleanup_unregisters_it(session_factory):
    session = session_factory()
    await session.initialize()
    calls = []

    async def end(event, data):
        await asyncio.sleep(0)
        calls.append((event, data["session_id"]))

    unregister = session.coordinator.hooks.register("session:end", end, name="telemetry")

    def close():
        calls.append("close")
        unregister()

    session.coordinator.register_cleanup(close)
    await session.cleanup()
    assert calls == [("session:end", session.session_id), "close"]
    assert not session.initialized


@pytest.mark.asyncio
async def test_concurrent_cleanup_cannot_close_while_end_handler_is_running(session_factory):
    session = session_factory()
    await session.initialize()
    entered, release = asyncio.Event(), asyncio.Event()
    calls = []

    async def end(event, data):
        calls.append("end:entered")
        entered.set()
        await release.wait()
        calls.append("end:finished")

    session.coordinator.hooks.register("session:end", end, name="telemetry")
    session.coordinator.register_cleanup(lambda: calls.append("close"))
    first = asyncio.create_task(session.cleanup())
    second = None
    try:
        await asyncio.wait_for(entered.wait(), timeout=2)
        second = asyncio.create_task(session.cleanup())
        done, _ = await asyncio.wait({second}, timeout=0.05)
        assert not done
        assert calls == ["end:entered"]
    finally:
        release.set()
        await asyncio.gather(first, *([second] if second else []))
    assert calls.count("end:entered") == 1
    assert calls.index("end:finished") < calls.index("close")


@pytest.mark.asyncio
async def test_cancelled_end_attempt_is_not_replayed_and_python_handler_exits(session_factory):
    session = session_factory()
    await session.initialize()
    entered, release, exited = asyncio.Event(), asyncio.Event(), asyncio.Event()
    calls = []

    async def end(event, data):
        calls.append("end:entered")
        entered.set()
        try:
            await release.wait()
        finally:
            calls.append("end:exited")
            exited.set()

    session.coordinator.hooks.register("session:end", end, name="telemetry")
    session.coordinator.register_cleanup(lambda: calls.append("close"))
    first = asyncio.create_task(session.cleanup())
    try:
        await asyncio.wait_for(entered.wait(), timeout=2)
        first.cancel()
        with pytest.raises(asyncio.CancelledError):
            await first
        await asyncio.wait_for(exited.wait(), timeout=2)
        await asyncio.wait_for(session.cleanup(), timeout=2)
        assert calls == ["end:entered", "end:exited", "close"]
        assert not session.initialized
    finally:
        release.set()
        if not first.done():
            first.cancel()
        await asyncio.gather(first, return_exceptions=True)


@pytest.mark.asyncio
async def test_uninitialized_cleanup_still_releases_partial_resources(session_factory):
    session = session_factory()
    calls = []
    session.coordinator.hooks.register(
        "session:end", lambda event, data: calls.append("end"), name="telemetry"
    )
    session.coordinator.register_cleanup(lambda: calls.append("partial cleanup"))
    await session.cleanup()
    assert calls == ["partial cleanup"]


@pytest.mark.asyncio
async def test_failed_initialization_cleans_up_already_mounted_module(session_factory):
    session = session_factory()
    calls = []
    session.coordinator.register_capability("fixture.cleanup", calls)
    session.config["session"]["context"] = {
        "module": "cleanup-fixture-context", "config": {"fail": True}
    }
    session.coordinator.hooks.register(
        "session:end", lambda event, data: calls.append("end"), name="telemetry"
    )
    with pytest.raises(RuntimeError, match="fixture mount failed"):
        await session.initialize()
    assert not session.initialized
    await session.cleanup()
    assert calls == ["orchestrator"]


@pytest.mark.asyncio
async def test_end_once_per_initialized_lifetime(session_factory):
    session = session_factory()
    calls = []
    session.coordinator.hooks.register(
        "session:end", lambda event, data: calls.append("end"), name="telemetry"
    )
    session.coordinator.register_cleanup(lambda: calls.append("close"))
    await session.initialize()
    await session.cleanup()
    await session.cleanup()
    # Cleanup callbacks remain best-effort/idempotent resources, as before;
    # a second cleanup does not fabricate another terminal lifecycle event.
    assert calls == ["end", "close", "close"]
    await session.initialize()
    await session.cleanup()
    assert calls == ["end", "close", "close", "end", "close"]


@pytest.mark.asyncio
async def test_end_handler_failure_does_not_skip_cleanup(session_factory):
    session = session_factory()
    await session.initialize()
    calls = []

    async def end(event, data):
        calls.append("end")
        raise RuntimeError("fixture failure")

    session.coordinator.hooks.register("session:end", end, name="telemetry")
    session.coordinator.register_cleanup(lambda: calls.append("close"))
    await session.cleanup()
    assert calls == ["end", "close"]
    assert not session.initialized


@pytest.mark.asyncio
@pytest.mark.parametrize("same_event", [False, True])
async def test_unregister_owns_one_registration_even_with_repeated_names(same_event):
    assert HookRegistry is RustHookRegistry
    hooks = HookRegistry()
    calls = []
    second_event = "first:event" if same_event else "second:event"
    first = hooks.register(
        "first:event", lambda event, data: calls.append("first"), name="shared"
    )
    second = hooks.register(
        second_event, lambda event, data: calls.append("second"), name="shared"
    )
    first()
    first()
    for event in dict.fromkeys(("first:event", second_event)):
        await hooks.emit(event, {})
    assert calls == ["second"]
    second()
    second()
    calls.clear()
    for event in dict.fromkeys(("first:event", second_event)):
        await hooks.emit(event, {})
    assert calls == []


@pytest.mark.asyncio
async def test_old_unregister_handle_cannot_remove_new_registration():
    hooks = HookRegistry()
    calls = []
    old = hooks.register("event", lambda event, data: calls.append("old"), name="shared")
    old()
    new = hooks.register("event", lambda event, data: calls.append("new"), name="shared")
    old()
    await hooks.emit("event", {})
    assert calls == ["new"]
    new()


@pytest.mark.asyncio
async def test_name_unregister_and_owned_handles_do_not_remove_other_registrations():
    hooks = HookRegistry()
    calls = []
    old = hooks.register("event", lambda event, data: calls.append("old"), name="shared")
    latest = hooks.register("event", lambda event, data: calls.append("latest"), name="shared")
    # Preserve the existing name API's most-recent-registration behavior.
    hooks.unregister("shared")
    latest()
    await hooks.emit("event", {})
    assert calls == ["old"]
    old()
    calls.clear()
    await hooks.emit("event", {})
    assert calls == []


@pytest.mark.asyncio
async def test_cancelled_hook_only_cancels_its_owned_python_task():
    hooks = HookRegistry()
    entered, exited, release = asyncio.Event(), asyncio.Event(), asyncio.Event()
    sibling_started = asyncio.Event()

    async def sibling():
        sibling_started.set()
        await release.wait()
        return "sibling completed"

    async def hook(event, data):
        entered.set()
        try:
            await release.wait()
        finally:
            exited.set()

    hooks.register("event", hook, name="owned")
    other = asyncio.create_task(sibling())
    pending = asyncio.create_task(hooks.emit("event", {}))
    try:
        await asyncio.wait_for(entered.wait(), timeout=2)
        await asyncio.wait_for(sibling_started.wait(), timeout=2)
        pending.cancel()
        with pytest.raises(asyncio.CancelledError):
            await pending
        await asyncio.wait_for(exited.wait(), timeout=2)
        assert not other.done()
    finally:
        release.set()
        await asyncio.gather(pending, return_exceptions=True)
        assert await other == "sibling completed"


@pytest.mark.asyncio
async def test_async_hook_result_error_and_emitting_context_are_preserved():
    import contextvars

    value = contextvars.ContextVar("owned-hook-context", default="unset")
    hooks = HookRegistry()
    calls = []

    async def failing(event, data):
        await asyncio.sleep(0)
        calls.append(value.get())
        raise RuntimeError("expected hook failure")

    async def result(event, data):
        await asyncio.sleep(0)
        return {"action": "deny", "reason": value.get()}

    value.set("registration")
    hooks.register("event", failing, priority=0, name="failure")
    hooks.register("event", result, priority=1, name="result")
    value.set("emission")
    answer = await hooks.emit("event", {})
    assert calls == ["emission"]
    assert answer.action == "deny"
    assert answer.reason == "emission"


@pytest.mark.asyncio
async def test_owned_hook_cancel_before_start_closes_unstarted_coroutine():
    import inspect
    from amplifier_core._async_compat import _OwnedHookTask

    calls = []

    async def hook():
        calls.append("started")

    coroutine = hook()
    owner = _OwnedHookTask(coroutine)
    owner.cancel()
    assert inspect.getcoroutinestate(coroutine) == inspect.CORO_CLOSED
    with pytest.raises(asyncio.CancelledError):
        await owner.run()
    assert calls == []


@pytest.mark.asyncio
async def test_hook_scheduling_failure_closes_both_unawaited_coroutines(monkeypatch):
    import inspect
    from amplifier_core import _async_compat

    coroutines, calls, refusals = [], [], []

    class ObservedOwner(_async_compat._OwnedHookTask):
        def __init__(self, coroutine):
            super().__init__(coroutine)
            coroutines.append(coroutine)

        def run(self):
            runner = super().run()
            coroutines.append(runner)
            return runner

    loop = asyncio.get_running_loop()
    original = loop.call_soon_threadsafe

    def refuse_hook_schedule(callback, *args, **kwargs):
        # Refuse only the actual PyO3 Python-awaitable conversion. Keep its
        # result-delivery callback working so the real registry returns the
        # normal fail-open HookResult for a failed hook.
        if type(callback).__name__ == "PyEnsureFuture":
            refusals.append(type(callback).__name__)
            raise RuntimeError("fixture hook scheduling refused")
        return original(callback, *args, **kwargs)

    async def hook(event, data):
        calls.append("started")

    monkeypatch.setattr(_async_compat, "_OwnedHookTask", ObservedOwner)
    monkeypatch.setattr(loop, "call_soon_threadsafe", refuse_hook_schedule)
    hooks = HookRegistry()
    hooks.register("event", hook, name="fixture")
    result = await hooks.emit("event", {})
    assert result.action == "continue"
    assert refusals == ["PyEnsureFuture"]
    assert calls == []
    assert len(coroutines) == 2
    assert all(inspect.getcoroutinestate(c) == inspect.CORO_CLOSED for c in coroutines)
