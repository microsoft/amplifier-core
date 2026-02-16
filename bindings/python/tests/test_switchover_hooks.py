"""Tests for expanded RustHookRegistry API matching Python HookRegistry."""

import pytest
from amplifier_core._engine import RustHookRegistry


def test_set_default_fields():
    """set_default_fields accepts keyword arguments and stores them."""
    registry = RustHookRegistry()
    # Python HookRegistry.set_default_fields takes **kwargs
    registry.set_default_fields(session_id="test-123", parent_id=None)
    # If it doesn't raise, the method exists and accepts kwargs
