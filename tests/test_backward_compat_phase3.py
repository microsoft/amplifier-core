"""Backward compatibility tests for Phase 3.

Verifies the external contract surface of amplifier_core is intact after
the migration from _rust_wrappers to _engine and the Python-to-Rust switchover.
"""

import types

import pytest


# ---------------------------------------------------------------------------
# HookResult import path tests
# ---------------------------------------------------------------------------


def test_hook_result_import_path_top_level():
    """from amplifier_core import HookResult works, can create instance."""
    from amplifier_core import HookResult

    hr = HookResult(action="continue")
    assert hr.action == "continue"


def test_hook_result_import_path_models():
    """from amplifier_core.models import HookResult works."""
    from amplifier_core.models import HookResult

    hr = HookResult(action="continue")
    assert hr.action == "continue"


def test_hook_result_import_path_hooks():
    """from amplifier_core.hooks import HookResult works."""
    from amplifier_core.hooks import HookResult

    hr = HookResult(action="continue")
    assert hr.action == "continue"


def test_hook_result_all_paths_same_type():
    """All 3 HookResult import paths resolve to the same type."""
    from amplifier_core import HookResult as HR1
    from amplifier_core.models import HookResult as HR2
    from amplifier_core.hooks import HookResult as HR3

    assert HR1 is HR2
    assert HR2 is HR3


# ---------------------------------------------------------------------------
# ModuleCoordinator import tests
# ---------------------------------------------------------------------------


def _make_minimal_session():
    """Create a minimal fake session for coordinator construction."""
    return types.SimpleNamespace(
        session_id="test-session",
        parent_id=None,
        config={"session": {"orchestrator": "loop-basic"}},
    )


def test_module_coordinator_import_top_level():
    """from amplifier_core import ModuleCoordinator works with expected attributes."""
    from amplifier_core import ModuleCoordinator

    coord = ModuleCoordinator(_make_minimal_session())

    # Verify the core backward-compat attribute surface
    assert hasattr(coord, "mount"), "coord missing 'mount'"
    assert hasattr(coord, "get"), "coord missing 'get'"
    assert hasattr(coord, "hooks"), "coord missing 'hooks'"
    # 'session' holds the session reference (backward-compat name for session_state)
    assert hasattr(coord, "session"), "coord missing 'session'"
    assert hasattr(coord, "process_hook_result"), "coord missing 'process_hook_result'"


def test_module_coordinator_import_from_coordinator_module():
    """from amplifier_core.coordinator import ModuleCoordinator works, has process_hook_result."""
    from amplifier_core.coordinator import ModuleCoordinator

    coord = ModuleCoordinator(_make_minimal_session())
    assert hasattr(coord, "process_hook_result"), (
        "ModuleCoordinator missing 'process_hook_result'"
    )


# ---------------------------------------------------------------------------
# _rust_wrappers removal test
# ---------------------------------------------------------------------------


def test_rust_wrappers_no_longer_exists():
    """import amplifier_core._rust_wrappers raises ImportError (module was deleted)."""
    with pytest.raises(ImportError):
        import amplifier_core._rust_wrappers  # noqa: F401


# ---------------------------------------------------------------------------
# RustSession config mutability test
# ---------------------------------------------------------------------------


def test_session_config_mutability():
    """RustSession config dict is mutable and mutations reflect on session.config."""
    from amplifier_core._engine import RustSession

    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config)

    # Mutate the config dict obtained from the session
    session.config["new_key"] = "new_value"

    # Verify mutation is reflected
    assert session.config.get("new_key") == "new_value", (
        "Mutation to session.config was not reflected"
    )
