"""Tests for thin Python re-export stubs (events.py, capabilities.py, coordinator.py, cancellation.py).

These verify that the Python modules re-export types/constants from the Rust _engine
module, maintaining backward-compatible import paths.

Note: session.py and hooks.py are NOT thinned yet because RustSession and
RustHookRegistry are not yet drop-in replacements for the Python implementations.
"""


def test_events_reexport_session_start():
    from amplifier_core.events import SESSION_START

    assert SESSION_START == "session:start"


def test_events_reexport_provider_throttle():
    from amplifier_core.events import PROVIDER_THROTTLE

    assert PROVIDER_THROTTLE == "provider:throttle"


def test_events_reexport_all_events():
    from amplifier_core.events import ALL_EVENTS

    # CP-V: 10 tiered :debug/:raw constants removed — 41 canonical events remain
    assert len(ALL_EVENTS) == 41


def test_capabilities_reexport_tools():
    from amplifier_core.capabilities import TOOLS

    assert TOOLS == "tools"


def test_capabilities_reexport_all_well_known():
    from amplifier_core.capabilities import ALL_WELL_KNOWN_CAPABILITIES

    assert len(ALL_WELL_KNOWN_CAPABILITIES) == 16


def test_capabilities_importable_from_init():
    from amplifier_core import capabilities

    assert hasattr(capabilities, "TOOLS")


# ---- Kernel module re-export stubs (coordinator, cancellation) ----


def test_coordinator_reexport():
    """coordinator.py re-exports the Rust-backed wrapper ModuleCoordinator."""
    from amplifier_core.coordinator import ModuleCoordinator

    from amplifier_core._rust_wrappers import ModuleCoordinator as WrapperCoord

    assert ModuleCoordinator is WrapperCoord


def test_cancellation_reexport():
    """cancellation.py re-exports RustCancellationToken as CancellationToken."""
    from amplifier_core.cancellation import CancellationToken

    from amplifier_core._engine import RustCancellationToken

    assert CancellationToken is RustCancellationToken


def test_cancellation_state_still_importable():
    """CancellationState enum must still be importable from cancellation.py."""
    from amplifier_core.cancellation import CancellationState

    assert CancellationState.NONE.value == "none"
    assert CancellationState.GRACEFUL.value == "graceful"
    assert CancellationState.IMMEDIATE.value == "immediate"


def test_backward_compat_imports():
    """All previously-importable submodule symbols remain importable."""
    # session.py (still full Python)
    from amplifier_core.session import AmplifierSession

    assert AmplifierSession is not None

    # coordinator.py (re-export stub)
    from amplifier_core.coordinator import ModuleCoordinator

    assert ModuleCoordinator is not None

    # hooks.py (still full Python)
    from amplifier_core.hooks import HookRegistry

    assert HookRegistry is not None

    # cancellation.py (re-export stub)
    from amplifier_core.cancellation import CancellationToken, CancellationState

    assert CancellationToken is not None
    assert CancellationState is not None
