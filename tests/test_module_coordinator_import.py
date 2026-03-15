"""
TDD test for task-8: __init__.py imports ModuleCoordinator from _engine.

Spec: line 17 of __init__.py must be:
    from ._engine import RustCoordinator as ModuleCoordinator

Not:
    from ._rust_wrappers import ModuleCoordinator
"""

import inspect


def test_module_coordinator_importable():
    """ModuleCoordinator must be importable from amplifier_core."""
    import amplifier_core

    assert hasattr(amplifier_core, "ModuleCoordinator"), (
        "ModuleCoordinator not found in amplifier_core namespace"
    )


def test_module_coordinator_is_rust_coordinator():
    """ModuleCoordinator must be the RustCoordinator class from _engine."""
    from amplifier_core import ModuleCoordinator
    from amplifier_core._engine import RustCoordinator

    assert ModuleCoordinator is RustCoordinator, (
        f"Expected ModuleCoordinator to be RustCoordinator from _engine, "
        f"but got: {ModuleCoordinator}"
    )


def test_module_coordinator_not_from_rust_wrappers():
    """ModuleCoordinator must NOT come from _rust_wrappers (which is deleted)."""
    from amplifier_core import ModuleCoordinator

    module_origin = getattr(ModuleCoordinator, "__module__", "")
    assert "_rust_wrappers" not in module_origin, (
        f"ModuleCoordinator still comes from _rust_wrappers: {module_origin}"
    )


def test_init_py_imports_from_engine_not_rust_wrappers():
    """The __init__.py source must not contain the old _rust_wrappers import for ModuleCoordinator."""
    import amplifier_core

    source_file = inspect.getfile(amplifier_core)
    with open(source_file) as f:
        content = f.read()

    # Must not import ModuleCoordinator from _rust_wrappers (old form)
    assert "from ._rust_wrappers import ModuleCoordinator" not in content, (
        "__init__.py still contains old import from _rust_wrappers"
    )
    # Must import from _engine (allow both single-line and ruff-formatted multi-line)
    from_engine_single = "from ._engine import RustCoordinator as ModuleCoordinator"
    # ruff may format it as a parenthesized import
    has_rust_coordinator_as_module_coordinator = (
        from_engine_single in content
        or "RustCoordinator as ModuleCoordinator" in content
    )
    assert has_rust_coordinator_as_module_coordinator, (
        "__init__.py missing: import of RustCoordinator as ModuleCoordinator from _engine"
    )
