"""Tests for thin Python re-export stubs (events.py, capabilities.py).

These verify that the Python modules re-export constants from the Rust _engine
module, maintaining backward-compatible import paths.
"""


def test_events_reexport_session_start():
    from amplifier_core.events import SESSION_START

    assert SESSION_START == "session:start"


def test_events_reexport_provider_throttle():
    from amplifier_core.events import PROVIDER_THROTTLE

    assert PROVIDER_THROTTLE == "provider:throttle"


def test_events_reexport_all_events():
    from amplifier_core.events import ALL_EVENTS

    assert len(ALL_EVENTS) == 51


def test_capabilities_reexport_tools():
    from amplifier_core.capabilities import TOOLS

    assert TOOLS == "tools"


def test_capabilities_reexport_all_well_known():
    from amplifier_core.capabilities import ALL_WELL_KNOWN_CAPABILITIES

    assert len(ALL_WELL_KNOWN_CAPABILITIES) == 16


def test_capabilities_reexport_cost_tiers():
    from amplifier_core.capabilities import COST_TIER_HIGH

    assert COST_TIER_HIGH == "high"


def test_capabilities_importable_from_init():
    from amplifier_core import capabilities

    assert hasattr(capabilities, "TOOLS")
