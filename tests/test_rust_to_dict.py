"""Tests for RustCoordinator.to_dict() — audit finding #1.

Addresses production audit finding: vars(coordinator) returns only the Python
__dict__, missing all Rust-managed state. to_dict() exposes the full state.
"""

from amplifier_core._engine import RustCoordinator


class TestRustToDict:
    """Verify RustCoordinator.to_dict() exposes Rust-managed state."""

    def test_coordinator_has_to_dict(self):
        """RustCoordinator instances must have a to_dict method."""
        coord = RustCoordinator()
        assert hasattr(coord, "to_dict"), "RustCoordinator missing to_dict()"

    def test_coordinator_to_dict_returns_dict(self):
        """to_dict() must return a plain Python dict."""
        coord = RustCoordinator()
        result = coord.to_dict()
        assert isinstance(result, dict), f"Expected dict, got {type(result)}"

    def test_coordinator_to_dict_includes_tools(self):
        """to_dict() must include a 'tools' key with a list value."""
        coord = RustCoordinator()
        result = coord.to_dict()
        assert "tools" in result, "to_dict() missing 'tools' key"
        assert isinstance(result["tools"], list), "tools should be a list"

    def test_coordinator_to_dict_includes_providers(self):
        """to_dict() must include a 'providers' key with a list value."""
        coord = RustCoordinator()
        result = coord.to_dict()
        assert "providers" in result, "to_dict() missing 'providers' key"
        assert isinstance(result["providers"], list), "providers should be a list"

    def test_coordinator_to_dict_includes_capabilities(self):
        """to_dict() must include a 'capabilities' key with a list value."""
        coord = RustCoordinator()
        result = coord.to_dict()
        assert "capabilities" in result, "to_dict() missing 'capabilities' key"
        assert isinstance(result["capabilities"], list), "capabilities should be a list"
