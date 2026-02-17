"""Tests for expanded RustCoordinator API matching Python ModuleCoordinator.

Milestone 2: Tasks 2.1 through 2.10.
"""

import pytest
from amplifier_core._engine import (
    RustCoordinator,
    RustHookRegistry,
    RustCancellationToken,
)


# ---- Helpers ----


class FakeSession:
    """Minimal session object for coordinator construction."""

    session_id = "test-session-123"
    parent_id = "parent-456"
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}


class FakeSessionNoParent:
    """Session without a parent_id."""

    session_id = "test-session-789"
    parent_id = None
    config = {"session": {"orchestrator": "loop-basic"}}


class FakeTool:
    name = "echo"
    description = "Echoes input"

    async def execute(self, input):
        return {"success": True, "output": str(input)}


class FakeProvider:
    name = "test-provider"
    description = "Test provider"


# ---- Task 2.1: mount_points dict ----


def test_coordinator_accepts_session():
    """Coordinator constructor accepts a session object."""
    coord = RustCoordinator(FakeSession())
    assert coord is not None


def test_mount_points_exists():
    """Coordinator has a mount_points dict attribute."""
    coord = RustCoordinator(FakeSession())
    assert hasattr(coord, "mount_points")
    mp = coord.mount_points
    assert isinstance(mp, dict)


def test_mount_points_has_expected_keys():
    """mount_points has all expected keys matching Python ModuleCoordinator."""
    coord = RustCoordinator(FakeSession())
    mp = coord.mount_points
    assert "orchestrator" in mp
    assert "providers" in mp
    assert "tools" in mp
    assert "context" in mp
    assert "hooks" in mp
    assert "module-source-resolver" in mp


def test_mount_points_initial_values():
    """mount_points has correct initial values."""
    coord = RustCoordinator(FakeSession())
    mp = coord.mount_points
    assert mp["orchestrator"] is None
    assert mp["context"] is None
    assert mp["module-source-resolver"] is None
    assert isinstance(mp["providers"], dict)
    assert isinstance(mp["tools"], dict)
    assert len(mp["providers"]) == 0
    assert len(mp["tools"]) == 0


def test_mount_points_hooks_is_registry():
    """mount_points['hooks'] is a RustHookRegistry instance."""
    coord = RustCoordinator(FakeSession())
    mp = coord.mount_points
    assert isinstance(mp["hooks"], RustHookRegistry)


def test_mount_points_hooks_is_same_as_hooks_property():
    """mount_points['hooks'] is the same object as coord.hooks."""
    coord = RustCoordinator(FakeSession())
    assert coord.mount_points["hooks"] is coord.hooks


def test_mount_points_is_mutable_dict():
    """mount_points dict can be modified directly (ecosystem compatibility)."""
    coord = RustCoordinator(FakeSession())
    coord.mount_points["tools"]["manual"] = lambda: "hi"
    assert "manual" in coord.mount_points["tools"]


# ---- Task 2.2: mount() and get() ----


@pytest.mark.asyncio
async def test_mount_tool():
    """mount() adds a module to mount_points['tools'] by name."""
    coord = RustCoordinator(FakeSession())
    tool = FakeTool()
    await coord.mount("tools", tool, name="echo")
    assert "echo" in coord.mount_points["tools"]
    assert coord.mount_points["tools"]["echo"] is tool


@pytest.mark.asyncio
async def test_mount_orchestrator():
    """mount() sets a single-slot module for orchestrator."""
    coord = RustCoordinator(FakeSession())
    orch = object()
    await coord.mount("orchestrator", orch)
    assert coord.mount_points["orchestrator"] is orch


@pytest.mark.asyncio
async def test_mount_context():
    """mount() sets a single-slot module for context."""
    coord = RustCoordinator(FakeSession())
    ctx = object()
    await coord.mount("context", ctx)
    assert coord.mount_points["context"] is ctx


@pytest.mark.asyncio
async def test_mount_tool_gets_name_from_module():
    """mount() auto-detects name from module.name attribute."""
    coord = RustCoordinator(FakeSession())
    tool = FakeTool()
    await coord.mount("tools", tool)  # No explicit name
    assert "echo" in coord.mount_points["tools"]


@pytest.mark.asyncio
async def test_mount_provider():
    """mount() adds provider by name."""
    coord = RustCoordinator(FakeSession())
    provider = FakeProvider()
    await coord.mount("providers", provider, name="test-provider")
    assert "test-provider" in coord.mount_points["providers"]


