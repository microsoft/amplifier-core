# Testing the Rust Core (rust-core branch)

## Switchover Status: COMPLETE

The switchover from Python to Rust-backed types is complete (Milestones 1-5).

- `from amplifier_core import AmplifierSession` now returns the **Rust-backed** `RustSession`
- `from amplifier_core.session import AmplifierSession` still returns the pure-Python type
- `from amplifier_core import HookRegistry` returns `RustHookRegistry`
- `from amplifier_core import CancellationToken` returns `RustCancellationToken`
- `from amplifier_core import ModuleCoordinator` returns a thin Python subclass of `RustCoordinator`

All **384 Python tests pass**, covering:
- 196 original Python unit tests (`tests/`)
- 188 bridge, switchover, and dogfood validation tests (`bindings/python/tests/`)

The Rust kernel also has its own test suite (190+ tests via `cargo test`).

## Quick Start

```bash
# Clone and switch to the rust-core branch
git clone https://github.com/microsoft/amplifier-core.git
cd amplifier-core
git checkout rust-core

# Install Rust toolchain (required for building from source)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build and install the Rust-backed wheel
pip install maturin
maturin develop

# Verify it works
python -c "from amplifier_core import AmplifierSession; print('Rust core loaded successfully')"
python -c "from amplifier_core._engine import RUST_AVAILABLE; print(f'Rust available: {RUST_AVAILABLE}')"
```

## What Changed

The `amplifier-core` package now includes a Rust-compiled extension module (`_engine`) that provides high-performance implementations of Session, Coordinator, HookRegistry, and CancellationToken. Top-level imports return the Rust-backed types; submodule paths still give the pure-Python implementations.

### Import behavior after switchover:

| Import path | Returns |
|---|---|
| `from amplifier_core import AmplifierSession` | `RustSession` (Rust-backed) |
| `from amplifier_core.session import AmplifierSession` | Python `AmplifierSession` |
| `from amplifier_core import HookRegistry` | `RustHookRegistry` |
| `from amplifier_core.hooks import HookRegistry` | Python `HookRegistry` |
| `from amplifier_core import CancellationToken` | `RustCancellationToken` |
| `from amplifier_core import ModuleCoordinator` | Python subclass of `RustCoordinator` |

### What's the same (everything consumers see):
- All 61 public symbols in `amplifier_core`
- All Pydantic models, Protocol interfaces, module loader, validation framework
- The API surface is identical — the Rust types expose the same methods and properties

### What's new:
- Rust types are the **default** at the top-level import
- `RUST_AVAILABLE` flag is `True` when the Rust extension is loaded
- Dogfood validation tests confirm real Foundation usage patterns work end-to-end

## Running Tests

```bash
# Rust kernel tests
cargo test -p amplifier-core

# All Python tests (original + bridge + dogfood)
uv run pytest tests/ bindings/python/tests/ -v

# Just the dogfood validation tests
uv run pytest bindings/python/tests/test_dogfood_validation.py -v

# Everything together
cargo test -p amplifier-core && uv run pytest tests/ bindings/python/tests/ -v
```

## Reporting Issues

If you encounter any issues:
1. Check if the issue reproduces with the Python-only version (main branch)
2. Include the output of `python -c "import amplifier_core._engine; print(amplifier_core._engine.__version__)"`
3. Include your platform info (OS, Python version, Rust version)
