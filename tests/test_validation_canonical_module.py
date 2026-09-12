"""Regression tests for validation sharing the runtime module object."""

import importlib
import importlib.metadata
import sys
import threading
import types
from pathlib import Path

import pytest

from amplifier_core.loader import ModuleLoader
from amplifier_core.loader import ModuleValidationError
from amplifier_core.validation import ToolValidator
from amplifier_core.validation.base import import_module_from_path


class _Source:
    def __init__(self, root: Path) -> None:
        self.root = root

    def resolve(self) -> Path:
        return self.root


class _Resolver:
    def __init__(self, root: Path) -> None:
        self.source = _Source(root)

    def resolve(
        self,
        module_id: str,
        source_hint: str | dict | None = None,
        profile_hint: str | dict | None = None,
    ) -> _Source:
        return self.source


class _ResolverCoordinator:
    def __init__(self, root: Path) -> None:
        self.resolver = _Resolver(root)

    def get(self, mount_point: str) -> _Resolver:
        assert mount_point == "module-source-resolver"
        return self.resolver


class _RecordingCoordinator:
    def __init__(self) -> None:
        self.mounted: dict[str, object] = {}

    async def mount(self, mount_point: str, module: object, name: str | None = None) -> None:
        assert mount_point == "tools"
        assert name is not None
        self.mounted[name] = module


def _write_tool_package(root: Path, package_name: str, token: str) -> Path:
    package = root / package_name
    package.mkdir(parents=True)
    (package / "marker.py").write_text(f'TOKEN = "{token}"\n')
    (package / "__init__.py").write_text(
        f"""
from .marker import TOKEN

IMPORT_COUNT = globals().get("IMPORT_COUNT", 0) + 1
MODULE_TOKEN = object()
__amplifier_module_type__ = "tool"

class StatefulTool:
    name = "stateful"
    description = TOKEN
    module_token = MODULE_TOKEN

    async def execute(self, input):
        return {{}}

async def mount(coordinator, config):
    await coordinator.mount("tools", StatefulTool(), name="stateful")
"""
    )
    return package


def _clear_package(package_name: str) -> None:
    for name in list(sys.modules):
        if name == package_name or name.startswith(f"{package_name}."):
            sys.modules.pop(name, None)


def _write_conflicting_entry_point(root: Path, module_id: str) -> None:
    (root / "installed_entry_point.py").write_text(
        """
async def mount(coordinator, config):
    await coordinator.mount("tools", object(), name="installed-entry-point")
"""
    )
    dist_info = root / "conflicting_entry_point-1.0.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text("Name: conflicting-entry-point\nVersion: 1.0\n")
    (dist_info / "entry_points.txt").write_text(
        f"[amplifier.modules]\n{module_id} = installed_entry_point:mount\n"
    )