@pytest.mark.asyncio
async def test_mount_unknown_raises():
    """mount() raises ValueError for unknown mount points."""
    coord = RustCoordinator(FakeSession())
    with pytest.raises(ValueError, match="Unknown mount point"):
        await coord.mount("nonexistent", object())


@pytest.mark.asyncio
async def test_mount_hooks_raises():
    """mount() raises ValueError if you try to mount to 'hooks'."""
    coord = RustCoordinator(FakeSession())
    with pytest.raises(ValueError, match="Hooks should be registered"):
        await coord.mount("hooks", object())


@pytest.mark.asyncio
async def test_get_single_slot():
    """get() returns a single-slot module (orchestrator, context)."""
    coord = RustCoordinator(FakeSession())
    orch = object()
    await coord.mount("orchestrator", orch)
    assert coord.get("orchestrator") is orch


@pytest.mark.asyncio
async def test_get_multi_slot_all():
    """get() returns all modules at a multi-slot mount point."""
    coord = RustCoordinator(FakeSession())
    tool1 = FakeTool()
    await coord.mount("tools", tool1, name="echo")
    all_tools = coord.get("tools")
    assert isinstance(all_tools, dict)
    assert "echo" in all_tools


@pytest.mark.asyncio
async def test_get_multi_slot_by_name():
    """get(mount_point, name) returns a specific module."""
    coord = RustCoordinator(FakeSession())
    tool1 = FakeTool()
    await coord.mount("tools", tool1, name="echo")
    tool = coord.get("tools", "echo")
    assert tool is tool1


def test_get_hooks_returns_registry():
    """get('hooks') returns the HookRegistry."""
    coord = RustCoordinator(FakeSession())
    hooks = coord.get("hooks")
    assert hooks is not None
    assert isinstance(hooks, RustHookRegistry)


def test_get_missing_returns_none():
    """get() returns None for unset single-slot or missing named module."""
    coord = RustCoordinator(FakeSession())
    assert coord.get("orchestrator") is None
    assert coord.get("tools", "nonexistent") is None


def test_get_unknown_raises():
    """get() raises ValueError for unknown mount points."""
    coord = RustCoordinator(FakeSession())
    with pytest.raises(ValueError, match="Unknown mount point"):
        coord.get("nonexistent")


# ---- Task 2.3: unmount() ----


@pytest.mark.asyncio
async def test_unmount_single_slot():
    """unmount() clears a single-slot mount point."""
    coord = RustCoordinator(FakeSession())
    await coord.mount("orchestrator", object())
    assert coord.get("orchestrator") is not None
    await coord.unmount("orchestrator")
    assert coord.get("orchestrator") is None


@pytest.mark.asyncio
async def test_unmount_multi_slot():
    """unmount() removes a named module from a multi-slot mount point."""
    coord = RustCoordinator(FakeSession())
    await coord.mount("tools", FakeTool(), name="echo")
    assert coord.get("tools", "echo") is not None
    await coord.unmount("tools", "echo")
    assert coord.get("tools", "echo") is None


@pytest.mark.asyncio
async def test_unmount_unknown_raises():
    """unmount() raises ValueError for unknown mount points."""
    coord = RustCoordinator(FakeSession())
    with pytest.raises(ValueError, match="Unknown mount point"):
        await coord.unmount("nonexistent")


@pytest.mark.asyncio
async def test_unmount_multi_without_name_raises():
    """unmount() raises ValueError when name missing for multi-slot."""
    coord = RustCoordinator(FakeSession())
    with pytest.raises(ValueError, match="Name required"):
        await coord.unmount("tools")


# ---- Task 2.4: session_id, parent_id, session ----


def test_coordinator_session_id():
    """Coordinator session_id comes from the session object."""
    coord = RustCoordinator(FakeSession())
    assert coord.session_id == "test-session-123"


def test_coordinator_parent_id():
    """Coordinator parent_id comes from the session object."""
    coord = RustCoordinator(FakeSession())
    assert coord.parent_id == "parent-456"


def test_coordinator_parent_id_none():
    """Coordinator parent_id is None when session has no parent."""
    coord = RustCoordinator(FakeSessionNoParent())
    assert coord.parent_id is None


def test_coordinator_session_property():
    """Coordinator session property returns the session back-reference."""
    session = FakeSession()
    coord = RustCoordinator(session)
    assert coord.session is session


# ---- Task 2.5: register_capability / get_capability ----


