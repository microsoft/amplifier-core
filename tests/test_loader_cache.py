"""
Tests for ModuleLoader cache fix: raw mount functions cached, fresh closures per load().

Bug: load() caches mount closures (with config baked in) keyed by module_id.
When two provider entries share the same module_id but different configs,
the second call returns the first's closure — silently discarding the second config.

Fix: Cache resolved raw mount functions (the entry point resolution), create
fresh closures per load() call with the correct config.
"""

from unittest.mock import MagicMock

import pytest

from amplifier_core.loader import ModuleLoader


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


async def fake_mount(coordinator, config):
    """Raw mount function that returns the config it was called with.

    Acts as a stand-in for a real provider's mount() — it accepts
    (coordinator, config) and returns config so tests can inspect it.
    """
    return config


class FakeEntryPoint:
    """Simulates a Python entry point for an Amplifier module."""

    def __init__(self, name: str):
        self.name = name

    def load(self):
        return fake_mount


class TrackingEntryPoint:
    """Entry point that counts how many times .load() is called."""

    def __init__(self, name: str):
        self.name = name
        self.load_call_count = 0

    def load(self):
        self.load_call_count += 1
        return fake_mount


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def loader():
    return ModuleLoader()


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_loader_returns_different_closures_for_same_module_different_config(
    loader, monkeypatch
):
    """PRIMARY BUG REGRESSION: Two load() calls with same module_id but different
    configs must produce closures that each pass THEIR OWN config downstream.

    Before the fix: the second load() returns the first's cached closure, which
    has config_a baked in — so result_b would equal config_a (wrong).
    After the fix: each load() creates a fresh closure with the current config.
    """
    config_a = {"model": "gpt-4", "instance": "a"}
    config_b = {"model": "claude-3", "instance": "b"}

    eps = [FakeEntryPoint("test-provider")]
    monkeypatch.setattr("importlib.metadata.entry_points", lambda **_kw: eps)

    closure_a = await loader.load("test-provider", config=config_a)
    closure_b = await loader.load("test-provider", config=config_b)

    # The two returned callables should be distinct objects
    assert closure_a is not closure_b, (
        "Each load() call should return a fresh closure, not the same cached object"
    )

    coordinator = MagicMock()
    result_a = await closure_a(coordinator)
    result_b = await closure_b(coordinator)

    assert result_a == config_a, (
        f"closure_a passed wrong config: expected {config_a!r}, got {result_a!r}"
    )
    assert result_b == config_b, (
        f"closure_b passed wrong config: expected {config_b!r}, got {result_b!r}\n"
        "This is the cache bug: second load() returned first instance's closure."
    )


@pytest.mark.asyncio
async def test_loader_caches_entry_point_resolution(loader, monkeypatch):
    """Entry point .load() (expensive resolution) should happen only once for
    repeated loads of the same module_id — even with different configs.

    This verifies the optimisation is preserved: we cache the raw mount
    function so the costly entry-point resolution isn't repeated.
    """
    ep = TrackingEntryPoint("test-provider")
    monkeypatch.setattr("importlib.metadata.entry_points", lambda **_kw: [ep])

    await loader.load("test-provider", config={"x": 1})
    await loader.load("test-provider", config={"x": 2})
    await loader.load("test-provider", config={"x": 3})

    assert ep.load_call_count == 1, (
        f"Entry point .load() should be called exactly once (result cached), "
        f"but was called {ep.load_call_count} times"
    )


@pytest.mark.asyncio
async def test_loader_backward_compat_single_instance(loader, monkeypatch):
    """A single load() call should work exactly as before — the returned
    closure must pass the correct config to the underlying mount function.
    """
    config = {"api_key": "sk-test", "model": "gpt-4", "temperature": 0.7}

    eps = [FakeEntryPoint("my-provider")]
    monkeypatch.setattr("importlib.metadata.entry_points", lambda **_kw: eps)

    closure = await loader.load("my-provider", config=config)

    coordinator = MagicMock()
    result = await closure(coordinator)

    assert result == config, (
        f"Single load() must pass the correct config. Expected {config!r}, got {result!r}"
    )
