"""Tests for WASM module mounting via loader.load() dispatch.

Verifies that WASM modules loaded through loader.load() are actually
mounted into the coordinator's mount_points, not just loaded and discarded.

Uses mocks to avoid slow WASM compilation on ARM64 while still verifying
the critical behavior: the mount closure returned by loader.load() calls
load_and_mount_wasm at mount time.
"""

import sys
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from amplifier_core.loader import ModuleLoader

MODULE_ID = "echo-tool"


@pytest.fixture
def wasm_fixture_path():
    """Path to the echo-tool.wasm fixture file. Skips if missing."""
    path = (
        Path(__file__).parent
        / ".."
        / ".."
        / "tests"
        / "fixtures"
        / "wasm"
        / f"{MODULE_ID}.wasm"
    )
    if not path.exists():
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


@pytest.mark.asyncio
async def test_wasm_tool_mounts_into_coordinator(wasm_fixture_path, mock_coordinator):
    """WASM tool loaded via loader.load() is actually registered in coordinator.mount_points['tools'].

    With the old _noop_mount, the mount function did nothing and the tool
    was never registered.  With the real bridge, load_and_mount_wasm is
    called at mount time and the tool appears in mount_points['tools'].
    """
    # -- Mock source resolution -----------------------------------------------
    fake_source = MagicMock()
    fake_source.resolve.return_value = wasm_fixture_path

    mock_resolver = MagicMock()
    mock_resolver.async_resolve = AsyncMock(return_value=fake_source)

    # Wire resolver into coordinator
    mock_coordinator.get.return_value = mock_resolver

    # -- Mock Rust engine -----------------------------------------------------
    fake_engine = MagicMock()
    fake_engine.resolve_module.return_value = {
        "transport": "wasm",
        "name": MODULE_ID,
    }

    # Simulate what load_and_mount_wasm does: mount tool into coordinator
    def fake_load_and_mount(coord, path):
        tool_mock = MagicMock()
        tool_mock.name = MODULE_ID
        coord.mount_points["tools"][MODULE_ID] = tool_mock
        return {"status": "mounted", "module_type": "tool", "name": MODULE_ID}

    fake_engine.load_and_mount_wasm = MagicMock(side_effect=fake_load_and_mount)

    # -- Execute --------------------------------------------------------------
    loader = ModuleLoader(coordinator=mock_coordinator)

    with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
        mount_fn = await loader.load(
            MODULE_ID, {}, source_hint="/fake/path", coordinator=mock_coordinator
        )

    # -- Verify ---------------------------------------------------------------
    # 1. mount_fn must be callable
    assert callable(mount_fn)

    # 2. echo-tool NOT in mount_points before calling mount
    assert MODULE_ID not in mock_coordinator.mount_points["tools"]

    # 3. Call the mount function — this is where the tool gets registered
    with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
        await mount_fn(mock_coordinator)

    # 4. echo-tool IS in mount_points after calling mount
    tools = mock_coordinator.mount_points["tools"]
    assert MODULE_ID in tools, (
        f"'{MODULE_ID}' not found in mount_points['tools']. Keys: {list(tools.keys())}"
    )

    # 5. load_and_mount_wasm was called with correct args
    fake_engine.load_and_mount_wasm.assert_called_once_with(
        mock_coordinator, str(wasm_fixture_path)
    )
