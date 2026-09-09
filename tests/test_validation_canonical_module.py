"""Regression tests for validation sharing the runtime module object."""

import importlib
import sys
from pathlib import Path

import pytest

from amplifier_core.loader import ModuleLoader
from amplifier_core.validation import ToolValidator


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


@pytest.mark.asyncio
async def test_loader_validation_and_runtime_share_canonical_package(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Validation must inspect the package the filesystem loader later mounts."""
    module_id = "tool-canonical"
    package_name = "amplifier_module_tool_canonical"
    package = _write_tool_package(tmp_path, package_name, "canonical")
    seen: dict[str, object] = {}
    original = ToolValidator._check_mount_exists

    def capture_validated_module(self, result, module):
        seen["module"] = module
        return original(self, result, module)

    monkeypatch.setattr(ToolValidator, "_check_mount_exists", capture_validated_module)
    loader = ModuleLoader(coordinator=_ResolverCoordinator(tmp_path))
    monkeypatch.setattr(loader, "_load_entry_point", lambda _module_id: None)

    try:
        mount = await loader.load(module_id)
        canonical = sys.modules[package_name]

        assert seen["module"] is canonical
        assert canonical.IMPORT_COUNT == 1

        runtime_coordinator = _RecordingCoordinator()
        await mount(runtime_coordinator)
        assert runtime_coordinator.mounted["stateful"].module_token is canonical.MODULE_TOKEN
        assert canonical.IMPORT_COUNT == 1
    finally:
        loader.cleanup()
        sys.modules.pop(package_name, None)


@pytest.mark.asyncio
async def test_path_validation_does_not_reuse_same_named_different_source(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A canonical package from another path must not substitute for the target."""
    package_name = "amplifier_module_tool_collision"
    first_root = tmp_path / "first"
    second_root = tmp_path / "second"
    _write_tool_package(first_root, package_name, "first")
    second_package = _write_tool_package(second_root, package_name, "second")
    seen: dict[str, object] = {}
    original = ToolValidator._check_mount_exists

    def capture_validated_module(self, result, module):
        seen["module"] = module
        return original(self, result, module)

    monkeypatch.setattr(ToolValidator, "_check_mount_exists", capture_validated_module)
    monkeypatch.syspath_prepend(str(first_root))

    try:
        canonical = importlib.import_module(package_name)
        result = await ToolValidator().validate(second_package)

        assert result.passed
        assert seen["module"] is not canonical
        assert Path(seen["module"].__file__).resolve() == (
            second_package / "__init__.py"
        ).resolve()
        assert seen["module"].StatefulTool.description == "second"
    finally:
        sys.modules.pop(package_name, None)


@pytest.mark.asyncio
async def test_path_only_validation_imports_requested_package(tmp_path: Path) -> None:
    """Standalone path validation remains valid when no package is loaded."""
    package_name = "amplifier_module_tool_standalone"
    package = _write_tool_package(tmp_path, package_name, "standalone")

    try:
        result = await ToolValidator().validate(package)
        assert result.passed
        assert package_name not in sys.modules
    finally:
        sys.modules.pop(package_name, None)


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