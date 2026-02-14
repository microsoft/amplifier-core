"""
Exportable structural test base class for context modules.

Modules inherit from ContextStructuralTests to run standard structural validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.structural import ContextStructuralTests

    class TestMyContextStructural(ContextStructuralTests):
        pass  # Inherits all standard structural tests
"""

import pytest


class ContextStructuralTests:
    """Authoritative structural tests for context modules.

    Modules inherit this class to run standard structural validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_structural_validation(self, module_path):
        """Module must pass all structural validation checks."""
        if module_path is None:
            pytest.skip("No module path detected")

        from amplifier_core.validation import ContextValidator

        validator = ContextValidator()
        result = await validator.validate(module_path)

        if not result.passed:
            errors = "\n".join(f"  - {c.name}: {c.message}" for c in result.errors)
            pytest.fail(f"Structural validation failed:\n{errors}")
