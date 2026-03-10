"""Tests for transport dispatch through ModuleLoader.load().

Verifies that loader.load() can dispatch to different transports (WASM, gRPC)
when the Rust engine resolves a module accordingly.
"""

import sys
import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from amplifier_core.loader import ModuleLoader

MODULE_ID = "echo-tool"


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def wasm_fixture_path():
    """Path to the echo-tool.wasm fixture file. Skips if missing."""
    path = Path(__file__).parent / "fixtures" / "wasm" / f"{MODULE_ID}.wasm"
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
        tool_mock.name = MODULE_ID
        coord.mount_points["tools"][MODULE_ID] = tool_mock
        return {"status": "mounted", "module_type": "tool", "name": MODULE_ID}

    fake_engine.load_and_mount_wasm = MagicMock(side_effect=fake_load_and_mount)

    # -- Execute --------------------------------------------------------------
    loader = ModuleLoader(coordinator=mock_coordinator)
    mount_points = mock_coordinator.mount_points

    with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
        mount_fn = await loader.load(
            MODULE_ID, {}, source_hint="/fake/path", coordinator=mock_coordinator
        )

    # -- Verify ---------------------------------------------------------------
    # mount_fn must be callable
    assert callable(mount_fn)

    # Call mount function and verify the tool is registered
    await mount_fn(mock_coordinator)
    assert MODULE_ID in mount_points["tools"]


@pytest.mark.asyncio
async def test_grpc_dispatch_routes_to_grpc_loader(mock_coordinator):
    """loader.load() with gRPC transport dispatches to gRPC loading path.

    When the Rust engine resolves a module as gRPC transport, loader.load()
    should attempt to establish a gRPC channel. Since grpcio is not installed
    (or connection fails), we expect an error whose message contains
    gRPC-related keywords, confirming the loader routed to the gRPC path
    rather than the Python entry-point path.
    """
    # -- Create temp module dir with amplifier.toml --------------------------
    with tempfile.TemporaryDirectory() as tmpdir:
        toml_path = Path(tmpdir) / "amplifier.toml"
        toml_path.write_text(
            "[module]\n"
            "name = 'my-tool'\n"
            "type = 'tool'\n"
            "transport = 'grpc'\n"
            "\n"
            "[grpc]\n"
            "endpoint = 'localhost:99999'\n"
        )

        # -- Mock source resolution ------------------------------------------
        fake_source = MagicMock()
        fake_source.resolve.return_value = Path(tmpdir)

        mock_resolver = MagicMock()
        mock_resolver.async_resolve = AsyncMock(return_value=fake_source)
        mock_coordinator.get.return_value = mock_resolver

        # -- Mock Rust engine ------------------------------------------------
        fake_engine = MagicMock()
        fake_engine.resolve_module.return_value = {
            "transport": "grpc",
            "module_type": "tool",
            "artifact_type": "grpc",
            "endpoint": "localhost:99999",
        }

        # -- Execute ---------------------------------------------------------
        loader = ModuleLoader(coordinator=mock_coordinator)

        with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
            with pytest.raises((ImportError, OSError, Exception)) as exc_info:
                await loader.load(
                    "my-grpc-tool",
                    {},
                    source_hint="/fake/path",
                    coordinator=mock_coordinator,
                )

        # -- Verify ----------------------------------------------------------
        # The error message must contain gRPC-related keywords, confirming
        # the loader dispatched to the gRPC path (not the Python path).
        error_msg = str(exc_info.value).lower()
        grpc_keywords = ("grpc", "grpcio", "connect", "channel")
        assert any(kw in error_msg for kw in grpc_keywords), (
            f"Expected gRPC-related error but got: {exc_info.value}"
        )