def test_register_and_get_capability():
    """register_capability/get_capability round-trip."""
    coord = RustCoordinator(FakeSession())
    coord.register_capability("agents.list", lambda: ["agent1", "agent2"])
    cap = coord.get_capability("agents.list")
    assert cap is not None
    assert cap() == ["agent1", "agent2"]


def test_get_capability_missing():
    """get_capability returns None for unregistered capabilities."""
    coord = RustCoordinator(FakeSession())
    assert coord.get_capability("nonexistent") is None


def test_register_capability_overwrites():
    """register_capability overwrites existing capability."""
    coord = RustCoordinator(FakeSession())
    coord.register_capability("test", lambda: 1)
    coord.register_capability("test", lambda: 2)
    assert coord.get_capability("test")() == 2


# ---- Task 2.6: register_cleanup / cleanup ----


def test_register_cleanup():
    """register_cleanup stores a callable."""
    coord = RustCoordinator(FakeSession())
    called = []
    coord.register_cleanup(lambda: called.append(1))
    # Just verify it doesn't raise


@pytest.mark.asyncio
async def test_cleanup_runs_in_reverse():
    """cleanup() runs registered functions in reverse order."""
    coord = RustCoordinator(FakeSession())
    order = []
    coord.register_cleanup(lambda: order.append(1))
    coord.register_cleanup(lambda: order.append(2))
    coord.register_cleanup(lambda: order.append(3))
    await coord.cleanup()
    assert order == [3, 2, 1]


@pytest.mark.asyncio
async def test_cleanup_handles_errors():
    """cleanup() continues even if a cleanup function raises."""
    coord = RustCoordinator(FakeSession())
    order = []
    coord.register_cleanup(lambda: order.append(1))

    def bad_cleanup():
        raise RuntimeError("oops")

    coord.register_cleanup(bad_cleanup)
    coord.register_cleanup(lambda: order.append(3))
    await coord.cleanup()
    # 3 runs first (reverse), then bad_cleanup errors, then 1
    assert 3 in order
    assert 1 in order


# ---- Task 2.7: register_contributor / collect_contributions ----


def test_register_contributor():
    """register_contributor doesn't raise."""
    coord = RustCoordinator(FakeSession())
    coord.register_contributor("events", "mod-a", lambda: ["event1"])
    # Just verify it doesn't raise


@pytest.mark.asyncio
async def test_collect_contributions_basic():
    """collect_contributions returns results from registered contributors."""
    coord = RustCoordinator(FakeSession())
    coord.register_contributor("events", "mod-a", lambda: ["event1", "event2"])
    coord.register_contributor("events", "mod-b", lambda: ["event3"])
    results = await coord.collect_contributions("events")
    assert len(results) == 2
    assert ["event1", "event2"] in results
    assert ["event3"] in results


@pytest.mark.asyncio
async def test_collect_contributions_empty_channel():
    """collect_contributions returns empty list for unknown channels."""
    coord = RustCoordinator(FakeSession())
    results = await coord.collect_contributions("nonexistent")
    assert results == []


@pytest.mark.asyncio
async def test_collect_contributions_filters_none():
    """collect_contributions filters out None returns."""
    coord = RustCoordinator(FakeSession())
    coord.register_contributor("ch", "a", lambda: "data")
    coord.register_contributor("ch", "b", lambda: None)
    coord.register_contributor("ch", "c", lambda: "more")
    results = await coord.collect_contributions("ch")
    assert len(results) == 2
    assert "data" in results
    assert "more" in results


@pytest.mark.asyncio
async def test_collect_contributions_handles_errors():
    """collect_contributions catches errors in individual contributors."""
    coord = RustCoordinator(FakeSession())
    coord.register_contributor("ch", "good", lambda: "ok")

    def bad_contributor():
        raise RuntimeError("fail")

    coord.register_contributor("ch", "bad", bad_contributor)
    coord.register_contributor("ch", "also-good", lambda: "fine")
    results = await coord.collect_contributions("ch")
    # Should get results from good contributors, skipping the bad one
    assert "ok" in results
    assert "fine" in results
    assert len(results) == 2


@pytest.mark.asyncio
async def test_collect_contributions_async_callback():
    """collect_contributions handles async callbacks."""
    coord = RustCoordinator(FakeSession())

    async def async_contributor():
        return ["async-data"]

    coord.register_contributor("ch", "async-mod", async_contributor)
    results = await coord.collect_contributions("ch")
    assert len(results) == 1
    assert results[0] == ["async-data"]


# ---- Task 2.8: request_cancel / reset_turn ----


