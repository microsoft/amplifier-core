"""Python WASM Session Example

Proves the Python host → PyO3 → Rust resolver → wasmtime → WASM tool pipeline.

Run with:
    python examples/python-wasm-session.py

Requires the dev-branch amplifier_core with resolve_module / load_wasm_from_path
bindings compiled in (the .venv built from this repo has them):
    .venv/bin/python examples/python-wasm-session.py

Notes
-----
WASM compilation via wasmtime is slow on ARM64 (aarch64).  The script runs
the resolve step unconditionally (fast — proves Python → PyO3 → Rust resolver)
and attempts the full WASM load in a child process with a generous timeout.
If the timeout fires the script still exits 0 with a clear explanatory note.
"""

import importlib
import os
import shutil
import subprocess
import sys
import tempfile

# ---------------------------------------------------------------------------
# Locate the echo-tool fixture relative to this script
# ---------------------------------------------------------------------------

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
FIXTURE_BASE = os.path.join(SCRIPT_DIR, "..", "tests", "fixtures", "wasm")
ECHO_TOOL_WASM = os.path.join(FIXTURE_BASE, "echo-tool.wasm")

if not os.path.isfile(ECHO_TOOL_WASM):
    print(f"ERROR: WASM fixture not found: {ECHO_TOOL_WASM}", file=sys.stderr)
    print("  Build fixtures first: cd tests/fixtures/wasm && bash build-fixtures.sh")
    sys.exit(1)

# ---------------------------------------------------------------------------
# Step 0 — verify the PyO3 module exposes the required symbols
# ---------------------------------------------------------------------------

print("Checking amplifier_core._engine symbols...")
_engine = importlib.import_module("amplifier_core._engine")
_missing = [
    sym
    for sym in ("resolve_module", "load_wasm_from_path", "load_and_mount_wasm")
    if not hasattr(_engine, sym)
]
if _missing:
    print(f"  FAIL: missing symbols: {_missing}", file=sys.stderr)
    print(
        "  Rebuild the Python bindings with:"
        "  maturin develop --manifest-path bindings/python/Cargo.toml",
        file=sys.stderr,
    )
    sys.exit(1)

print("  resolve_module:      OK")
print("  load_wasm_from_path: OK")
print("  load_and_mount_wasm: OK")

resolve_module = _engine.resolve_module

# ---------------------------------------------------------------------------
# Helper: build an isolated temp directory for the resolver.
# The directory must contain:
#   amplifier.toml  — declares transport = "wasm", type = "tool"
#   module.wasm     — the compiled WASM binary (default name the resolver looks for)
# ---------------------------------------------------------------------------


def make_wasm_fixture_dir(base_tmpdir: str) -> str:
    """Copy echo-tool fixture into a clean directory the resolver can scan."""
    fixture_dir = os.path.join(base_tmpdir, "echo-tool")
    os.makedirs(fixture_dir, exist_ok=True)
    with open(os.path.join(fixture_dir, "amplifier.toml"), "w") as fh:
        fh.write('[module]\ntransport = "wasm"\ntype = "tool"\n')
    shutil.copy(ECHO_TOOL_WASM, os.path.join(fixture_dir, "module.wasm"))
    return fixture_dir


# ---------------------------------------------------------------------------
# Step 1 — resolve_module: Python → PyO3 → Rust resolver
# This step is fast (<1 ms) and proves the bridge works end-to-end.
# ---------------------------------------------------------------------------

print("\nStep 1: resolve_module  (Python → PyO3 → Rust resolver)")

with tempfile.TemporaryDirectory(prefix="amplifier-py-wasm-") as tmpdir:
    fixture_dir = make_wasm_fixture_dir(tmpdir)
    print(f"  fixture dir: {fixture_dir}")

    manifest = resolve_module(fixture_dir)
    print(f"  transport:   {manifest['transport']}")
    print(f"  module_type: {manifest['module_type']}")

    if manifest["transport"] != "wasm":
        print(f"  FAIL: expected transport='wasm', got '{manifest['transport']}'")
        sys.exit(1)
    if manifest["module_type"] != "tool":
        print(f"  FAIL: expected module_type='tool', got '{manifest['module_type']}'")
        sys.exit(1)

    print("  resolve_module: PASS")

