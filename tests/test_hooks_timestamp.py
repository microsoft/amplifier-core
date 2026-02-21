"""
Tests for event timestamp stamping in HookRegistry.emit().

Verifies that emit() stamps a UTC ISO-8601 timestamp as an
infrastructure-owned field that callers cannot omit or override.
"""

from datetime import datetime

import pytest
from amplifier_core.hooks import HookRegistry
from amplifier_core.models import HookResult


@pytest.mark.asyncio
async def test_emit_stamps_timestamp():
    """emit() should stamp a valid ISO-8601 UTC timestamp on the event data."""
    registry = HookRegistry()
    captured = {}

    async def capture_handler(event, data):
        captured.update(data)
        return HookResult(action="continue")

    registry.register("test:event", capture_handler, name="capture")

    await registry.emit("test:event", {"key": "value"})

    assert "timestamp" in captured, "emit() must stamp a 'timestamp' field"
    # Must parse as valid ISO-8601
    ts = datetime.fromisoformat(captured["timestamp"])
    assert ts.tzinfo is not None, "timestamp must be timezone-aware"
    # Must be UTC (offset zero)
    offset = ts.utcoffset()
    assert offset is not None
    assert offset.total_seconds() == 0, "timestamp must be UTC"


@pytest.mark.asyncio
async def test_emit_timestamp_is_infrastructure_owned():
    """emit() timestamp is infrastructure-owned — callers cannot override it."""
    registry = HookRegistry()
    captured = {}

    async def capture_handler(event, data):
        captured.update(data)
        return HookResult(action="continue")

    registry.register("test:event", capture_handler, name="capture")

    await registry.emit("test:event", {"timestamp": "user-provided"})

    assert captured["timestamp"] != "user-provided", (
        "Infrastructure-owned timestamp must not be overridable by callers"
    )
    # Must still be a valid ISO-8601 UTC timestamp
    ts = datetime.fromisoformat(captured["timestamp"])
    assert ts.tzinfo is not None
    offset = ts.utcoffset()
    assert offset is not None
    assert offset.total_seconds() == 0


@pytest.mark.asyncio
async def test_emit_and_collect_does_not_stamp_timestamp():
    """emit_and_collect() must NOT stamp a timestamp (per upstream design)."""
    registry = HookRegistry()
    captured = {}

    async def capture_handler(event, data):
        captured.update(data)
        return HookResult(action="continue", data={"seen": True})

    registry.register("test:event", capture_handler, name="capture")

    await registry.emit_and_collect("test:event", {"key": "value"})

    assert "timestamp" not in captured, "emit_and_collect() must NOT stamp a timestamp"
