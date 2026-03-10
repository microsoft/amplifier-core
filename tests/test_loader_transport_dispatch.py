"""Tests for WASM transport dispatch through ModuleLoader.load().

Verifies that loader.load() can dispatch to WASM transport when the
Rust engine resolves a module as WASM, returning a callable mount
function that registers the tool in the coordinator's mount_points.
"""

import os
import sys
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from amplifier_core.loader import ModuleLoader


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def wasm_fixture_path():
    """Path to the echo-tool.wasm fixture file. Skips if missing."""
    path = os.path.join(
        os.path.dirname(__file__),
        "fixtures",
        "wasm",
        "echo-tool.wasm",
    )
    if not os.path.exists(path):
        pytest.skip(f"WASM fixture not found: {path}")
    return path


@pytest.fixture
def mock_coordinator():
    """MagicMock coordinator with real mount_points structure."""
    coord = MagicMock()
    coord.mount_points = {
        "orchestrator": None,
        "providers": {},
        "tools": {},
        "context": None,
        "hooks": MagicMock(),
        "module-source-resolver": None,
    }
    return coord


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_wasm_dispatch_returns_mount_function(
    wasm_fixture_path, mock_coordinator
):
    """loader.load() with a WASM path returns a callable mount function.

    The Rust engine resolves the module as WASM transport. loader.load()
    should dispatch to the WASM loading path and return a mount function
    that, when called with a coordinator, mounts the tool into
    mount_points['tools'].
    """
    # -- Mock source resolution -----------------------------------------------
    # fake_source.resolve returns the wasm fixture path
    fake_source = MagicMock()
    fake_source.resolve.return_value = wasm_fixture_path

    # mock_resolver.async_resolve returns fake_source
    mock_resolver = MagicMock()
    mock_resolver.async_resolve = AsyncMock(return_value=fake_source)

    # Wire resolver into coordinator so the loader finds it at
    # coordinator.get("module-source-resolver")
    mock_coordinator.get.return_value = mock_resolver

    # -- Mock Rust engine -----------------------------------------------------
    fake_engine = MagicMock()
    fake_engine.resolve_module.return_value = {
        "transport": "wasm",
        "module_type": "tool",
        "artifact_type": "wasm",
        "artifact_path": wasm_fixture_path,
    }

    # Simulate what load_and_mount_wasm does: mount tool into coordinator
    def fake_load_and_mount(coord, path):
        tool_mock = MagicMock()
        tool_mock.name = "echo-tool"
        coord.mount_points["tools"]["echo-tool"] = tool_mock
        return {"status": "mounted", "module_type": "tool", "name": "echo-tool"}

    fake_engine.load_and_mount_wasm = MagicMock(side_effect=fake_load_and_mount)

    # -- Execute --------------------------------------------------------------
    loader = ModuleLoader(coordinator=mock_coordinator)
    mount_points = mock_coordinator.mount_points

    with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
        mount_fn = await loader.load(
            "echo-tool", {}, source_hint="/fake/path", coordinator=mock_coordinator
        )

    # -- Verify ---------------------------------------------------------------
    # mount_fn must be callable
    assert callable(mount_fn)

    # Call mount function and verify the tool is registered
    await mount_fn(mock_coordinator)
    assert "echo-tool" in mount_points["tools"]
