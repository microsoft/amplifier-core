"""Source-resolved tools are admitted on their actual runtime instance."""

import asyncio
import importlib
import sys
from contextlib import asynccontextmanager

import pytest

from amplifier_core import AmplifierSession, HookResult, ModuleCoordinator
from amplifier_core._session_init import _mount_tool
from amplifier_core.events import MODULE_LOAD_FAILED
from amplifier_core.loader import ModuleLoader, ModuleValidationError
from amplifier_core.validation import ToolValidator


TOOL_SOURCE = """
from amplifier_core import HookResult

__amplifier_module_type__ = "tool"
HANDLES, CLEANUPS, CLOSED, INSTANCES, READY, HOOK_CALLS = [], [], [], [], [], []

class Tool:
    description = "Synthetic local tool"
    def __init__(self, name):
        self.name = name
    async def execute(self, input):
        return {}

class InvalidTool:
    name = "invalid"
    description = "Missing execute"

SINGLETON = Tool("singleton")

async def mount(coordinator, config):
    mode = config.get("mode", "valid")
    if mode == "singleton":
        await coordinator.mount("tools", SINGLETON, name="singleton")
        return None
    handle = open(config["resource_path"], "a")
    HANDLES.append(handle)
    name = config.get("name", "runtime-tool")
    async def cleanup():
        CLOSED.append(name)
        handle.close()
    CLEANUPS.append(cleanup)
    if config.get("registered") or mode in ("direct", "empty", "unsupported"):
        coordinator.register_cleanup(cleanup)
    if config.get("side_effects"):
        coordinator.register_capability("admission-test", handle)
        def on_event(event, data):
            HOOK_CALLS.append(event)
            return HookResult()
        coordinator.hooks.register("admission:test", on_event, name="admission-test")
    if mode == "conditional":
        return cleanup
    if mode == "empty":
        return None
    if mode == "unsupported":
        return config["return_value"]
    tool = InvalidTool() if mode == "invalid" else Tool(name)
    INSTANCES.append(tool)
    if mode == "direct":
        return tool
    if config.get("remove_slot"):
        coordinator.get("tools").pop("remove")
    await coordinator.mount("tools", tool, name=name)
    if config.get("extra_slot"):
        await coordinator.mount("tools", Tool("new-slot"), name="new-slot")
    if "error" in config:
        raise config["error"]
    if "entered" in config:
        config["entered"].set()
        await config["release"].wait()
    return cleanup

async def on_session_ready(coordinator):
    READY.append(True)
"""

ORCHESTRATOR_SOURCE = """
__amplifier_module_type__ = "orchestrator"
class Orchestrator:
    async def execute(self, prompt, context, providers, tools, hooks, **kwargs):
        return "unused"
async def mount(coordinator, config):
    await coordinator.mount("orchestrator", Orchestrator())
"""

CONTEXT_SOURCE = """
__amplifier_module_type__ = "context"
class Context:
    async def add_message(self, message): pass
    async def get_messages(self): return []
    async def get_messages_for_request(self, request=None): return []
    async def set_messages(self, messages): pass
    async def clear(self): pass
async def mount(coordinator, config):
    await coordinator.mount("context", Context())
"""


class _Source:
    def __init__(self, path):
        self.path = path

    def resolve(self):
        return self.path


class _Resolver:
    def __init__(self, roots):
        self.roots = roots

    def resolve(self, module_id, source_hint=None, profile_hint=None):
        return _Source(self.roots[module_id])


@pytest.fixture
def packages(tmp_path):
    roots = {}
    names = []

    def add(module_id, source=TOOL_SOURCE):
        name = "amplifier_module_" + module_id.replace("-", "_")
        root = tmp_path / module_id
        package = root / name
        package.mkdir(parents=True)
        (package / "__init__.py").write_text(source)
        roots[module_id] = root
        names.append(name)
        return name, package

    yield add, roots
    for name in names:
        for imported in list(sys.modules):
            if imported == name or imported.startswith(name + "."):
                sys.modules.pop(imported, None)
    importlib.invalidate_caches()


@asynccontextmanager
async def _runtime(roots):
    coordinator = ModuleCoordinator()
    await coordinator.mount("module-source-resolver", _Resolver(roots))
    loader = ModuleLoader(coordinator=coordinator)
    coordinator.loader = loader
    try:
        yield coordinator, loader
    finally:
        try:
            await coordinator.cleanup()
        finally:
            loader.cleanup()


