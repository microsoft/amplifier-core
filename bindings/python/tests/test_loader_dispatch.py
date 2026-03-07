"""Tests for the polyglot loader dispatch module."""

import os
import sys
import tempfile
from unittest.mock import MagicMock, patch

import pytest


def test_dispatch_module_exists():
    """The loader_dispatch module is importable."""
    from amplifier_core import loader_dispatch

    assert hasattr(loader_dispatch, "load_module")


def test_dispatch_no_toml_falls_back_to_python():
    """Without amplifier.toml, dispatch falls through to Python loader."""
    from amplifier_core.loader_dispatch import _detect_transport

    with tempfile.TemporaryDirectory() as tmpdir:
        transport = _detect_transport(tmpdir)
        assert transport == "python"


def test_dispatch_detects_grpc_transport():
    """amplifier.toml with transport=grpc is detected."""
    from amplifier_core.loader_dispatch import _detect_transport

    with tempfile.TemporaryDirectory() as tmpdir:
        toml_path = os.path.join(tmpdir, "amplifier.toml")
        with open(toml_path, "w") as f:
            f.write('[module]\nname = "test"\ntype = "tool"\ntransport = "grpc"\n')
        transport = _detect_transport(tmpdir)
        assert transport == "grpc"


def test_dispatch_detects_python_transport():
    """amplifier.toml with transport=python is detected."""
    from amplifier_core.loader_dispatch import _detect_transport

    with tempfile.TemporaryDirectory() as tmpdir:
        toml_path = os.path.join(tmpdir, "amplifier.toml")
        with open(toml_path, "w") as f:
            f.write('[module]\nname = "test"\ntype = "tool"\ntransport = "python"\n')
        transport = _detect_transport(tmpdir)
        assert transport == "python"


def test_dispatch_detects_native_transport():
    """amplifier.toml with transport=native is detected."""
    from amplifier_core.loader_dispatch import _detect_transport

    with tempfile.TemporaryDirectory() as tmpdir:
        toml_path = os.path.join(tmpdir, "amplifier.toml")
        with open(toml_path, "w") as f:
            f.write('[module]\nname = "test"\ntype = "tool"\ntransport = "native"\n')
        transport = _detect_transport(tmpdir)
        assert transport == "native"


def test_dispatch_defaults_to_python_when_transport_missing():
    """amplifier.toml without transport key defaults to python."""
    from amplifier_core.loader_dispatch import _detect_transport

    with tempfile.TemporaryDirectory() as tmpdir:
        toml_path = os.path.join(tmpdir, "amplifier.toml")
        with open(toml_path, "w") as f:
            f.write('[module]\nname = "test"\ntype = "tool"\n')
        transport = _detect_transport(tmpdir)
        assert transport == "python"


def test_dispatch_reads_grpc_endpoint():
    """amplifier.toml grpc section provides endpoint."""
    from amplifier_core.loader_dispatch import _read_module_meta

    with tempfile.TemporaryDirectory() as tmpdir:
        toml_path = os.path.join(tmpdir, "amplifier.toml")
        with open(toml_path, "w") as f:
            f.write(
                '[module]\nname = "my-tool"\ntype = "tool"\ntransport = "grpc"\n\n[grpc]\nendpoint = "localhost:50052"\n'
            )
        meta = _read_module_meta(tmpdir)
        assert meta["module"]["transport"] == "grpc"
        assert meta["grpc"]["endpoint"] == "localhost:50052"


@pytest.mark.asyncio
async def test_load_module_uses_rust_loader_for_wasm_transport():
    """load_module calls load_wasm_from_path and returns callable when Rust resolver detects wasm."""
    from amplifier_core.loader_dispatch import load_module

    fake_engine = MagicMock()
    fake_engine.resolve_module.return_value = {"transport": "wasm", "name": "test-wasm"}
    fake_engine.load_wasm_from_path.return_value = b"wasm-bytes"

    coordinator = MagicMock()
    coordinator.loader = None

    with tempfile.TemporaryDirectory() as tmpdir:
        with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
            result = await load_module("test-wasm", {}, tmpdir, coordinator)

    assert callable(result)
    fake_engine.load_wasm_from_path.assert_called_once_with(tmpdir)


@pytest.mark.asyncio
async def test_load_module_wasm_without_rust_engine_raises_not_implemented():
    """load_module raises NotImplementedError for wasm when Rust engine is not available."""
    from amplifier_core.loader_dispatch import load_module

    coordinator = MagicMock()
    coordinator.loader = None

    with tempfile.TemporaryDirectory() as tmpdir:
        # Write an amplifier.toml so Python fallback detects wasm
        toml_path = os.path.join(tmpdir, "amplifier.toml")
        with open(toml_path, "w") as f:
            f.write('[module]\nname = "test"\ntype = "tool"\ntransport = "wasm"\n')

        # Setting sys.modules entry to None makes any "from pkg import X" raise ImportError
        with patch.dict(sys.modules, {"amplifier_core._engine": None}):
            with pytest.raises(NotImplementedError, match="Rust engine"):
                await load_module("test-wasm", {}, tmpdir, coordinator)


@pytest.mark.asyncio
async def test_load_module_falls_back_when_rust_resolver_raises():
    """load_module falls back to Python transport detection when Rust resolver raises."""
    from amplifier_core.loader_dispatch import load_module

    fake_engine = MagicMock()
    fake_engine.resolve_module.side_effect = RuntimeError("resolver blew up")

    coordinator = MagicMock()
    coordinator.loader = None

    with tempfile.TemporaryDirectory() as tmpdir:
        # No amplifier.toml → Python detection returns "python" → tries Python loader
        with patch.dict(sys.modules, {"amplifier_core._engine": fake_engine}):
            # Python loader itself will fail (no real coordinator), but we just need
            # to confirm it tried the Python fallback path (not raise from Rust error).
            # TypeError is raised when the MagicMock coordinator's source_resolver
            # returns a MagicMock that can't be awaited.
            with pytest.raises((TypeError, ValueError)):
                await load_module("test-mod", {}, tmpdir, coordinator)
