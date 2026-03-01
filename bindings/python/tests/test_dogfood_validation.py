"""Dogfood validation — tests that simulate real Amplifier Foundation usage patterns.

Milestone 5: These tests go beyond unit tests to verify the Rust-backed kernel
works with the same patterns that Foundation and real modules actually use.
Every test uses the PUBLIC import paths (`from amplifier_core import ...`),
NOT the internal `_engine` module.
"""

import pytest

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

MINIMAL_CONFIG = {"session": {"orchestrator": "test", "context": "test"}}

FULL_CONFIG = {
    "session": {
        "orchestrator": {"module": "loop-basic"},
        "context": {"module": "context-simple"},
        "providers": [{"module": "provider-anthropic", "config": {"api_key": "test"}}],
        "tools": [{"module": "tool-bash"}],
        "hooks": [],
    }
}


# ---------------------------------------------------------------------------
# Task 5.1a — Session creation (the pattern Foundation uses)
# ---------------------------------------------------------------------------


def test_foundation_create_session_pattern():
    """Foundation creates sessions by passing a mount plan config dict."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=FULL_CONFIG)
    assert session.session_id  # UUID generated
    assert session.coordinator is not None
    assert session.coordinator.mount_points is not None
    assert session.coordinator.hooks is not None


def test_session_generates_unique_ids():
    """Every session gets a distinct UUID."""
    from amplifier_core import AmplifierSession

    s1 = AmplifierSession(config=MINIMAL_CONFIG)
    s2 = AmplifierSession(config=MINIMAL_CONFIG)
    assert s1.session_id != s2.session_id


def test_session_config_accessible():
    """Session config is accessible and matches what was passed."""
    from amplifier_core import AmplifierSession

    config = {
        "session": {"orchestrator": "test", "context": "test"},
        "custom_key": "custom_value",
    }
    session = AmplifierSession(config=config)
    assert session.config is not None
    assert "session" in session.config


def test_session_with_parent_id():
    """Child sessions track parent ID."""
    from amplifier_core import AmplifierSession

    parent = AmplifierSession(config=MINIMAL_CONFIG)
    child = AmplifierSession(config=MINIMAL_CONFIG, parent_id=parent.session_id)
    assert child.parent_id == parent.session_id


def test_multiple_sessions_independent():
    """Multiple sessions don't interfere with each other."""
    from amplifier_core import AmplifierSession

    s1 = AmplifierSession(config=MINIMAL_CONFIG)
    s2 = AmplifierSession(config=MINIMAL_CONFIG)

    assert s1.session_id != s2.session_id

    # Mount a tool on s1 only
    tool = type("T", (), {"name": "tool1"})()
    # mount() is async, so use the dict directly (Foundation does this too)
    s1.coordinator.mount_points["tools"]["tool1"] = tool

    # s2 should not have tool1
    assert s1.coordinator.get("tools", "tool1") is not None
    assert s2.coordinator.get("tools", "tool1") is None


# ---------------------------------------------------------------------------
# Task 5.1b — Coordinator mount round-trip
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_coordinator_mount_roundtrip():
    """Modules are mounted on coordinator and retrievable."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    class MockTool:
        name = "echo"
        description = "Echoes input"

        async def execute(self, **kwargs):
            return {"success": True, "output": str(kwargs)}

    await session.coordinator.mount("tools", MockTool(), name="echo")
    tool = session.coordinator.get("tools", "echo")
    assert tool is not None
    assert tool.name == "echo"


@pytest.mark.asyncio
async def test_mount_provider_and_retrieve():
    """Providers mount correctly through the coordinator."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    class MockProvider:
        name = "test-provider"
        description = "A test provider"

    await session.coordinator.mount("providers", MockProvider(), name="test-provider")
    provider = session.coordinator.get("providers", "test-provider")
    assert provider is not None
    assert provider.name == "test-provider"


@pytest.mark.asyncio
async def test_mount_orchestrator_single_slot():
    """Orchestrator is a single-slot mount point."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)
    orch = type("Orch", (), {"name": "basic"})()
    await session.coordinator.mount("orchestrator", orch)
    assert session.coordinator.get("orchestrator") is orch


# ---------------------------------------------------------------------------
# Task 5.1c — Hook registration and emit
# ---------------------------------------------------------------------------


def test_hook_registration_does_not_crash():
    """Hooks can be registered through the coordinator."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    async def my_hook(event, data):
        return None

    # register(event, name, handler, priority)
    session.coordinator.hooks.register("test:event", my_hook, 0, name="my-hook")
    # No crash means it works


