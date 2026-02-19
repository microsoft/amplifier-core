"""Tests for the polyglot loader dispatch module."""

import os
import tempfile


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
