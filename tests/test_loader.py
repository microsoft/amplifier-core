"""
Tests for ModuleLoader, specifically the _find_package_dir guard
against missing module root directories.

Regression test for the bug where calling _find_package_dir() on a
non-existent path raised an opaque FileNotFoundError (from .iterdir())
instead of returning None and letting _validate_module raise a clear
ModuleValidationError.
"""

from pathlib import Path

import pytest
from amplifier_core.loader import ModuleLoader
from amplifier_core.loader import ModuleValidationError


class TestFindPackageDirMissingPath:
    """Guard tests: _find_package_dir must never raise on a missing root."""

    def test_returns_none_for_nonexistent_path(self):
        """Core regression: a missing module root returns None, not FileNotFoundError."""
        loader = ModuleLoader()
        ghost = Path("/nonexistent/path/that/does/not/exist")

        assert not ghost.exists(), "pre-condition: path must not exist"

        result = loader._find_package_dir("provider-phantom", ghost)

        assert result is None

    def test_does_not_raise_file_not_found_error(self):
        """Explicit anti-regression: the old behaviour was to propagate FileNotFoundError."""
        loader = ModuleLoader()
        ghost = Path("/nonexistent/amplifier-module-hooks-insight-blocks-ee85c27df4b5d3df")

        try:
            loader._find_package_dir("hook-insight-blocks", ghost)
        except FileNotFoundError:
            pytest.fail(
                "_find_package_dir raised FileNotFoundError for a missing path; "
                "the existence guard is missing or broken."
            )

    def test_hash_style_cache_path_returns_none(self, tmp_path):
        """Paths that look like real cache dirs but don't exist also return None."""
        loader = ModuleLoader()
        # Mimic the actual path shape that triggered the original bug
        cache_root = tmp_path / "cache"
        missing_module = cache_root / "amplifier-module-hooks-insight-blocks-ee85c27df4b5d3df"
        # Intentionally do NOT create missing_module on disk

        result = loader._find_package_dir("hook-insight-blocks", missing_module)

        assert result is None

    def test_existing_empty_dir_returns_none(self, tmp_path):
        """An existing but empty directory still returns None (no package inside)."""
        loader = ModuleLoader()
        empty_dir = tmp_path / "amplifier-module-empty"
        empty_dir.mkdir()

        result = loader._find_package_dir("provider-empty", empty_dir)

        assert result is None

    def test_existing_dir_with_package_returns_package(self, tmp_path):
        """Happy-path: a correctly structured module directory returns the package dir."""
        loader = ModuleLoader()
        module_root = tmp_path / "amplifier-module-myprovider"
        module_root.mkdir()
        pkg_dir = module_root / "amplifier_module_myprovider"
        pkg_dir.mkdir()
        (pkg_dir / "__init__.py").write_text("# package\n")

        result = loader._find_package_dir("myprovider", module_root)

        assert result == pkg_dir


class TestValidateModuleProducesClearError:
    """
    Integration: _validate_module must surface a ModuleValidationError
    (not a raw FileNotFoundError) when the module root is missing.
    """

    @pytest.mark.asyncio
    async def test_validate_module_raises_module_validation_error_not_file_not_found(self):
        """
        When the module root directory doesn't exist, _validate_module should
        raise ModuleValidationError with an informative message — not the
        opaque FileNotFoundError that was propagating before the guard was added.
        """
        loader = ModuleLoader()
        ghost = Path("/nonexistent/amplifier-module-provider-ghost-aabbccdd")

        with pytest.raises(ModuleValidationError) as exc_info:
            await loader._validate_module("provider-ghost", ghost)

        msg = str(exc_info.value)
        assert "provider-ghost" in msg, f"Expected module id in error message, got: {msg!r}"
        assert "no valid Python package" in msg, f"Expected clear reason in message, got: {msg!r}"

    @pytest.mark.asyncio
    async def test_validate_module_error_message_contains_path(self):
        """The error message includes the missing path so operators can act on it."""
        loader = ModuleLoader()
        ghost = Path("/missing/amplifier-module-tool-calculator-deadbeef")

        with pytest.raises(ModuleValidationError) as exc_info:
            await loader._validate_module("tool-calculator", ghost)

        assert str(ghost) in str(exc_info.value)