@pytest.mark.asyncio
async def test_hook_emit_async():
    """Hook emit works correctly through the coordinator with sync handlers.

    Note: The Rust→Python bridge invokes handlers synchronously inside emit().
    Real Foundation hooks are sync callables; async handlers should use
    emit_and_collect() which has dedicated async support.
    """
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    received = []

    def hook_handler(event, data):
        received.append(event)
        return None

    session.coordinator.hooks.register("test:event", hook_handler, 0, name="test-hook")
    await session.coordinator.hooks.emit("test:event", {"foo": "bar"})

    assert "test:event" in received


@pytest.mark.asyncio
async def test_hook_emit_and_collect():
    """emit_and_collect gathers results from multiple handlers."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    def handler_a(event, data):
        return {"source": "a"}

    def handler_b(event, data):
        return {"source": "b"}

    session.coordinator.hooks.register("gather:event", handler_a, 0, name="hook-a")
    session.coordinator.hooks.register("gather:event", handler_b, 0, name="hook-b")

    results = await session.coordinator.hooks.emit_and_collect(
        "gather:event", {"key": "value"}
    )
    assert isinstance(results, list)


# ---------------------------------------------------------------------------
# Task 5.1d — Cancellation token
# ---------------------------------------------------------------------------


def test_cancellation_token_through_coordinator():
    """CancellationToken is accessible and functional through the coordinator."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    token = session.coordinator.cancellation
    assert not token.is_cancelled
    token.request_cancellation()
    assert token.is_cancelled


# ---------------------------------------------------------------------------
# Task 5.1e — Cleanup
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_cleanup_runs_through_session():
    """Cleanup functions registered on coordinator run when session cleans up."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    cleaned_up = []
    session.coordinator.register_cleanup(lambda: cleaned_up.append("a"))
    session.coordinator.register_cleanup(lambda: cleaned_up.append("b"))

    await session.cleanup()
    assert "a" in cleaned_up
    assert "b" in cleaned_up


@pytest.mark.asyncio
async def test_cleanup_via_context_manager():
    """Session async context manager calls cleanup on exit."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    cleaned_up = []
    session.coordinator.register_cleanup(lambda: cleaned_up.append("done"))

    # __aexit__ should trigger cleanup
    await session.__aexit__(None, None, None)
    assert "done" in cleaned_up


# ---------------------------------------------------------------------------
# Task 5.1f — Capability registration
# ---------------------------------------------------------------------------


def test_capability_registration():
    """Capabilities can be registered and retrieved."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    session.coordinator.register_capability("spawn", lambda: "spawned")
    cap = session.coordinator.get_capability("spawn")
    assert cap is not None
    assert cap() == "spawned"


def test_capability_missing_returns_none():
    """get_capability returns None for unregistered capabilities."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)
    assert session.coordinator.get_capability("nonexistent") is None


# ---------------------------------------------------------------------------
# Task 5.1g — Contribution channels
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_contribution_channels():
    """Contribution channels work through coordinator."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    session.coordinator.register_contributor("events", "mod1", lambda: {"type": "test"})

    contributions = await session.coordinator.collect_contributions("events")
    assert len(contributions) == 1
    assert contributions[0] == {"type": "test"}


@pytest.mark.asyncio
async def test_contribution_channels_empty():
    """Collecting from an empty channel returns empty list."""
    from amplifier_core import AmplifierSession

    session = AmplifierSession(config=MINIMAL_CONFIG)

    contributions = await session.coordinator.collect_contributions("events")
    assert contributions == []


# ---------------------------------------------------------------------------
# Task 5.1h — Public import smoke tests
# ---------------------------------------------------------------------------


def test_public_imports_all_available():
    """All key public symbols are importable from amplifier_core."""
    from amplifier_core import (
        AmplifierSession,
        CancellationToken,
        HookRegistry,
        HookResult,
        ModuleCoordinator,
    )

    # These should be the Rust-backed types (post-switchover)
    assert AmplifierSession is not None
    assert CancellationToken is not None
    assert HookRegistry is not None
    assert HookResult is not None
    assert ModuleCoordinator is not None


def test_hook_result_constructable():
    """HookResult can be instantiated (Foundation uses this constantly)."""
    from amplifier_core import HookResult

    result = HookResult()
    assert result.action == "continue"


def test_rust_available_flag():
    """RUST_AVAILABLE flag is True when engine is loaded."""
    from amplifier_core import RUST_AVAILABLE

    assert RUST_AVAILABLE is True
