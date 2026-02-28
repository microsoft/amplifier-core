"""Tests for PyCancellationToken — verifies all 14 methods are exposed and work correctly.

These tests validate the PyO3 bindings for amplifier_core::CancellationToken.
"""

import asyncio

import pytest

from amplifier_core._engine import RustCancellationToken


# ---------------------------------------------------------------------------
# 1. All properties exist and have correct types
# ---------------------------------------------------------------------------


def test_cancellation_token_has_all_properties():
    """All 6 properties are accessible with correct types."""
    token = RustCancellationToken()

    # Existing
    assert isinstance(token.is_cancelled, bool)
    assert isinstance(token.state, str)

    # New properties
    assert isinstance(token.is_graceful, bool)
    assert isinstance(token.is_immediate, bool)
    assert isinstance(token.running_tools, set)
    assert isinstance(token.running_tool_names, list)


# ---------------------------------------------------------------------------
# 2. request_graceful
# ---------------------------------------------------------------------------


def test_cancellation_token_request_graceful():
    """request_graceful() returns bool and sets is_graceful."""
    token = RustCancellationToken()
    assert not token.is_graceful

    result = token.request_graceful()
    assert result is True
    assert token.is_graceful is True
    assert token.is_cancelled is True
    assert token.state == "graceful"

    # Second call returns False (already cancelled)
    result2 = token.request_graceful()
    assert result2 is False


# ---------------------------------------------------------------------------
# 3. request_immediate
# ---------------------------------------------------------------------------


def test_cancellation_token_request_immediate():
    """request_immediate() returns bool and sets is_immediate."""
    token = RustCancellationToken()
    assert not token.is_immediate

    result = token.request_immediate()
    assert result is True
    assert token.is_immediate is True
    assert token.is_cancelled is True
    assert token.state == "immediate"

    # Second call returns False (already immediate)
    result2 = token.request_immediate()
    assert result2 is False


def test_cancellation_token_graceful_then_immediate():
    """Graceful -> Immediate transition works."""
    token = RustCancellationToken()
    token.request_graceful()
    assert token.is_graceful
    assert not token.is_immediate

    result = token.request_immediate()
    assert result is True
    assert token.is_immediate is True
    assert not token.is_graceful


# ---------------------------------------------------------------------------
# 4. Tool tracking
# ---------------------------------------------------------------------------


def test_cancellation_token_tool_tracking():
    """register_tool_start/complete with running_tools/running_tool_names."""
    token = RustCancellationToken()

    assert token.running_tools == set()
    assert token.running_tool_names == []

    token.register_tool_start("tc_1", "bash")
    assert "tc_1" in token.running_tools
    assert "bash" in token.running_tool_names

    token.register_tool_start("tc_2", "python")
    assert len(token.running_tools) == 2

    token.register_tool_complete("tc_1")
    assert "tc_1" not in token.running_tools
    assert "bash" not in token.running_tool_names
    assert "tc_2" in token.running_tools

    token.register_tool_complete("tc_2")
    assert token.running_tools == set()
    assert token.running_tool_names == []


# ---------------------------------------------------------------------------
# 5. Reset
# ---------------------------------------------------------------------------


def test_cancellation_token_reset():
    """reset() clears state and running tools."""
    token = RustCancellationToken()
    token.request_graceful()
    token.register_tool_start("tc_1", "bash")

    assert token.is_cancelled
    assert len(token.running_tools) == 1

    token.reset()
    assert not token.is_cancelled
    assert token.state == "none"
    assert token.running_tools == set()
    assert token.running_tool_names == []


# ---------------------------------------------------------------------------
# 6. on_cancel + trigger_callbacks
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_cancellation_token_on_cancel():
    """on_cancel(callback) registers; trigger_callbacks() fires it."""
    token = RustCancellationToken()
    called = []

    async def my_callback():
        called.append(True)

    token.on_cancel(my_callback)
    token.request_graceful()
    await token.trigger_callbacks()

    assert len(called) == 1


# ---------------------------------------------------------------------------
# 7. Child registration
# ---------------------------------------------------------------------------


def test_cancellation_token_register_child():
    """register_child propagates cancellation; unregister_child stops it."""
    parent = RustCancellationToken()
    child = RustCancellationToken()

    parent.register_child(child)
    parent.request_graceful()
    assert child.is_graceful

    # Create another child, unregister, verify no propagation
    child2 = RustCancellationToken()
    parent.register_child(child2)
    parent.unregister_child(child2)
    parent.request_immediate()
    assert not child2.is_immediate  # Should not have propagated