@pytest.mark.asyncio
async def test_request_cancel_graceful():
    """request_cancel() marks cancellation as graceful."""
    coord = RustCoordinator(FakeSession())
    await coord.request_cancel()
    assert coord.cancellation.is_cancelled


@pytest.mark.asyncio
async def test_request_cancel_immediate():
    """request_cancel(immediate=True) marks immediate cancellation."""
    coord = RustCoordinator(FakeSession())
    await coord.request_cancel(immediate=True)
    assert coord.cancellation.is_cancelled


def test_reset_turn():
    """reset_turn() resets per-turn tracking."""
    coord = RustCoordinator(FakeSession())
    coord.reset_turn()  # Should not raise


def test_reset_turn_resets_injection_count():
    """reset_turn() resets _current_turn_injections to 0."""
    coord = RustCoordinator(FakeSession())
    assert coord._current_turn_injections == 0
    coord._current_turn_injections = 5
    assert coord._current_turn_injections == 5
    coord.reset_turn()
    assert coord._current_turn_injections == 0


# ---- Task 2.9: injection_budget_per_turn / injection_size_limit ----


def test_injection_budget_per_turn_default_none():
    """injection_budget_per_turn returns None when not configured."""
    coord = RustCoordinator(FakeSession())
    assert coord.injection_budget_per_turn is None


def test_injection_size_limit_default_none():
    """injection_size_limit returns None when not configured."""
    coord = RustCoordinator(FakeSession())
    assert coord.injection_size_limit is None


def test_injection_budget_from_config():
    """injection_budget_per_turn reads from session config."""

    class ConfiguredSession:
        session_id = "s1"
        parent_id = None
        config = {
            "session": {
                "orchestrator": "loop-basic",
                "injection_budget_per_turn": 100,
            }
        }

    coord = RustCoordinator(ConfiguredSession())
    assert coord.injection_budget_per_turn == 100


def test_injection_size_limit_from_config():
    """injection_size_limit reads from session config."""

    class ConfiguredSession:
        session_id = "s1"
        parent_id = None
        config = {
            "session": {
                "orchestrator": "loop-basic",
                "injection_size_limit": 4000,
            }
        }

    coord = RustCoordinator(ConfiguredSession())
    assert coord.injection_size_limit == 4000


# ---- Task 2.10: loader, approval_system, display_system ----


def test_approval_system_default_none():
    """approval_system is None by default."""
    coord = RustCoordinator(FakeSession())
    assert coord.approval_system is None


def test_display_system_default_none():
    """display_system is None by default."""
    coord = RustCoordinator(FakeSession())
    assert coord.display_system is None


def test_loader_default_none():
    """loader is None by default."""
    coord = RustCoordinator(FakeSession())
    assert coord.loader is None


def test_approval_system_from_constructor():
    """approval_system can be passed in constructor."""
    approval = object()
    coord = RustCoordinator(FakeSession(), approval_system=approval)
    assert coord.approval_system is approval


def test_display_system_from_constructor():
    """display_system can be passed in constructor."""
    display = object()
    coord = RustCoordinator(FakeSession(), display_system=display)
    assert coord.display_system is display


def test_approval_system_settable():
    """approval_system can be set after construction."""
    coord = RustCoordinator(FakeSession())
    approval = object()
    coord.approval_system = approval
    assert coord.approval_system is approval


def test_display_system_settable():
    """display_system can be set after construction."""
    coord = RustCoordinator(FakeSession())
    display = object()
    coord.display_system = display
    assert coord.display_system is display


def test_loader_settable():
    """loader can be set after construction."""
    coord = RustCoordinator(FakeSession())
    loader = object()
    coord.loader = loader
    assert coord.loader is loader


# ---- Task 2.10 continued: channels attribute ----


def test_channels_attribute():
    """Coordinator has a channels dict attribute."""
    coord = RustCoordinator(FakeSession())
    assert hasattr(coord, "channels")
    assert isinstance(coord.channels, dict)


# ---- Task 2.10 continued: config property ----


def test_config_property():
    """Coordinator has a config property returning the session config."""
    coord = RustCoordinator(FakeSession())
    config = coord.config
    assert isinstance(config, dict)
    assert "session" in config
    assert config["session"]["orchestrator"] == "loop-basic"


# ---- Task 2.10 continued: cancellation property ----


def test_cancellation_property():
    """Coordinator has a cancellation property returning a CancellationToken."""
    coord = RustCoordinator(FakeSession())
    cancel = coord.cancellation
    assert isinstance(cancel, RustCancellationToken)
    assert cancel.is_cancelled is False
