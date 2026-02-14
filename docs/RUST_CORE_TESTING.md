# Testing the Rust Core (rust-core branch)

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
cd bindings/python
maturin develop --release

# Verify it works
python -c "from amplifier_core import AmplifierSession; print('Rust core loaded successfully')"
python -c "from amplifier_core._engine import RUST_AVAILABLE; print(f'Rust available: {RUST_AVAILABLE}')"
```

## What Changed

The `amplifier-core` package now includes a Rust-compiled extension module (`_engine`) that provides high-performance implementations of Session, Coordinator, HookRegistry, and CancellationToken. All existing Python APIs remain unchanged.

### What's the same (everything consumers see):
- All 61 public symbols in `amplifier_core`
- All import paths (`from amplifier_core import X`, `from amplifier_core.models import Y`)
- All Pydantic models, Protocol interfaces, module loader, validation framework
- All existing tests pass (196 Python tests + 190 Rust tests + 47 bridge tests = 433 total)

### What's new:
- Rust types available at `amplifier_core._engine` (RustSession, RustHookRegistry, etc.)
- `RUST_AVAILABLE` flag indicates the Rust extension is loaded
- Future: Rust implementations will replace Python implementations for Session/Coordinator/Hooks

## Running Tests

```bash
# Rust kernel tests
cargo test -p amplifier-core

# Original Python tests
pytest tests/ -v

# Bridge/sync tests
pytest bindings/python/tests/ -v

# All tests
cargo test -p amplifier-core && pytest tests/ -v && pytest bindings/python/tests/ -v
```

## Reporting Issues

If you encounter any issues:
1. Check if the issue reproduces with the Python-only version (main branch)
2. Include the output of `python -c "import amplifier_core._engine; print(amplifier_core._engine.__version__)"`
3. Include your platform info (OS, Python version, Rust version)
