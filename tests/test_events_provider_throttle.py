"""Tests for PROVIDER_THROTTLE event constant."""

from amplifier_core.events import ALL_EVENTS, PROVIDER_THROTTLE


class TestProviderThrottleEvent:
    """Tests for the PROVIDER_THROTTLE event constant."""

    def test_value(self) -> None:
        assert PROVIDER_THROTTLE == "provider:throttle"

    def test_in_all_events(self) -> None:
        assert PROVIDER_THROTTLE in ALL_EVENTS
