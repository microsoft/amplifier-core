"""Test that setting approval_system on the coordinator sets has_approval_provider."""

import pytest


def test_approval_system_sets_has_approval_provider():
    """Setting coordinator.approval_system should set has_approval_provider in to_dict."""
    try:
        from amplifier_core._engine import RustCoordinator
    except ImportError:
        pytest.skip("Rust engine not available")

    coord = RustCoordinator()

    # Initially no approval provider
    d = coord.to_dict()
    assert (
        d.get("has_approval_provider") is False
        or d.get("has_approval_provider") is None
    )

    # Set a simple approval system
    class FakeApproval:
        def request_approval(self, prompt, options, timeout, default):
            return "approve"

    coord.approval_system = FakeApproval()
    d = coord.to_dict()
    assert d.get("has_approval_provider") is True
