"""
Exportable behavioral test base class for provider modules.

Modules inherit from ProviderBehaviorTests to run standard contract validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.behavioral import ProviderBehaviorTests

    class TestMyProviderBehavior(ProviderBehaviorTests):
        pass  # Inherits all standard tests
"""

import pytest

from amplifier_core.models import ProviderInfo


class ProviderBehaviorTests:
    """Authoritative behavioral tests for provider modules.

    Modules inherit this class to run standard contract validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_mount_succeeds(self, provider_module):
        """mount() must succeed and return a provider instance."""
        assert provider_module is not None

    @pytest.mark.asyncio
    async def test_get_info_returns_valid_provider_info(self, provider_module):
        """get_info() must return ProviderInfo with required fields."""
        info = provider_module.get_info()

        assert isinstance(info, ProviderInfo), "get_info() must return ProviderInfo"
        assert info.id, "ProviderInfo must have id"
        assert info.display_name, "ProviderInfo must have display_name"

    @pytest.mark.asyncio
    async def test_list_models_returns_list(self, provider_module):
        """list_models() must return a list."""
        models = await provider_module.list_models()

        assert isinstance(models, list), "list_models() must return a list"

    @pytest.mark.asyncio
    async def test_provider_has_name_attribute(self, provider_module):
        """Provider must have a name attribute."""
        assert hasattr(provider_module, "name"), "Provider must have name attribute"
        assert provider_module.name, "Provider name must not be empty"
        assert isinstance(provider_module.name, str), "Provider name must be string"

    @pytest.mark.asyncio
    async def test_parse_tool_calls_returns_list(self, provider_module):
        """parse_tool_calls() must return a list (possibly empty)."""
        from amplifier_core.message_models import ChatResponse
        from amplifier_core.message_models import TextBlock

        # Create a mock response without tool calls
        mock_response = ChatResponse(content=[TextBlock(text="Hello")])

        calls = provider_module.parse_tool_calls(mock_response)

        assert isinstance(calls, list), "parse_tool_calls() must return a list"
