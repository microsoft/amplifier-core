"""Provider mount failures preserve sibling accounts before host policy runs."""

import copy
from unittest.mock import AsyncMock, MagicMock

import pytest
from amplifier_core import AmplifierSession
from amplifier_core._session_init import initialize_session
from amplifier_core.loader import ModuleLoader
from amplifier_core.testing import MockCoordinator


@pytest.fixture(params=["mock", "native"])
def coordinator(request):
    if request.param == "mock":
        return MockCoordinator()
    # Exercise the installed native mount table and capability registry too.
    return AmplifierSession(
        {"session": {"orchestrator": "test", "context": "test"}}
    ).coordinator


def loader_for(coordinator, provider_mount):
    async def load(module_id, config=None, **kwargs):
        if module_id.startswith("provider-"):
            return provider_mount

        async def empty_mount(c):
            return None

        return empty_mount

    loader = MagicMock(spec=ModuleLoader)
    loader.load = AsyncMock(side_effect=load)
    loader.get_on_session_ready_queue.return_value = []
    coordinator.loader = loader
    return loader


@pytest.mark.asyncio
async def test_partial_mount_restored_before_instance_policy_and_event(coordinator):
    original = object()
    await coordinator.mount("providers", original, name="openai")
    observed = []

    async def policy(c, spec, error):
        assert c.get("providers") == {"openai": original}
        assert isinstance(error, ValueError)
        observed.append(copy.deepcopy(spec))
        # Mutating policy input must not mutate the caller's config.
        spec["config"]["default_model"] = "changed by policy"
        await c.mount("providers", "unavailable", name=spec["instance_id"])

    coordinator.register_capability("provider.load_failure", policy)

    async def mount(c):
        await c.mount("providers", object(), name="openai")
        await c.mount("providers", object(), name="leaked")
        raise ValueError("synthetic mount failed")

    mount.__on_session_ready__ = ("provider-openai", AsyncMock())
    loader = loader_for(coordinator, mount)
    events = []

    async def capture(event, data):
        events.append(dict(data))
        assert coordinator.get("providers") == {"openai": original}

    coordinator.hooks.register("module:load_failed", capture, 0, name="capture")
    config = {
        "providers": [
            {
                "module": "provider-openai",
                "instance_id": "broken-account",
                "source": "synthetic",
                "config": {"default_model": "kept", "priority": 7},
            }
        ]
    }
    before = copy.deepcopy(config)
    await initialize_session(config, coordinator, "test", None)
    assert config == before
    assert coordinator.get("providers") == {
        "openai": original,
        "broken-account": "unavailable",
    }
    assert events[0]["instance_id"] == "broken-account"
    assert observed == config["providers"]
    loader.enqueue_on_session_ready.assert_not_called()


@pytest.mark.asyncio
async def test_host_failure_policy_can_abort_without_observability_swallowing_it(
    coordinator,
):
    async def reject(*args):
        raise LookupError("host chose to stop")

    coordinator.register_capability("provider.load_failure", reject)

    async def fail(c):
        raise ValueError("failed mount")

    loader_for(coordinator, fail)
    with pytest.raises(LookupError, match="host chose to stop"):
        await initialize_session(
            {"providers": [{"module": "provider-broken"}]}, coordinator, "test", None
        )


@pytest.mark.asyncio
async def test_no_policy_restores_partial_mount_and_continues_to_healthy(coordinator):
    original = object()
    healthy = object()
    await coordinator.mount("providers", original, name="openai")
    calls = 0

    async def mount(c):
        nonlocal calls
        calls += 1
        await c.mount("providers", healthy, name="openai")
        if calls == 1:
            raise RuntimeError("first account failed")

    loader_for(coordinator, mount)
    await initialize_session(
        {
            "providers": [
                {"module": "provider-openai", "instance_id": "broken"},
                {"module": "provider-openai", "instance_id": "healthy"},
            ]
        },
        coordinator,
        "test",
        None,
    )
    assert calls == 2
    assert coordinator.get("providers") == {"openai": original, "healthy": healthy}


@pytest.mark.asyncio
async def test_failed_attempt_removing_default_preserves_original_order(coordinator):
    first, second = object(), object()
    await coordinator.mount("providers", first, name="first")
    await coordinator.mount("providers", second, name="second")
    before = list(coordinator.get("providers"))

    async def mount(c):
        await c.unmount("providers", name=before[0])
        raise ValueError("removed a sibling before failing")

    loader_for(coordinator, mount)
    await initialize_session(
        {"providers": [{"module": "provider-broken"}]}, coordinator, "test", None
    )
    assert list(coordinator.get("providers")) == before
    assert coordinator.get("providers") == {"first": first, "second": second}


@pytest.mark.asyncio
async def test_import_failure_reports_exact_entry_and_skips_readiness(coordinator):
    policy = AsyncMock()
    coordinator.register_capability("provider.load_failure", policy)
    loader = loader_for(coordinator, AsyncMock())
    original_load = loader.load.side_effect
    error = ImportError("synthetic import failure")

    async def fail_load(module_id, *args, **kwargs):
        if module_id == "provider-missing":
            raise error
        return await original_load(module_id, *args, **kwargs)

    loader.load.side_effect = fail_load
    spec = {
        "module": "provider-missing",
        "instance_id": "account",
        "config": {"priority": 1},
    }
    await initialize_session({"providers": [spec]}, coordinator, "test", None)
    policy.assert_awaited_once_with(coordinator, spec, error)
    loader.enqueue_on_session_ready.assert_not_called()


@pytest.mark.asyncio
async def test_remap_failure_restores_default_and_existing_named_slot():
    c = MockCoordinator()
    original, named = object(), object()
    await c.mount("providers", original, name="openai")
    await c.mount("providers", named, name="account")
    real_mount = c.mount
    failures = []

    async def broken_mount(point, instance, name=None):
        if name == "account" and instance is not named:
            # Even a remap that mutates and then fails must be restored.
            await real_mount(point, instance, name=name)
            raise ValueError("remap failure")
        await real_mount(point, instance, name=name)

    c.mount = broken_mount

    async def mount(coord):
        await coord.mount("providers", object(), name="openai")

    mount.__on_session_ready__ = ("provider-openai", AsyncMock())
    loader = loader_for(c, mount)

    async def policy(coord, spec, error):
        failures.append(spec["instance_id"])
        assert coord.get("providers") == {"openai": original, "account": named}

    c.register_capability("provider.load_failure", policy)
    await initialize_session(
        {"providers": [{"module": "provider-openai", "instance_id": "account"}]},
        c,
        "test",
        None,
    )
    assert failures == ["account"]
    loader.enqueue_on_session_ready.assert_not_called()