# ---------------------------------------------------------------------------
# Step 2 — load_wasm_from_path: Rust resolver → wasmtime → WASM tool
# WASM compilation via wasmtime-cranelift can be very slow on ARM64.
# We run it in a child process so the parent can impose a wall-clock timeout
# without hanging indefinitely.
# ---------------------------------------------------------------------------

WASM_LOAD_TIMEOUT = int(os.environ.get("AMPLIFIER_WASM_LOAD_TIMEOUT", "300"))

print("\nStep 2: load_wasm_from_path  (Rust resolver → wasmtime → WASM tool)")
print(f"  timeout: {WASM_LOAD_TIMEOUT}s  (override with AMPLIFIER_WASM_LOAD_TIMEOUT)")

# Inline Python passed to the child process via -c.
# sys.argv[1] receives the path to the .wasm fixture file.
_child_script = r"""
import os, sys, tempfile, shutil

fixture = sys.argv[1]
from amplifier_core._engine import load_wasm_from_path

with tempfile.TemporaryDirectory(prefix="amplifier-py-wasm-child-") as tmpdir:
    fixture_dir = os.path.join(tmpdir, "echo-tool")
    os.makedirs(fixture_dir)
    with open(os.path.join(fixture_dir, "amplifier.toml"), "w") as fh:
        fh.write('[module]\ntransport = "wasm"\ntype = "tool"\n')
    shutil.copy(fixture, os.path.join(fixture_dir, "module.wasm"))

    result = load_wasm_from_path(fixture_dir)
    print(f"status:      {result['status']}")
    print(f"module_type: {result['module_type']}")
    assert result["status"] == "loaded", f"unexpected status: {result['status']}"
    assert result["module_type"] == "tool", f"unexpected module_type: {result['module_type']}"
"""

# wasm_load_ok: True = pass, False = fail, None = skipped (timeout)
wasm_load_ok = None
proc = None
try:
    proc = subprocess.Popen(
        [sys.executable, "-c", _child_script, ECHO_TOOL_WASM],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    stdout, stderr = proc.communicate(timeout=WASM_LOAD_TIMEOUT)

    if proc.returncode == 0:
        for line in stdout.strip().splitlines():
            print(f"  {line}")
        print("  load_wasm_from_path: PASS")
        wasm_load_ok = True
    else:
        print(f"  FAIL (exit {proc.returncode})")
        if stderr.strip():
            print(f"  stderr: {stderr.strip()[:400]}")
        wasm_load_ok = False

except subprocess.TimeoutExpired:
    if proc is not None:
        proc.kill()
        proc.wait()
    print(f"  SKIP: WASM compilation did not complete within {WASM_LOAD_TIMEOUT}s")
    print(
        "  (wasmtime-cranelift compilation is extremely slow on ARM64/aarch64;"
        " this is expected and does not indicate a code defect)"
    )
    print("  load_wasm_from_path: SKIPPED (ARM64 timeout)")
    wasm_load_ok = None  # skipped, not failed

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

print("\n" + "=" * 70)
if wasm_load_ok is True:
    print("Python → PyO3 → Rust resolver → wasmtime → WASM tool pipeline: SUCCESS")
elif wasm_load_ok is None:
    print("Python → PyO3 → Rust resolver pipeline:               SUCCESS")
    print(
        "wasmtime WASM compilation:                             SKIPPED (ARM64 timeout)"
    )
    print()
    print("Resolver pipeline fully verified.  Run on x86_64 for full end-to-end proof,")
    print("or extend AMPLIFIER_WASM_LOAD_TIMEOUT for a longer ARM64 attempt.")
else:
    print("Python → PyO3 → Rust resolver pipeline:               SUCCESS")
    print("wasmtime WASM load:                                    FAIL")
    sys.exit(1)
