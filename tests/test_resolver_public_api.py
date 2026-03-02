"""Tests for resolver public API methods.

Addresses production audit finding: _bundle, _paths, _bundle_mappings
accessed via hasattr chains in session_spawner.py. This adds public
get_module_paths() and get_mention_mappings() methods.
"""

from amplifier_core.module_sources import (
    FileSystemModuleSource,
)


class TestResolverPublicAPI:
    """Tests for public resolver methods."""

    def test_filesystem_source_has_get_module_paths(self):
        """FileSystemModuleSource must have get_module_paths()."""
        source = FileSystemModuleSource(paths=["/tmp"])
        assert hasattr(source, "get_module_paths")

    def test_filesystem_source_get_module_paths_returns_list(self):
        """get_module_paths() must return a list."""
        source = FileSystemModuleSource(paths=["/tmp"])
        result = source.get_module_paths()
        assert isinstance(result, list)

    def test_filesystem_source_has_get_mention_mappings(self):
        """FileSystemModuleSource must have get_mention_mappings()."""
        source = FileSystemModuleSource(paths=["/tmp"])
        assert hasattr(source, "get_mention_mappings")

    def test_filesystem_source_get_mention_mappings_returns_dict(self):
        """get_mention_mappings() must return a dict."""
        source = FileSystemModuleSource(paths=["/tmp"])
        result = source.get_mention_mappings()
        assert isinstance(result, dict)
