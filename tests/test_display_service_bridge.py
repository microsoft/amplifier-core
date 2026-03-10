"""Tests for PyDisplayServiceBridge — verifies that setting coordinator.display_system
creates a Rust-side DisplayService bridge and is reflected in to_dict()."""

import pytest


def test_display_system_sets_has_display_service():
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")
    coord = RustCoordinator()
    d = coord.to_dict()
    assert d.get("has_display_service") is False or d.get("has_display_service") is None

    class FakeDisplay:
        def __init__(self):
            self.messages = []

        def show_message(self, message, level, source):
            self.messages.append((message, level, source))

    coord.display_system = FakeDisplay()
    d = coord.to_dict()
    assert d.get("has_display_service") is True
