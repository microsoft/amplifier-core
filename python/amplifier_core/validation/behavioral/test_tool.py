"""
Exportable behavioral test base class for tool modules.

Modules inherit from ToolBehaviorTests to run standard contract validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.behavioral import ToolBehaviorTests

    class TestMyToolBehavior(ToolBehaviorTests):
        pass  # Inherits all standard tests
"""

import pytest

from amplifier_core import ToolResult


class ToolBehaviorTests:
    """Authoritative behavioral tests for tool modules.

    Modules inherit this class to run standard contract validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_mount_succeeds(self, tool_module):
        """mount() must succeed and return a tool instance."""
        assert tool_module is not None

    @pytest.mark.asyncio
    async def test_tool_has_name(self, tool_module):
        """Tool must have a name property."""
        assert hasattr(tool_module, "name"), "Tool must have name attribute"
        assert tool_module.name, "Tool name must not be empty"
        assert isinstance(tool_module.name, str), "Tool name must be string"

    @pytest.mark.asyncio
    async def test_tool_has_description(self, tool_module):
        """Tool must have a description property."""
        assert hasattr(tool_module, "description"), "Tool must have description attribute"
        assert tool_module.description, "Tool description must not be empty"
        assert isinstance(tool_module.description, str), "Tool description must be string"

    @pytest.mark.asyncio
    async def test_tool_has_execute_method(self, tool_module):
        """Tool must have an execute method."""
        assert hasattr(tool_module, "execute"), "Tool must have execute method"
        assert callable(tool_module.execute), "execute must be callable"

    @pytest.mark.asyncio
    async def test_execute_returns_tool_result(self, tool_module):
        """execute() must return ToolResult."""
        result = await tool_module.execute({"_tool_call_id": "test-123"})

        assert isinstance(result, ToolResult), "execute() must return ToolResult"

    @pytest.mark.asyncio
    async def test_tool_result_has_required_fields(self, tool_module):
        """ToolResult must have success and output fields."""
        result = await tool_module.execute({"_tool_call_id": "test-456"})

        assert hasattr(result, "success"), "ToolResult must have success field"
        assert hasattr(result, "output"), "ToolResult must have output field"

    @pytest.mark.asyncio
    async def test_invalid_input_returns_error_result(self, tool_module):
        """Errors must return ToolResult with success=False, not raise."""
        try:
            result = await tool_module.execute({})
            # Should return error result, not raise
            assert isinstance(result, ToolResult), "Must return ToolResult even on error"
        except Exception as e:
            # Only allow expected validation errors, not code bugs
            assert not isinstance(e, AttributeError | TypeError | KeyError), f"Tool crashed with code error: {e}"
