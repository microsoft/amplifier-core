"""Tests that top-level amplifier_core imports return Rust-backed types.

After the switchover:
- `from amplifier_core import AmplifierSession` → RustSession
- `from amplifier_core import HookRegistry` → RustHookRegistry
- `from amplifier_core import CancellationToken` → RustCancellationToken
- `from amplifier_core import ModuleCoordinator` → subclass of RustCoordinator
- Submodule paths still give Python types
"""


def test_amplifier_session_is_rust_backed():
    """Top-level AmplifierSession should be the Rust type."""
    from amplifier_core import AmplifierSession
    from amplifier_core._engine import RustSession

    assert AmplifierSession is RustSession


def test_hook_registry_is_rust_backed():
    """Top-level HookRegistry should be the Rust type."""
    from amplifier_core import HookRegistry
    from amplifier_core._engine import RustHookRegistry

    assert HookRegistry is RustHookRegistry


def test_cancellation_token_is_rust_backed():
    """Top-level CancellationToken should be the Rust type."""
    from amplifier_core import CancellationToken
    from amplifier_core._engine import RustCancellationToken

    assert CancellationToken is RustCancellationToken


def test_module_coordinator_is_rust_backed():
    """Top-level ModuleCoordinator should be based on RustCoordinator."""
    from amplifier_core import ModuleCoordinator
    from amplifier_core._engine import RustCoordinator

    assert issubclass(ModuleCoordinator, RustCoordinator)


def test_module_coordinator_has_process_hook_result():
    """Top-level ModuleCoordinator should have process_hook_result."""
    from amplifier_core import ModuleCoordinator

    assert hasattr(ModuleCoordinator, "process_hook_result")


def test_submodule_session_still_python():
    """Submodule import should still give Python type."""
    from amplifier_core.session import AmplifierSession as PySession
    from amplifier_core._engine import RustSession

    assert PySession is not RustSession


def test_submodule_coordinator_still_python():
    """Submodule import should still give Python type."""
    from amplifier_core.coordinator import ModuleCoordinator as PyCo
    from amplifier_core._engine import RustCoordinator

    assert not issubclass(PyCo, RustCoordinator)


def test_submodule_hooks_still_python():
    """Submodule import should still give Python type."""
    from amplifier_core.hooks import HookRegistry as PyHR
    from amplifier_core._engine import RustHookRegistry

    assert PyHR is not RustHookRegistry
