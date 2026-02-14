"""amplifier-core: Ultra-thin core for Amplifier modular AI agent system."""

__version__ = "1.0.0"

# Verify Rust engine loads
from amplifier_core._engine import RUST_AVAILABLE as _RUST_AVAILABLE

assert _RUST_AVAILABLE, "Rust engine failed to load"
