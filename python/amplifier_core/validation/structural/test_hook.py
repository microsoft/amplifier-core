"""
Exportable structural test base class for hook modules.

Modules inherit from HookStructuralTests to run standard structural validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.structural import HookStructuralTests

    class TestMyHookStructural(HookStructuralTests):
        pass  # Inherits all standard structural tests
"""

import pytest


class HookStructuralTests:
    """Authoritative structural tests for hook modules.

    Modules inherit this class to run standard structural validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_structural_validation(self, module_path):
        """Module must pass all structural validation checks."""
        if module_path is None:
            pytest.skip("No module path detected")

        from amplifier_core.validation import HookValidator

        validator = HookValidator()
        result = await validator.validate(module_path)

        if not result.passed:
            errors = "\n".join(f"  - {c.name}: {c.message}" for c in result.errors)
            pytest.fail(f"Structural validation failed:\n{errors}")