@pytest.mark.asyncio
async def test_loader_validation_and_runtime_share_canonical_package(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A source-resolved module wins over an installed conflicting entry point."""
    module_id = "tool-canonical"
    package_name = "amplifier_module_tool_canonical"
    source_root = tmp_path / "source"
    package = _write_tool_package(source_root, package_name, "canonical")
    entry_point_root = tmp_path / "entry-point"
    entry_point_root.mkdir()
    _write_conflicting_entry_point(entry_point_root, module_id)
    seen: dict[str, object] = {}
    original = ToolValidator._check_mount_exists

    def capture_validated_module(self, result, module):
        seen["module"] = module
        return original(self, result, module)

    monkeypatch.setattr(ToolValidator, "_check_mount_exists", capture_validated_module)
    monkeypatch.syspath_prepend(str(entry_point_root))
    loader = ModuleLoader(coordinator=_ResolverCoordinator(source_root))

    try:
        assert any(
            entry_point.name == module_id
            for entry_point in importlib.metadata.entry_points(group="amplifier.modules")
        )
        mount = await loader.load(module_id)
        canonical = sys.modules[package_name]

        assert seen["module"] is canonical
        assert canonical.IMPORT_COUNT == 1

        runtime_coordinator = _RecordingCoordinator()
        await mount(runtime_coordinator)
        assert runtime_coordinator.mounted["stateful"].module_token is canonical.MODULE_TOKEN
        assert "installed-entry-point" not in runtime_coordinator.mounted
        assert "installed_entry_point" not in sys.modules
        assert canonical.IMPORT_COUNT == 1
    finally:
        loader.cleanup()
        _clear_package(package_name)


@pytest.mark.asyncio
async def test_path_validation_rejects_same_named_different_source_without_cache_mutation(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A conflicting source fails without replacing the canonical package tree."""
    package_name = "amplifier_module_tool_collision"
    first_root = tmp_path / "first"
    second_root = tmp_path / "second"
    _write_tool_package(first_root, package_name, "first")
    second_package = _write_tool_package(second_root, package_name, "second")

    monkeypatch.syspath_prepend(str(first_root))

    try:
        canonical = importlib.import_module(package_name)
        canonical_marker = sys.modules[f"{package_name}.marker"]
        result = await ToolValidator().validate(second_package)

        assert not result.passed
        assert any(
            check.name == "module_importable"
            and not check.passed
            and "Refusing to import" in check.message
            for check in result.checks
        )
        assert sys.modules[package_name] is canonical
        assert sys.modules[f"{package_name}.marker"] is canonical_marker
        assert canonical.StatefulTool.description == "first"
    finally:
        _clear_package(package_name)


@pytest.mark.asyncio
async def test_path_validation_rejects_a_conflicting_cached_submodule(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A dangling child cache cannot be mixed into a requested package source."""
    package_name = "amplifier_module_tool_submodule_collision"
    first_root = tmp_path / "first"
    second_root = tmp_path / "second"
    _write_tool_package(first_root, package_name, "first")
    second_package = _write_tool_package(second_root, package_name, "second")
    monkeypatch.syspath_prepend(str(first_root))

    try:
        importlib.import_module(package_name)
        cached_marker = sys.modules[f"{package_name}.marker"]
        sys.modules.pop(package_name)

        result = await ToolValidator().validate(second_package)

        assert not result.passed
        assert any(
            check.name == "module_importable"
            and not check.passed
            and "cached submodule" in check.message
            for check in result.checks
        )
        assert package_name not in sys.modules
        assert sys.modules[f"{package_name}.marker"] is cached_marker
        assert cached_marker.TOKEN == "first"
    finally:
        _clear_package(package_name)


@pytest.mark.asyncio
async def test_source_resolved_cache_rejects_a_different_source(
    tmp_path: Path,
) -> None:
    """A cached source-resolved module cannot silently mount a later source."""
    module_id = "tool-cache-collision"
    package_name = "amplifier_module_tool_cache_collision"
    first_root = tmp_path / "first"
    second_root = tmp_path / "second"
    _write_tool_package(first_root, package_name, "first")
    _write_tool_package(second_root, package_name, "second")
    coordinator = _ResolverCoordinator(first_root)
    loader = ModuleLoader(coordinator=coordinator)

    try:
        await loader.load(module_id)
        coordinator.resolver.source.root = second_root

        with pytest.raises(ImportError, match="already loaded from"):
            await loader.load(module_id)

        assert Path(sys.modules[package_name].__file__).resolve() == (
            first_root / package_name / "__init__.py"
        ).resolve()
    finally:
        loader.cleanup()
        _clear_package(package_name)


@pytest.mark.asyncio
async def test_matching_root_with_foreign_cached_child_cannot_mount(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A cached canonical root does not exempt its children from source checks."""
    module_id = "tool-mixed-cache"
    package_name = "amplifier_module_tool_mixed_cache"
    source_root = tmp_path / "source"
    _write_tool_package(source_root, package_name, "canonical")
    foreign_package = _write_tool_package(tmp_path / "foreign", package_name, "foreign")
    monkeypatch.syspath_prepend(str(source_root))
    loader = ModuleLoader(coordinator=_ResolverCoordinator(source_root))

    try:
        canonical = importlib.import_module(package_name)
        foreign_child = types.ModuleType(f"{package_name}.marker")
        foreign_child.__file__ = str(foreign_package / "marker.py")
        sys.modules[f"{package_name}.marker"] = foreign_child

        with pytest.raises(ModuleValidationError, match="cached submodule"):
            await loader.load(module_id)

        assert sys.modules[package_name] is canonical
        assert sys.modules[f"{package_name}.marker"] is foreign_child
        assert module_id not in loader._loaded_modules
        assert canonical.IMPORT_COUNT == 1
    finally:
        loader.cleanup()
        _clear_package(package_name)


@pytest.mark.asyncio
async def test_direct_cache_does_not_override_a_later_source_resolution(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A direct cache cannot silently satisfy a later conflicting source request."""
    module_id = "tool-direct-cache"
    package_name = "amplifier_module_tool_direct_cache"
    direct_root = tmp_path / "direct"
    source_root = tmp_path / "source"
    _write_tool_package(direct_root, package_name, "direct")
    _write_tool_package(source_root, package_name, "source")
    monkeypatch.syspath_prepend(str(direct_root))
    loader = ModuleLoader()

    try:
        await loader.load(module_id)
        loader._coordinator = _ResolverCoordinator(source_root)

        with pytest.raises(ModuleValidationError, match="Refusing to import"):
            await loader.load(module_id)

        assert Path(sys.modules[package_name].__file__).resolve() == (
            direct_root / package_name / "__init__.py"
        ).resolve()
    finally:
        loader.cleanup()
        _clear_package(package_name)


@pytest.mark.asyncio
async def test_source_resolved_fallback_package_is_mounted_from_validated_source(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A fallback package name remains the identity used for runtime mounting."""
    module_id = "tool-beta"
    package_name = "amplifier_module_alpha"
    package = _write_tool_package(tmp_path, package_name, "source")
    installed_root = tmp_path / "installed"
    installed_package = installed_root / "amplifier_module_tool_beta"
    installed_package.mkdir(parents=True)
    (installed_package / "__init__.py").write_text(
        '__amplifier_module_type__ = "provider"\n'
    )
    monkeypatch.syspath_prepend(str(installed_root))
    loader = ModuleLoader(coordinator=_ResolverCoordinator(tmp_path))

    try:
        mount = await loader.load(module_id)

        runtime_coordinator = _RecordingCoordinator()
        await mount(runtime_coordinator)
        assert runtime_coordinator.mounted["stateful"].description == "source"
        assert "amplifier_module_tool_beta" not in sys.modules
        assert Path(sys.modules[package_name].__file__).resolve() == (
            package / "__init__.py"
        ).resolve()
    finally:
        loader.cleanup()
        _clear_package(package_name)
        _clear_package("amplifier_module_tool_beta")


@pytest.mark.asyncio
async def test_path_only_validation_imports_requested_package(tmp_path: Path) -> None:
    """Standalone path validation remains valid when no package is loaded."""
    package_name = "amplifier_module_tool_standalone"
    package = _write_tool_package(tmp_path, package_name, "standalone")

    try:
        result = await ToolValidator().validate(package)
        assert result.passed
        assert Path(sys.modules[package_name].__file__).resolve() == (
            package / "__init__.py"
        ).resolve()
        assert sys.modules[f"{package_name}.marker"].TOKEN == "standalone"
    finally:
        _clear_package(package_name)


def test_standalone_path_validation_prioritizes_requested_source(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The requested path wins even when it already trails another source path."""
    package_name = "amplifier_module_tool_path_priority"
    installed_root = tmp_path / "installed"
    requested_root = tmp_path / "requested"
    _write_tool_package(installed_root, package_name, "installed")
    requested_package = _write_tool_package(requested_root, package_name, "requested")
    injected_path = str(tmp_path / "added-by-module")
    init_file = requested_package / "__init__.py"
    init_file.write_text(
        f"import sys\nsys.path.insert(0, {injected_path!r})\n" + init_file.read_text()
    )
    monkeypatch.syspath_prepend(str(installed_root))
    sys.path.append(str(requested_root))
    original_path = list(sys.path)

    try:
        module = import_module_from_path(requested_package)

        assert module.StatefulTool.description == "requested"
        assert sys.path == [injected_path, *original_path]
    finally:
        _clear_package(package_name)
        sys.path.remove(injected_path)
        sys.path.remove(str(requested_root))


def test_standalone_path_validation_removes_reconstructed_import_path(
    tmp_path: Path,
) -> None:
    """Cleanup works when the imported package recreates sys.path strings."""
    package_name = "amplifier_module_tool_path_reconstruction"
    import_root = tmp_path / "requested"
    package = import_root / package_name
    package.mkdir(parents=True)
    (package / "__init__.py").write_text(
        "import sys\n"
        "sys.path[:] = [entry.encode().decode() for entry in sys.path]\n"
    )

    try:
        module = import_module_from_path(package)

        assert module.__name__ == package_name
        assert str(import_root) not in sys.path
    finally:
        _clear_package(package_name)


def test_standalone_path_validation_does_not_expose_temporary_import_path(
    tmp_path: Path,
) -> None:
    """Normal imports block until the standalone validation path is removed."""
    package_name = "amplifier_module_tool_blocking"
    package = tmp_path / package_name
    package.mkdir()
    entered = threading.Event()
    release = threading.Event()
    synchronizer = types.ModuleType("_validation_import_synchronizer")
    synchronizer.entered = entered
    synchronizer.release = release
    sys.modules[synchronizer.__name__] = synchronizer
    (package / "__init__.py").write_text(
        """
from _validation_import_synchronizer import entered, release

entered.set()
assert release.wait(timeout=5)
"""
    )
    (tmp_path / "unrelated.py").write_text("VALUE = 'must-not-import'\n")
    validation_error: list[Exception] = []
    unrelated_result: list[object] = []
    unrelated_done = threading.Event()

    def validate() -> None:
        try:
            import_module_from_path(package)
        except Exception as error:
            validation_error.append(error)

    def import_unrelated() -> None:
        try:
            unrelated_result.append(importlib.import_module("unrelated"))
        except Exception as error:
            unrelated_result.append(error)
        finally:
            unrelated_done.set()

    validation_thread = threading.Thread(target=validate)
    validation_thread.start()
    assert entered.wait(timeout=5)
    unrelated_thread = threading.Thread(target=import_unrelated)
    unrelated_thread.start()
    assert not unrelated_done.wait(timeout=0.1)
    release.set()
    validation_thread.join(timeout=5)
    unrelated_thread.join(timeout=5)

    try:
        assert not validation_thread.is_alive()
        assert not unrelated_thread.is_alive()
        assert not validation_error
        assert isinstance(unrelated_result[0], ModuleNotFoundError)
    finally:
        _clear_package(package_name)
        sys.modules.pop(synchronizer.__name__, None)
        sys.modules.pop("unrelated", None)


@pytest.mark.asyncio
async def test_path_validation_rejects_malformed_module(tmp_path: Path) -> None:
    """Reusing canonical modules does not turn malformed source into a pass."""
    package = tmp_path / "amplifier_module_tool_malformed"
    package.mkdir()
    (package / "__init__.py").write_text("async def mount(:\n")

    result = await ToolValidator().validate(package)

    assert not result.passed
    assert any(
        check.name == "module_importable" and not check.passed
        for check in result.checks
    )