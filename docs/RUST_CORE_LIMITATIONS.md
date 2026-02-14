# Rust Core Known Limitations

## Current State

The Rust core is at the "parallel availability" stage. Rust implementations exist alongside Python implementations. The Python implementations remain the active default.

## Known Limitations

### Not Yet Switched Over
- `AmplifierSession` still uses the Python implementation
- `ModuleCoordinator` still uses the Python implementation
- `HookRegistry` still uses the Python implementation
- The switchover from Python → Rust implementations is planned for a future milestone

### Async Bridge
- The `pyo3-async-runtimes` bridge between tokio and asyncio is functional but has not been stress-tested under high concurrency
- Edge cases around event loop management may exist

### Module Loading
- The module loader remains entirely in Python (by design)
- Rust-native modules are not yet supported (planned for future phases)

### Platform Support
- Tested on: Linux x86_64, Linux aarch64
- Expected to work: macOS x86_64/arm64, Windows x86_64
- Pre-built wheels: not yet available (build from source required during testing)

### Performance
- No performance improvements expected yet (Python implementations are still active)
- Performance gains will come when the switchover to Rust implementations occurs

## How to Report Issues

File issues on the amplifier-core repo with the `rust-core` label. Include:
- Platform and Python version
- Steps to reproduce
- Expected vs actual behavior
- Output of `python -c "import amplifier_core._engine as e; print(e.__version__, e.RUST_AVAILABLE)"`
