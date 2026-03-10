"""Tests for PyWasmProvider (WasmProvider) Rust binding.

Verifies that WasmProvider is exported from _engine and has the expected
interface: name property, get_info(), list_models(), complete(), parse_tool_calls(),
and __repr__.
"""


class TestWasmProviderExport:
    """WasmProvider must be importable from the Rust _engine module."""

    def test_wasm_provider_class_exists(self):
        """WasmProvider class must be exported from _engine."""
        from amplifier_core._engine import WasmProvider

        assert WasmProvider is not None

    def test_wasm_provider_has_name_property(self):
        """WasmProvider must expose a 'name' property."""
        from amplifier_core._engine import WasmProvider

        # name should be a defined descriptor (getter) on the class
        assert hasattr(WasmProvider, "name"), "WasmProvider missing 'name' property"

    def test_wasm_provider_has_get_info(self):
        """WasmProvider must have a get_info method."""
        from amplifier_core._engine import WasmProvider

        assert hasattr(WasmProvider, "get_info"), "WasmProvider missing 'get_info'"

    def test_wasm_provider_has_list_models(self):
        """WasmProvider must have a list_models method."""
        from amplifier_core._engine import WasmProvider

        assert hasattr(WasmProvider, "list_models"), (
            "WasmProvider missing 'list_models'"
        )

    def test_wasm_provider_has_complete(self):
        """WasmProvider must have a complete method."""
        from amplifier_core._engine import WasmProvider

        assert hasattr(WasmProvider, "complete"), "WasmProvider missing 'complete'"

    def test_wasm_provider_has_parse_tool_calls(self):
        """WasmProvider must have a parse_tool_calls method."""
        from amplifier_core._engine import WasmProvider

        assert hasattr(WasmProvider, "parse_tool_calls"), (
            "WasmProvider missing 'parse_tool_calls'"
        )

    def test_wasm_provider_has_repr(self):
        """WasmProvider must have a __repr__ method."""
        from amplifier_core._engine import WasmProvider

        assert hasattr(WasmProvider, "__repr__"), "WasmProvider missing '__repr__'"
