# Rust Core Known Limitations

## Current State

The Rust core switchover is **complete**. Rust implementations are the default exports for top-level imports. The Rust `HookRegistry` handles all hook dispatch, and `CancellationToken` uses the Rust implementation. Python implementations remain accessible via submodule imports for backward compatibility.

## Known Limitations

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

### Submodule Import Compatibility
- Submodule imports (`from amplifier_core.session import AmplifierSession`) return Python types for backward compatibility
- Top-level imports (`from amplifier_core import AmplifierSession`) return Rust-backed types
- This dual-path behavior is intentional but may cause confusion if both import styles are mixed in the same codebase

## How to Report Issues

File issues on the amplifier-core repo with the `rust-core` label. Include:
- Platform and Python version
- Steps to reproduce
- Expected vs actual behavior
- Output of `python -c "import amplifier_core._engine as e; print(e.__version__, e.RUST_AVAILABLE)"`