def _config(tmp_path, **values):
    return {"resource_path": str(tmp_path / "resource.txt"), **values}


@pytest.mark.asyncio
async def test_load_opens_nothing_then_checks_the_one_runtime_instance(
    packages, tmp_path, monkeypatch
):
    add, roots = packages
    name, _ = add("tool-admission-single")
    checked = []
    original = ToolValidator._check_tool_methods

    def observe(self, result, tool):
        assert coordinator._cleanup_fns == [module.CLEANUPS[0]]
        assert module.CLOSED == []
        checked.append(tool)
        return original(self, result, tool)

    monkeypatch.setattr(ToolValidator, "_check_tool_methods", observe)
    async with _runtime(roots) as (coordinator, loader):
        mount = await loader.load("tool-admission-single", _config(tmp_path))
        module = sys.modules[name]
        assert module.HANDLES == []
        assert loader._loaded_modules["tool-admission-single"] is module.mount
        result = await _mount_tool(loader, "tool-admission-single", mount, coordinator)
        tool = coordinator.get("tools")["runtime-tool"]
        assert checked == [tool]
        assert tool is module.INSTANCES[0]
        assert len(module.HANDLES) == 1 and not module.HANDLES[0].closed
        assert result is None
        assert coordinator._cleanup_fns == [module.CLEANUPS[0]]
        assert module.CLOSED == []
    assert module.CLOSED == ["runtime-tool"]
    assert module.HANDLES[0].closed


@pytest.mark.asyncio
async def test_source_signature_failure_does_not_enter_mount(packages, tmp_path):
    add, roots = packages
    name, _ = add(
        "tool-admission-signature",
        "__amplifier_module_type__ = 'tool'\n"
        "ENTERED = []\n"
        "async def mount(coordinator):\n    ENTERED.append(True)\n",
    )
    async with _runtime(roots) as (_, loader):
        with pytest.raises(ModuleValidationError):
            await loader.load("tool-admission-signature", _config(tmp_path))
        assert sys.modules[name].ENTERED == []


@pytest.mark.asyncio
@pytest.mark.parametrize("mode", ["invalid", "empty"])
async def test_optional_admission_keeps_cleanup_and_initializes_later_tool(
    packages, tmp_path, mode
):
    add, roots = packages
    add("loop-admission", ORCHESTRATOR_SOURCE)
    add("context-admission", CONTEXT_SOURCE)
    bad_name, _ = add("tool-admission-bad")
    good_name, _ = add("tool-admission-good")
    failures = []

    def capture(event, data):
        failures.append(dict(data))
        return HookResult()

    config = {
        "session": {"orchestrator": "loop-admission", "context": "context-admission"},
        "tools": [
            {"module": "tool-admission-bad", "config": _config(tmp_path, mode=mode)},
            {"module": "tool-admission-good", "config": _config(tmp_path, name="good")},
        ],
    }
    session = AmplifierSession(config)
    coordinator = session.coordinator
    await coordinator.mount("module-source-resolver", _Resolver(roots))
    loader = ModuleLoader(coordinator=coordinator)
    coordinator.loader = loader
    try:
        coordinator.hooks.register(MODULE_LOAD_FAILED, capture, name="capture-failure")
        await session.initialize()
        bad, good = sys.modules[bad_name], sys.modules[good_name]
        assert list(coordinator.get("tools")) == ["good"]
        assert coordinator.get("orchestrator") is not None
        assert coordinator.get("context") is not None
        if mode == "invalid":
            assert len(failures) == 1
            assert failures[0]["module_id"] == "tool-admission-bad"
            assert failures[0]["reason_code"] == "validation_failed"
            assert bad.READY == []
        else:
            assert failures == []
            assert bad.READY == [True]
        assert good.READY == [True]
        assert loader.get_on_session_ready_queue() == []
        assert len(bad.HANDLES) == len(good.HANDLES) == 1
        assert bad.CLOSED == good.CLOSED == []
    finally:
        try:
            await session.cleanup()
        finally:
            loader.cleanup()
    assert bad.CLOSED == ["runtime-tool"] and good.CLOSED == ["good"]
    assert bad.HANDLES[0].closed and good.HANDLES[0].closed


