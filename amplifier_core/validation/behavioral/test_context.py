"""
Exportable behavioral test base class for context manager modules.

Modules inherit from ContextBehaviorTests to run standard contract validation.
All test methods use fixtures from the pytest plugin.

Usage in module:
    from amplifier_core.validation.behavioral import ContextBehaviorTests

    class TestMyContextBehavior(ContextBehaviorTests):
        pass  # Inherits all standard tests
"""

import asyncio

import pytest


class ContextBehaviorTests:
    """Authoritative behavioral tests for context manager modules.

    Modules inherit this class to run standard contract validation.
    All test methods use fixtures provided by the amplifier-core pytest plugin.
    """

    @pytest.mark.asyncio
    async def test_mount_succeeds(self, context_module):
        """mount() must succeed and return a context manager instance."""
        assert context_module is not None

    @pytest.mark.asyncio
    async def test_context_has_required_methods(self, context_module):
        """Context manager must have required methods."""
        required_methods = ["add_message", "get_messages", "clear"]

        for method in required_methods:
            assert hasattr(context_module, method), f"Context must have {method} method"
            assert callable(getattr(context_module, method)), f"{method} must be callable"

    @pytest.mark.asyncio
    async def test_message_round_trip(self, context_module):
        """Messages added can be retrieved."""
        message = {"role": "user", "content": "Hello"}
        await context_module.add_message(message)

        messages = await context_module.get_messages()

        assert len(messages) >= 1, "Should have at least one message"
        # Find our message
        user_messages = [m for m in messages if m.get("content") == "Hello"]
        assert len(user_messages) >= 1, "Our message should be retrievable"

    @pytest.mark.asyncio
    async def test_multiple_messages(self, context_module):
        """Multiple messages can be added and retrieved."""
        messages_to_add = [
            {"role": "user", "content": "First"},
            {"role": "assistant", "content": "Response"},
            {"role": "user", "content": "Second"},
        ]

        for msg in messages_to_add:
            await context_module.add_message(msg)

        retrieved = await context_module.get_messages()

        # Should have at least our 3 messages
        assert len(retrieved) >= 3, "Should have at least 3 messages"

    @pytest.mark.asyncio
    async def test_clear_removes_messages(self, context_module):
        """clear() must remove all messages."""
        # Add a message first
        await context_module.add_message({"role": "user", "content": "Test"})

        # Clear
        await context_module.clear()

        # Should be empty
        messages = await context_module.get_messages()
        assert len(messages) == 0, "clear() should remove all messages"

    @pytest.mark.asyncio
    async def test_get_messages_for_request_returns_messages(self, context_module):
        """get_messages_for_request() must return messages ready for LLM."""
        # Add a test message first
        await context_module.add_message({"role": "user", "content": "Test"})

        if hasattr(context_module, "get_messages_for_request"):
            messages = await context_module.get_messages_for_request()
            assert isinstance(messages, list), "get_messages_for_request() must return list"
            assert len(messages) >= 1, "Should return added messages"

    @pytest.mark.asyncio
    async def test_internal_should_compact_returns_bool(self, context_module):
        """_should_compact() must return boolean if present (internal method)."""
        # Note: _should_compact is an internal method, not called by orchestrators
        # It may be sync or async depending on implementation
        # Method signature varies: some take no args, some take (token_count, budget)
        import inspect

        if hasattr(context_module, "_should_compact"):
            method = context_module._should_compact

            # Determine required arguments (excluding self)
            sig = inspect.signature(method)
            required_params = [
                p for p in sig.parameters.values()
                if p.default is inspect.Parameter.empty
                and p.kind in (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD)
            ]

            # Prepare args based on signature
            if len(required_params) == 0:
                args = ()
            elif len(required_params) == 2:
                # Likely (token_count, budget) signature
                args = (100_000, 200_000)  # token_count, budget
            else:
                # Unknown signature, skip test
                pytest.skip(f"_should_compact has unexpected signature: {sig}")
                return

            if asyncio.iscoroutinefunction(method):
                result = await method(*args)
            else:
                result = method(*args)
            assert isinstance(result, bool), "_should_compact() must return bool"

    @pytest.mark.asyncio
    async def test_internal_compact_does_not_crash(self, context_module):
        """_compact_internal() must not crash if present (internal method)."""
        # Note: _compact_internal is an internal method, not called by orchestrators
        # It may be sync or async depending on implementation
        if hasattr(context_module, "_compact_internal"):
            try:
                method = context_module._compact_internal
                if asyncio.iscoroutinefunction(method):
                    await method()
                else:
                    method()
            except Exception as e:
                # Should not crash with code errors
                assert not isinstance(e, AttributeError | TypeError), f"_compact_internal() crashed: {e}"

    @pytest.mark.asyncio
    async def test_add_invalid_message_does_not_crash(self, context_module):
        """Adding invalid message should not crash."""
        try:
            # Empty message
            await context_module.add_message({})
        except Exception as e:
            # Should be validation error, not code bug
            assert not isinstance(e, AttributeError | TypeError), f"add_message crashed: {e}"

    @pytest.mark.asyncio
    async def test_get_messages_never_returns_none(self, context_module):
        """get_messages() should return list, not None."""
        messages = await context_module.get_messages()
        assert messages is not None, "get_messages() must not return None"
        assert isinstance(messages, list), "get_messages() must return list"
