"""Tests that PyO3 async methods return proper Python coroutines.

asyncio.create_task() requires coroutines, not just awaitables.
These tests ensure RustSession/RustHookRegistry methods are drop-in
compatible with the old pure-Python async def methods.

NOTE: These tests run inside an async context because pyo3_async_runtimes'
future_into_py() requires a running event loop. The important property
being tested is that the return value satisfies inspect.iscoroutine() and
works with asyncio.create_task() — both of which need a running loop too.
"""
import asyncio
import inspect
import json
import pytest

from amplifier_core._engine import RustSession, RustHookRegistry


class TestAsyncMethodsReturnCoroutines:
    """All PyO3 async methods must return proper Python coroutines."""

    @pytest.mark.asyncio
    async def test_hook_registry_emit_returns_coroutine(self):
        registry = RustHookRegistry()
        result = registry.emit("test:event", json.dumps({"key": "value"}))
        assert inspect.iscoroutine(result), (
            f"emit() should return a coroutine, got {type(result).__name__}"
        )
        result.close()  # cleanup

    @pytest.mark.asyncio
    async def test_hook_registry_emit_and_collect_returns_coroutine(self):
        registry = RustHookRegistry()
        result = registry.emit_and_collect("test:event", json.dumps({"key": "value"}))
        assert inspect.iscoroutine(result), (
            f"emit_and_collect() should return a coroutine, got {type(result).__name__}"
        )
        result.close()

    @pytest.mark.asyncio
    async def test_emit_works_with_create_task(self):
        registry = RustHookRegistry()
        task = asyncio.create_task(registry.emit("test:event", json.dumps({})))
        result = await task
        # Should not raise TypeError: a coroutine was expected
