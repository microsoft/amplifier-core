"""
Exportable structural test base class for orchestrator modules.

Modules inherit from OrchestratorStructuralTests to run standard structural validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.structural import OrchestratorStructuralTests

    class TestMyOrchestratorStructural(OrchestratorStructuralTests):
        pass  # Inherits all standard structural tests
"""

import pytest


class OrchestratorStructuralTests:
    """Authoritative structural tests for orchestrator modules.

    Modules inherit this class to run standard structural validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_structural_validation(self, module_path):
        """Module must pass all structural validation checks."""
        if module_path is None:
            pytest.skip("No module path detected")

        from amplifier_core.validation import OrchestratorValidator

        validator = OrchestratorValidator()
        result = await validator.validate(module_path)

        if not result.passed:
            errors = "\n".join(f"  - {c.name}: {c.message}" for c in result.errors)
            pytest.fail(f"Structural validation failed:\n{errors}")
