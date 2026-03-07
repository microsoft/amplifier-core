"""Tests for WASM module mounting via loader_dispatch.

Verifies that WASM modules loaded through loader_dispatch are actually
mounted into the coordinator's mount_points, not just loaded and discarded.

Uses mocks to avoid slow WASM compilation on ARM64 while still verifying
the critical behavior: _noop_mount is replaced with a real bridge that
calls load_and_mount_wasm.
"""

import os
import sys
import tempfile
from unittest.mock import MagicMock, patch

import pytest


@pytest.fixture
def fixture_dir():
    """Create a temp directory referencing the echo-tool fixture location."""
    # Use the real fixture path for documentation clarity, but the mock
    # means we won't actually read WASM files during the test.
    fixture_base = os.path.join(
        os.path.dirname(__file__),
        "..",
        "..",
        "tests",
        "fixtures",
        "wasm",
    )
    wasm_path = os.path.join(fixture_base, "echo-tool.wasm")
    if not os.path.exists(wasm_path):
        pytest.skip(f"WASM fixture not found: {wasm_path}")

    with tempfile.TemporaryDirectory() as tmpdir:
        # Write an amplifier.toml so Python fallback detects wasm transport
        toml_path = os.path.join(tmpdir, "amplifier.toml")
        with open(toml_path, "w") as f:
            f.write('[module]\ntransport = "wasm"\ntype = "tool"\n')
        yield tmpdir


@pytest.mark.asyncio
async def test_wasm_tool_mounts_into_coordinator(fixture_dir):
    """WASM tool loaded via loader_dispatch is actually registered in coordinator.mount_points['tools'].

    With the old _noop_mount, the mount function did nothing and the tool
    was never registered.  With the real bridge, load_and_mount_wasm is
    called at mount time and the tool appears in mount_points['tools'].
    """
    from amplifier_core.loader_dispatch import load_module

    # Mock coordinator with real mount_points dict structure
    coordinator = MagicMock()
    coordinator.loader = None
    coordinator.mount_points = {
        "orchestrator": None,
        "providers": {},
        "tools": {},
        "context": None,
        "hooks": MagicMock(),
        "module-source-resolver": None,
    }

    # Mock the Rust _engine module
    fake_engine = MagicMock()
    fake_engine.resolve_module.return_value = {
        "transport": "wasm",
        "name": "echo-tool",
    }

    # Simulate what load_and_mount_wasm does: mount tool into coordinator
    def fake_load_and_mount(coord, path):
        tool_mock = MagicMock()
        tool_mock.name = "echo-tool"
        coord.mount_points["tools"]["echo-tool"] = tool_mock
        return {"status": "mounted", "module_type": "tool", "name": "echo-tool"}

    fake_engine.load_and_mount_wasm = MagicMock(side_effect=fake_load_and_mount)
    # Also provide load_wasm_from_path for backward compat (old code path)
    fake_engine.load_wasm_from_path.return_value = {
        "status": "loaded",
        "module_type": "tool",
    }

    with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
        mount_fn = await load_module("echo-tool", {}, fixture_dir, coordinator)

    # mount_fn must be callable
    assert callable(mount_fn)

    # Before calling mount: tools should still be empty
    assert "echo-tool" not in coordinator.mount_points["tools"]

    # Call the mount function — this is where the tool gets registered
    await mount_fn(coordinator)  # type: ignore[misc]

    # The tool must now be in the coordinator's mount_points
    tools = coordinator.mount_points["tools"]
    assert "echo-tool" in tools, (
        f"'echo-tool' not found in mount_points['tools']. Keys: {list(tools.keys())}"
    )

    # Verify load_and_mount_wasm was called with the coordinator and path
    fake_engine.load_and_mount_wasm.assert_called_once_with(coordinator, fixture_dir)
