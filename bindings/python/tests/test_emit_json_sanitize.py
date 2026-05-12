"""Tests for non-JSON-native type safety at the emit() FFI boundary.

Verifies that emit(), emit_and_collect(), and hook result serialization
never crash on non-JSON-native Python types (e.g. Decimal, datetime).
The fix: json.dumps(..., default=str) at all FFI call sites.
"""

import pytest

from amplifier_core._engine import RustHookRegistry


@pytest.mark.asyncio
async def test_emit_with_decimal_does_not_crash():
    """emit() must not raise when the event payload contains a Decimal.

    Without the fix, json.dumps({"cost": Decimal("1.23")}) raises TypeError
    at the FFI boundary, crashing the caller before the handler is ever invoked.
    """
    from decimal import Decimal

    registry = RustHookRegistry()
    received = []

    def handler(event, data):
        received.append(data)
        return None

    registry.register("test:event", handler, 0, name="test-hook")
    await registry.emit("test:event", {"cost": Decimal("1.23")})
    assert len(received) == 1


@pytest.mark.asyncio
async def test_emit_decimal_serializes_as_string():
    """emit() must serialize Decimal values as their str() representation.

    str(Decimal("1.23")) == "1.23", which matches the @field_serializer
    output on the Pydantic model path — no inconsistency.
    """
    from decimal import Decimal

    registry = RustHookRegistry()
    received = []

    def handler(event, data):
        received.append(data)
        return None

    registry.register("test:event", handler, 0, name="test-hook")
    await registry.emit("test:event", {"cost": Decimal("1.23")})
    assert received[0]["cost"] == "1.23"


@pytest.mark.asyncio
async def test_emit_datetime_does_not_crash():
    """emit() must not raise when the event payload contains a datetime.

    datetime objects are not JSON-native. str(datetime(2024,1,1,12,0,0))
    produces "2024-01-01 12:00:00".
    """
    from datetime import datetime

    registry = RustHookRegistry()
    received = []

    def handler(event, data):
        received.append(data)
        return None

    registry.register("test:event", handler, 0, name="test-hook")
    await registry.emit("test:event", {"ts": datetime(2024, 1, 1, 12, 0, 0)})
    assert len(received) == 1
    assert received[0]["ts"] == "2024-01-01 12:00:00"


@pytest.mark.asyncio
async def test_emit_and_collect_with_decimal_does_not_crash():
    """emit_and_collect() must not raise on Decimal in the event payload.

    This exercises a separate json.dumps() call site from emit() —
    both FFI entry points must be fixed.
    """
    from decimal import Decimal

    registry = RustHookRegistry()

    def handler(event, data):
        return {"action": "continue", "data": {}}

    registry.register("test:event", handler, 0, name="test-hook")
    results = await registry.emit_and_collect("test:event", {"cost": Decimal("1.23")})
    assert isinstance(results, list)


@pytest.mark.asyncio
async def test_hook_result_with_decimal_in_data_does_not_crash():
    """A hook handler returning a dict with Decimal must not crash.

    This exercises the bridges.rs result-serialization path (Step 3 inside
    PyHookHandlerBridge::handle()), a separate call site from the emit()
    input-serialization path.
    """
    from decimal import Decimal

    registry = RustHookRegistry()

    def handler(event, data):
        return {"action": "continue", "data": {"cost": Decimal("2.50")}}

    registry.register("test:event", handler, 0, name="test-hook")
    # Must not raise — handler returns Decimal in result dict
    await registry.emit("test:event", {"input": "test"})
