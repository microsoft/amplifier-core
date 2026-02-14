"""Tests for PROVIDER_RETRY event constant."""

from amplifier_core.events import ALL_EVENTS, PROVIDER_RETRY


class TestProviderRetryEvent:
    """Tests for the PROVIDER_RETRY event constant."""

    def test_value(self) -> None:
        assert PROVIDER_RETRY == "provider:retry"

    def test_in_all_events(self) -> None:
        assert PROVIDER_RETRY in ALL_EVENTS
