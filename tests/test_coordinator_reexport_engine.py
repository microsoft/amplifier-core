"""
TDD test for task-9: coordinator.py re-exports ModuleCoordinator from _engine.

Spec: coordinator.py must contain:
    from amplifier_core._engine import RustCoordinator as ModuleCoordinator

Not:
    from amplifier_core._rust_wrappers import ...  (deleted)
"""

import inspect


def test_module_coordinator_importable_from_coordinator_module():
    """ModuleCoordinator must be importable from amplifier_core.coordinator."""
    from amplifier_core.coordinator import ModuleCoordinator

    assert ModuleCoordinator is not None


def test_coordinator_module_exposes_module_coordinator_in_all():
    """amplifier_core.coordinator.__all__ must include ModuleCoordinator."""
    import amplifier_core.coordinator as coord_mod

    assert hasattr(coord_mod, "__all__")
    assert "ModuleCoordinator" in coord_mod.__all__


def test_coordinator_module_coordinator_is_rust_coordinator():
    """ModuleCoordinator from coordinator.py must be RustCoordinator from _engine."""
    from amplifier_core.coordinator import ModuleCoordinator
    from amplifier_core._engine import RustCoordinator

    assert ModuleCoordinator is RustCoordinator, (
        f"Expected ModuleCoordinator to be RustCoordinator from _engine, "
        f"but got: {ModuleCoordinator}"
    )


def test_coordinator_py_does_not_import_from_rust_wrappers():
    """coordinator.py source must not import from _rust_wrappers (deleted module)."""
    import amplifier_core.coordinator as coord_mod

    source_file = inspect.getfile(coord_mod)
    with open(source_file) as f:
        content = f.read()

    assert "_rust_wrappers" not in content, (
        "coordinator.py still references deleted _rust_wrappers module: "
        f"found in {source_file}"
    )


def test_coordinator_py_imports_from_engine():
    """coordinator.py source must import RustCoordinator from _engine."""
    import amplifier_core.coordinator as coord_mod

    source_file = inspect.getfile(coord_mod)
    with open(source_file) as f:
        content = f.read()

    assert (
        "from amplifier_core._engine import RustCoordinator as ModuleCoordinator"
        in content
    ), (
        "coordinator.py missing: "
        "'from amplifier_core._engine import RustCoordinator as ModuleCoordinator'"
    )
