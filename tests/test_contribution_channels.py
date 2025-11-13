"""Tests for contribution channels mechanism."""

import pytest
from amplifier_core.coordinator import ModuleCoordinator


@pytest.fixture
def coordinator():
    """Create a minimal coordinator for testing (no session needed for these tests)."""

    # Create coordinator without full session infrastructure (not needed for channel tests)
    class MockSession:
        session_id = "test-session"

    mock_session = MockSession()
    return ModuleCoordinator(session=mock_session)  # type: ignore[arg-type]


@pytest.mark.asyncio
async def test_register_contributor(coordinator):
    """Test basic registration."""
    coordinator.register_contributor("test-channel", "test-module", lambda: ["item1", "item2"])

    assert "test-channel" in coordinator.channels
    assert len(coordinator.channels["test-channel"]) == 1
    assert coordinator.channels["test-channel"][0]["name"] == "test-module"


@pytest.mark.asyncio
async def test_collect_contributions(coordinator):
    """Test collection from multiple contributors."""
    coordinator.register_contributor("test", "mod1", lambda: ["a", "b"])
    coordinator.register_contributor("test", "mod2", lambda: ["c", "d"])

    contributions = await coordinator.collect_contributions("test")

    assert len(contributions) == 2
    assert ["a", "b"] in contributions
    assert ["c", "d"] in contributions


@pytest.mark.asyncio
async def test_contributor_failure_non_interference(coordinator):
    """Test that contributor failures don't crash collection."""

    def failing_contributor():
        raise RuntimeError("Test failure")

    coordinator.register_contributor("test", "good1", lambda: ["success1"])
    coordinator.register_contributor("test", "bad", failing_contributor)
    coordinator.register_contributor("test", "good2", lambda: ["success2"])

    contributions = await coordinator.collect_contributions("test")

    # Should get contributions from non-failing modules
    assert len(contributions) == 2
    assert ["success1"] in contributions
    assert ["success2"] in contributions


@pytest.mark.asyncio
async def test_empty_channel(coordinator):
    """Test collecting from non-existent channel."""
    contributions = await coordinator.collect_contributions("nonexistent")

    assert contributions == []


@pytest.mark.asyncio
async def test_none_filtering(coordinator):
    """Test that None contributions are filtered."""
    coordinator.register_contributor("test", "mod1", lambda: ["data"])
    coordinator.register_contributor("test", "mod2", lambda: None)  # Conditional skip
    coordinator.register_contributor("test", "mod3", lambda: ["more-data"])

    contributions = await coordinator.collect_contributions("test")

    assert len(contributions) == 2
    assert None not in contributions
    assert ["data"] in contributions
    assert ["more-data"] in contributions


@pytest.mark.asyncio
async def test_multiple_channels_independent(coordinator):
    """Test that channels are independent."""
    coordinator.register_contributor("channel1", "mod1", lambda: ["ch1-data"])
    coordinator.register_contributor("channel2", "mod2", lambda: ["ch2-data"])

    ch1_contributions = await coordinator.collect_contributions("channel1")
    ch2_contributions = await coordinator.collect_contributions("channel2")

    assert len(ch1_contributions) == 1
    assert len(ch2_contributions) == 1
    assert ["ch1-data"] in ch1_contributions
    assert ["ch2-data"] in ch2_contributions


@pytest.mark.asyncio
async def test_contribution_data_formats(coordinator):
    """Test that various data formats can be contributed."""
    coordinator.register_contributor("test", "list-contrib", lambda: ["a", "b"])
    coordinator.register_contributor("test", "dict-contrib", lambda: {"key": "value"})
    coordinator.register_contributor("test", "str-contrib", lambda: "single-string")
    coordinator.register_contributor("test", "int-contrib", lambda: 42)

    contributions = await coordinator.collect_contributions("test")

    assert len(contributions) == 4
    assert ["a", "b"] in contributions
    assert {"key": "value"} in contributions
    assert "single-string" in contributions
    assert 42 in contributions


@pytest.mark.asyncio
async def test_async_callback(coordinator):
    """Test that async callbacks work correctly."""

    async def async_contributor():
        # Simulate async work
        return ["async-data"]

    coordinator.register_contributor("test", "async-mod", async_contributor)

    contributions = await coordinator.collect_contributions("test")

    assert len(contributions) == 1
    assert ["async-data"] in contributions


@pytest.mark.asyncio
async def test_conditional_contribution(coordinator):
    """Test conditional contribution (returning None based on state)."""

    class ConditionalModule:
        def __init__(self):
            self.enabled = False

        def contribute(self):
            return ["data"] if self.enabled else None

    mod = ConditionalModule()
    coordinator.register_contributor("test", "conditional", mod.contribute)

    # First collection: disabled
    contributions = await coordinator.collect_contributions("test")
    assert contributions == []

    # Enable and collect again
    mod.enabled = True
    contributions = await coordinator.collect_contributions("test")
    assert len(contributions) == 1
    assert ["data"] in contributions


@pytest.mark.asyncio
async def test_observability_events_pattern(coordinator):
    """Test the observability.events pattern (real-world usage)."""
    # Simulate modules registering events
    coordinator.register_contributor(
        "observability.events", "tool-filesystem", lambda: ["filesystem:read", "filesystem:write", "filesystem:delete"]
    )
    coordinator.register_contributor(
        "observability.events", "tool-task", lambda: ["task:agent_spawned", "task:agent_completed"]
    )
    coordinator.register_contributor(
        "observability.events", "loop-streaming", lambda: ["session:start", "session:end", "context:pre_compact"]
    )

    # Consumer (hooks-logging) collects events
    discovered = await coordinator.collect_contributions("observability.events")

    assert len(discovered) == 3

    # Flatten list of lists
    all_events = []
    for event_list in discovered:
        if isinstance(event_list, list):
            all_events.extend(event_list)

    assert len(all_events) == 8
    assert "filesystem:read" in all_events
    assert "task:agent_spawned" in all_events
    assert "session:start" in all_events


@pytest.mark.asyncio
async def test_registration_order_preserved(coordinator):
    """Test that contributions are collected in registration order."""
    coordinator.register_contributor("test", "mod1", lambda: "first")
    coordinator.register_contributor("test", "mod2", lambda: "second")
    coordinator.register_contributor("test", "mod3", lambda: "third")

    contributions = await coordinator.collect_contributions("test")

    # Order should be preserved
    assert contributions == ["first", "second", "third"]
