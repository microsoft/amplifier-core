"""
Exportable structural test base class for tool modules.

Modules inherit from ToolStructuralTests to run standard structural validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.structural import ToolStructuralTests

    class TestMyToolStructural(ToolStructuralTests):
        pass  # Inherits all standard structural tests
"""

import pytest


class ToolStructuralTests:
    """Authoritative structural tests for tool modules.

    Modules inherit this class to run standard structural validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_tool_has_input_schema(self, tool_module):
        """Tool must have an input_schema property returning a dict."""
        assert hasattr(tool_module, "input_schema"), (
            "Tool must have input_schema attribute"
        )
        schema = tool_module.input_schema
        assert isinstance(schema, dict), "input_schema must return a dict"

    @pytest.mark.asyncio
    async def test_structural_validation(self, module_path):
        """Module must pass all structural validation checks."""
        if module_path is None:
            pytest.skip("No module path detected")

        from amplifier_core.validation import ToolValidator

        validator = ToolValidator()
        result = await validator.validate(module_path)

        if not result.passed:
            errors = "\n".join(f"  - {c.name}: {c.message}" for c in result.errors)
            pytest.fail(f"Structural validation failed:\n{errors}")
