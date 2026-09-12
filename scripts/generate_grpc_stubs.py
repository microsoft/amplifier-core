#!/usr/bin/env python3
"""Generate package-safe Python gRPC stubs from the canonical proto."""

from __future__ import annotations

import argparse
from importlib.metadata import version
import subprocess
import sys
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parent.parent
PROTO_PATH = PROJECT_ROOT / "proto" / "amplifier_module.proto"
RAW_IMPORT = "import amplifier_module_pb2 as amplifier__module__pb2"
PACKAGE_IMPORT = "from . import amplifier_module_pb2 as amplifier__module__pb2"
GRPCIO_TOOLS_VERSION = "1.78.0"


def main() -> None:
    if version("grpcio-tools") != GRPCIO_TOOLS_VERSION:
        raise RuntimeError(
            f"grpcio-tools=={GRPCIO_TOOLS_VERSION} is required; install it with "
            f"`python -m pip install grpcio-tools=={GRPCIO_TOOLS_VERSION}`."
        )
    parser = argparse.ArgumentParser(
        description="Generate package-safe Python stubs for amplifier_module.proto."
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("python/amplifier_core/_grpc_gen"),
        help="Output directory, relative to the repository root by default.",
    )
    args = parser.parse_args()
    output = args.output if args.output.is_absolute() else PROJECT_ROOT / args.output
    output.mkdir(parents=True, exist_ok=True)

    subprocess.run(
        [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            "-Iproto",
            f"--python_out={output}",
            f"--grpc_python_out={output}",
            str(PROTO_PATH.relative_to(PROJECT_ROOT)),
        ],
        cwd=PROJECT_ROOT,
        check=True,
    )

    grpc_stub = output / "amplifier_module_pb2_grpc.py"
    generated = grpc_stub.read_text()
    if RAW_IMPORT not in generated:
        raise RuntimeError(
            f"Expected generated import not found in {grpc_stub}: {RAW_IMPORT!r}"
        )
    grpc_stub.write_text(generated.replace(RAW_IMPORT, PACKAGE_IMPORT, 1))


if __name__ == "__main__":
    main()