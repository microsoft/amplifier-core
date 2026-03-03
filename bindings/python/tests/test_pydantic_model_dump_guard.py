"""Tests for Pydantic model_dump() guard in serialization sites.

Verifies that emit(), emit_and_collect(), and config serialization
correctly handle Pydantic-like objects by calling model_dump() before
json.dumps(), rather than passing the raw object to json.dumps().
"""

import pytest
from amplifier_core._engine import RustCoordinator, RustHookRegistry


class FakePydanticModel:
    """Simulates a Pydantic BaseModel with model_dump().

    json.dumps() cannot serialize this directly (raises TypeError),
    but model_dump() returns a plain dict that json.dumps() handles.
    """

    def __init__(self, key, value):
        self._key = key
        self._value = value

    def model_dump(self):
        return {self._key: self._value}


@pytest.mark.asyncio
async def test_emit_accepts_pydantic_model_data():
    """emit() should call model_dump() on Pydantic-like data before json.dumps().

    Without the guard, json.dumps(FakePydanticModel(...)) raises TypeError.
    With the guard, model_dump() is called first, returning a serializable dict.
    """
    registry = RustHookRegistry()

    def handler(event, data):
        return {"action": "continue", "data": data}

    registry.register("test:event", handler, 0, name="test-hook")

    model = FakePydanticModel("greeting", "hello")
    # This should NOT raise — model_dump() guard converts to dict first
    result = await registry.emit("test:event", model)  # type: ignore[arg-type]
    assert result is not None
    assert result.action == "continue"


@pytest.mark.asyncio
async def test_emit_and_collect_accepts_pydantic_model_data():
    """emit_and_collect() should call model_dump() on Pydantic-like data before json.dumps().

    Without the guard, json.dumps(FakePydanticModel(...)) raises TypeError.
    With the guard, model_dump() is called first, returning a serializable dict.
    """
    registry = RustHookRegistry()

    model = FakePydanticModel("key", "value")
    # This should NOT raise — model_dump() guard converts to dict first
    result = await registry.emit_and_collect("test:event", model)  # type: ignore[arg-type]
    assert isinstance(result, list)


@pytest.mark.asyncio
async def test_emit_still_works_with_plain_dict():
    """emit() should still work with plain dicts (model_dump() guard is no-op)."""
    registry = RustHookRegistry()

    def handler(event, data):
        return {"action": "continue", "data": data}

    registry.register("test:event", handler, 0, name="test-hook")
    result = await registry.emit("test:event", {"plain": "dict"})
    assert result is not None
    assert result.action == "continue"


@pytest.mark.asyncio
async def test_emit_and_collect_still_works_with_plain_dict():
    """emit_and_collect() should still work with plain dicts."""
    registry = RustHookRegistry()
    result = await registry.emit_and_collect("test:event", {"plain": "dict"})
    assert isinstance(result, list)


# ---- Config serialization path (PyCoordinator.__new__) ----


class FakePydanticConfig:
    """Simulates a Pydantic config object with model_dump().

    json.dumps() cannot serialize this directly (raises TypeError),
    but model_dump() returns a plain dict that json.dumps() handles.
    """

    def model_dump(self):
        return {"session": {"orchestrator": "loop-basic"}}


class _SessionWithPydanticConfig:
    """Session whose config is a Pydantic-like object, not a plain dict."""

    session_id = "pydantic-cfg-test"
    parent_id = None
    config = FakePydanticConfig()


def test_coordinator_accepts_pydantic_config():
    """RustCoordinator.__new__ should call model_dump() on config before json.dumps().

    Without the guard, json.dumps(FakePydanticConfig()) raises TypeError.
    With the guard, model_dump() is called first, returning a serializable dict.
    """
    # This should NOT raise — model_dump() guard converts config to dict first
    coord = RustCoordinator(_SessionWithPydanticConfig())
    assert coord is not None
    assert coord.session_id == "pydantic-cfg-test"


def test_coordinator_still_works_with_plain_dict_config():
    """RustCoordinator.__new__ should still work with plain dict configs."""

    class _SessionWithDictConfig:
        session_id = "dict-cfg-test"
        parent_id = None
        config = {"session": {"orchestrator": "loop-basic"}}

    coord = RustCoordinator(_SessionWithDictConfig())
    assert coord is not None
    assert coord.session_id == "dict-cfg-test"