@pytest.mark.asyncio
async def test_cached_module_revalidates_and_restores_only_new_or_replaced_slots(
    packages, tmp_path
):
    add, roots = packages
    name, _ = add("tool-admission-cached")
    async with _runtime(roots) as (coordinator, loader):
        first = await loader.load(
            "tool-admission-cached", _config(tmp_path, name="replace")
        )
        await _mount_tool(loader, "tool-admission-cached", first, coordinator)
        await coordinator.mount("tools", object(), name="remove")
        await coordinator.mount("tools", object(), name="tail")
        before = list(coordinator.get("tools").items())
        second = await loader.load(
            "tool-admission-cached",
            _config(
                tmp_path, name="replace", mode="invalid", remove_slot=True,
                extra_slot=True, side_effects=True,
            ),
        )
        with pytest.raises(ModuleValidationError):
            await _mount_tool(loader, "tool-admission-cached", second, coordinator)
        assert list(coordinator.get("tools").items()) == [
            (key, tool) for key, tool in before if key != "remove"
        ]
        module = sys.modules[name]
        assert len(module.HANDLES) == 2
        assert module.CLOSED == []
        assert all(not handle.closed for handle in module.HANDLES)
        assert coordinator.get_capability("admission-test") is module.HANDLES[1]
        await coordinator.hooks.emit("admission:test", {})
        assert module.HOOK_CALLS == ["admission:test"]
    assert module.CLOSED == ["replace", "replace"]
    assert all(handle.closed for handle in module.HANDLES)


@pytest.mark.asyncio
@pytest.mark.parametrize("cancel", [False, True], ids=["raised-mount", "cancelled-mount"])
async def test_interrupted_mount_restores_tools_but_retains_registered_effects(
    packages, tmp_path, cancel
):
    add, roots = packages
    name, _ = add("tool-admission-interrupted")
    error = RuntimeError("original mount failure")
    entered, release = asyncio.Event(), asyncio.Event()
    values = {"entered": entered, "release": release} if cancel else {"error": error}
    async with _runtime(roots) as (coordinator, loader):
        for key in ("replace", "remove", "tail"):
            await coordinator.mount("tools", object(), name=key)
        before = list(coordinator.get("tools").items())
        mount = await loader.load(
            "tool-admission-interrupted",
            _config(tmp_path, name="replace", registered=True, remove_slot=True,
                    extra_slot=True, side_effects=True, **values),
        )
        if cancel:
            task = asyncio.create_task(
                _mount_tool(loader, "tool-admission-interrupted", mount, coordinator)
            )
            try:
                await asyncio.wait_for(entered.wait(), 5)
                task.cancel("stop-tool")
                with pytest.raises(asyncio.CancelledError, match="stop-tool"):
                    await task
            finally:
                if not task.done():
                    task.cancel()
                    await asyncio.gather(task, return_exceptions=True)
        else:
            with pytest.raises(RuntimeError) as failure:
                await _mount_tool(loader, "tool-admission-interrupted", mount, coordinator)
            assert failure.value is error
        assert list(coordinator.get("tools").items()) == [
            (key, tool) for key, tool in before if key != "remove"
        ]
        module = sys.modules[name]
        assert len(module.HANDLES) == 1 and module.CLOSED == []
        # Deleted slots, hooks and capabilities remain module-owned effects.
        assert coordinator.get_capability("admission-test") is module.HANDLES[0]
        await coordinator.hooks.emit("admission:test", {})
        assert module.HOOK_CALLS == ["admission:test"]
    assert module.CLOSED == ["replace"]
    assert module.HANDLES[0].closed


@pytest.mark.asyncio
@pytest.mark.parametrize("mode", ["valid", "direct", "conditional", "empty"])
async def test_direct_loader_preserves_return_value_without_instance_admission(
    packages, tmp_path, monkeypatch, mode
):
    add, roots = packages
    name, _ = add("tool-admission-return")

    def unexpected_inspection(self, result, tool):
        pytest.fail("Direct loader mounting must leave instance admission to its caller")

    monkeypatch.setattr(ToolValidator, "_check_tool_methods", unexpected_inspection)
    async with _runtime(roots) as (coordinator, loader):
        mount = await loader.load("tool-admission-return", _config(tmp_path, mode=mode))
        result = await mount(coordinator)
        module = sys.modules[name]
        if mode == "empty":
            assert result is None
        elif mode == "direct":
            assert result is module.INSTANCES[0]
        else:
            assert result is module.CLEANUPS[0]
            coordinator.register_cleanup(result)
        if mode == "valid":
            assert coordinator.get("tools")["runtime-tool"] is module.INSTANCES[0]
        else:
            assert coordinator.get("tools") == {}
        assert len(module.HANDLES) == 1 and module.CLOSED == []
    assert module.CLOSED == ["runtime-tool"]
    assert module.HANDLES[0].closed


