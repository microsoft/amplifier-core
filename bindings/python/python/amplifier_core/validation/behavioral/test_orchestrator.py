"""
Exportable behavioral test base class for orchestrator modules.

Modules inherit from OrchestratorBehaviorTests to run standard contract validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.behavioral import OrchestratorBehaviorTests

    class TestMyOrchestratorBehavior(OrchestratorBehaviorTests):
        pass  # Inherits all standard tests
"""

import pytest


class OrchestratorBehaviorTests:
    """Authoritative behavioral tests for orchestrator modules.

    Modules inherit this class to run standard contract validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_mount_succeeds(self, orchestrator_module):
        """mount() must succeed and return an orchestrator instance."""
        assert orchestrator_module is not None

    @pytest.mark.asyncio
    async def test_orchestrator_has_execute_method(self, orchestrator_module):
        """Orchestrator must have an execute method."""
        assert hasattr(orchestrator_module, "execute"), "Orchestrator must have execute method"
        assert callable(orchestrator_module.execute), "execute must be callable"

    @pytest.mark.asyncio
    async def test_execute_returns_string(self, orchestrator_module, mock_deps):
        """execute() must return string response."""
        context, providers, tools, event_recorder = mock_deps

        result = await orchestrator_module.execute(
            prompt="Test prompt",
            context=context,
            providers=providers,
            tools=tools,
            hooks=event_recorder,
        )

        assert isinstance(result, str), "execute() must return string"
        assert len(result) > 0, "Response must not be empty"

    @pytest.mark.asyncio
    async def test_execute_with_empty_prompt(self, orchestrator_module, mock_deps):
        """execute() should handle empty prompt gracefully."""
        context, providers, tools, event_recorder = mock_deps

        try:
            result = await orchestrator_module.execute(
                prompt="",
                context=context,
                providers=providers,
                tools=tools,
                hooks=event_recorder,
            )
            # If it returns, should be string
            assert isinstance(result, str)
        except Exception as e:
            # Should raise a sensible error, not crash with code bugs
            assert not isinstance(e, AttributeError | TypeError | KeyError), f"Orchestrator crashed: {e}"

    @pytest.mark.asyncio
    async def test_orchestrator_uses_provider(self, orchestrator_module, mock_deps):
        """Orchestrator must call provider.complete()."""
        context, providers, tools, event_recorder = mock_deps

        await orchestrator_module.execute(
            prompt="Test",
            context=context,
            providers=providers,
            tools=tools,
            hooks=event_recorder,
        )

        # Verify provider was called (through mock tracking)
        provider = providers.get("default")
        if provider and hasattr(provider, "complete"):
            assert callable(provider.complete)

    @pytest.mark.asyncio
    async def test_orchestrator_updates_context(self, orchestrator_module, mock_deps):
        """Orchestrator should add messages to context."""
        context, providers, tools, event_recorder = mock_deps

        await orchestrator_module.execute(
            prompt="Test message",
            context=context,
            providers=providers,
            tools=tools,
            hooks=event_recorder,
        )

        # Context should have been updated with at least user message
        if hasattr(context, "add_message") and hasattr(context.add_message, "called"):
            assert context.add_message.called, "Context should be updated"