@pytest.mark.asyncio
@pytest.mark.parametrize("mode", ["direct", "conditional"])
async def test_initializer_accepts_direct_tool_and_conditional_cleanup(
    packages, tmp_path, mode
):
    add, roots = packages
    name, _ = add("tool-admission-conditional")
    async with _runtime(roots) as (coordinator, loader):
        mount = await loader.load(
            "tool-admission-conditional", _config(tmp_path, mode=mode)
        )
        assert await _mount_tool(
            loader, "tool-admission-conditional", mount, coordinator
        ) is None
        module = sys.modules[name]
        assert coordinator._cleanup_fns == [module.CLEANUPS[0]]
        assert coordinator.get("tools") == {}
        assert module.CLOSED == []
    assert module.CLOSED == ["runtime-tool"]
    assert module.HANDLES[0].closed


@pytest.mark.asyncio
async def test_singleton_remount_with_none_keeps_the_accepted_instance(
    packages, tmp_path, monkeypatch
):
    add, roots = packages
    name, _ = add("tool-admission-singleton")
    checked = []
    original = ToolValidator._check_tool_methods

    def observe(self, result, tool):
        checked.append(tool)
        return original(self, result, tool)

    monkeypatch.setattr(ToolValidator, "_check_tool_methods", observe)
    async with _runtime(roots) as (coordinator, loader):
        mount = await loader.load(
            "tool-admission-singleton", _config(tmp_path, mode="singleton")
        )
        module = sys.modules[name]
        assert await _mount_tool(
            loader, "tool-admission-singleton", mount, coordinator
        ) is None
        assert checked == [module.SINGLETON]
        assert await _mount_tool(
            loader, "tool-admission-singleton", mount, coordinator
        ) is None
        assert checked == [module.SINGLETON]
        assert list(coordinator.get("tools")) == ["singleton"]
        assert coordinator.get("tools")["singleton"] is module.SINGLETON


@pytest.mark.asyncio
@pytest.mark.parametrize("mode", ["valid", "empty"])
async def test_standalone_diagnostic_still_checks_and_cleans_empty_mounts(
    packages, tmp_path, mode
):
    add, _ = packages
    name, package = add("tool-admission-diagnostic")
    result = await ToolValidator().validate(package, config=_config(tmp_path, mode=mode))
    module = sys.modules[name]
    assert result.passed is (mode == "valid")
    if mode == "empty":
        assert any(check.name == "protocol_compliance" for check in result.errors)
    assert len(module.HANDLES) == 1
    assert module.CLOSED == ["runtime-tool"]
    assert module.HANDLES[0].closed


@pytest.mark.asyncio
@pytest.mark.parametrize("value", [False, 0, {}])
async def test_falsey_unsupported_return_is_not_graceful_none(packages, tmp_path, value):
    add, roots = packages
    name, _ = add("tool-admission-falsey")
    async with _runtime(roots) as (coordinator, loader):
        mount = await loader.load(
            "tool-admission-falsey",
            _config(tmp_path, mode="unsupported", return_value=value),
        )
        with pytest.raises(ModuleValidationError, match="No tool was mounted"):
            await _mount_tool(loader, "tool-admission-falsey", mount, coordinator)
        module = sys.modules[name]
        assert not module.HANDLES[0].closed
        assert coordinator.get("tools") == {}
    assert module.HANDLES[0].closed


@pytest.mark.asyncio
async def test_inspection_cancellation_retains_returned_cleanup(
    packages, tmp_path, monkeypatch
):
    add, roots = packages
    name, _ = add("tool-admission-inspection-cancel")
    error = asyncio.CancelledError("inspection-stop")

    def interrupt(self, result, tool):
        assert coordinator._cleanup_fns == [sys.modules[name].CLEANUPS[0]]
        raise error

    monkeypatch.setattr(ToolValidator, "_check_tool_methods", interrupt)
    async with _runtime(roots) as (coordinator, loader):
        mount = await loader.load("tool-admission-inspection-cancel", _config(tmp_path))
        with pytest.raises(asyncio.CancelledError) as failure:
            await _mount_tool(loader, "tool-admission-inspection-cancel", mount, coordinator)
        assert failure.value is error
        module = sys.modules[name]
        assert len(module.HANDLES) == 1 and not module.HANDLES[0].closed
        assert coordinator.get("tools") == {}
    assert module.HANDLES[0].closed
