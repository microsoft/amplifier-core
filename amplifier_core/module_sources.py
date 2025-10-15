"""Module source resolution system.

Provides flexible module sourcing through layered resolution supporting:
- Local development (file paths, workspace convention)
- Git-based remote modules
- Python package fallback
- Custom resolution strategies

Architecture:
- ModuleSource: Protocol for source types (file, git, package)
- ModuleSourceResolver: Protocol for resolution strategies
- StandardModuleSourceResolver: Reference 6-layer fallback implementation
"""

import hashlib
import logging
import os
import subprocess
from abc import ABC
from abc import abstractmethod
from pathlib import Path
from typing import Protocol

try:
    import yaml
except ImportError:
    yaml = None  # Optional dependency

logger = logging.getLogger(__name__)


# ============================================================================
# Exceptions
# ============================================================================


class ModuleNotFoundError(Exception):
    """Raised when a module cannot be found in any resolution layer."""

    pass


class ModuleLoadError(Exception):
    """Raised when a module is found but cannot be loaded."""

    pass


# ============================================================================
# ModuleSource Protocol and Implementations
# ============================================================================


class ModuleSource(ABC):
    """Base class for module sources.

    Implementations resolve to filesystem paths where modules can be imported.
    """

    @abstractmethod
    def resolve(self) -> Path:
        """Resolve source to filesystem path.

        Returns:
            Path to directory containing importable Python module

        Raises:
            ModuleNotFoundError: Source cannot be resolved
            OSError: Filesystem access error
        """
        pass


class FileSource(ModuleSource):
    """Local filesystem path source."""

    def __init__(self, path: str | Path):
        """Initialize with file path.

        Args:
            path: Absolute or relative path to module directory
        """
        if isinstance(path, str):
            # Handle file:// prefix
            if path.startswith("file://"):
                path = path[7:]
            path = Path(path)

        self.path = path.resolve()

    def resolve(self) -> Path:
        """Resolve to filesystem path."""
        if not self.path.exists():
            raise ModuleNotFoundError(f"Module path not found: {self.path}")

        if not self.path.is_dir():
            raise ModuleLoadError(f"Module path is not a directory: {self.path}")

        # Validate it's a Python module
        if not self._is_valid_module(self.path):
            raise ModuleLoadError(f"Path does not contain a valid Python module: {self.path}")

        return self.path

    def _is_valid_module(self, path: Path) -> bool:
        """Check if directory contains Python module."""
        return any(path.glob("**/*.py"))

    def __repr__(self) -> str:
        return f"FileSource({self.path})"


class GitSource(ModuleSource):
    """Git repository source with caching."""

    def __init__(self, url: str, ref: str = "main", subdirectory: str | None = None):
        """Initialize with git URL.

        Args:
            url: Git repository URL (without git+ prefix)
            ref: Branch, tag, or commit (default: main)
            subdirectory: Optional subdirectory within repo
        """
        self.url = url
        self.ref = ref
        self.subdirectory = subdirectory
        self.cache_dir = Path.home() / ".amplifier" / "module-cache"

    @classmethod
    def from_uri(cls, uri: str) -> "GitSource":
        """Parse git+https://... URI into GitSource.

        Format: git+https://github.com/org/repo@ref#subdirectory=path

        Args:
            uri: Git URI string

        Returns:
            GitSource instance

        Raises:
            ValueError: Invalid URI format
        """
        if not uri.startswith("git+"):
            raise ValueError(f"Git URI must start with 'git+': {uri}")

        # Remove git+ prefix
        uri = uri[4:]

        # Split on # for subdirectory
        subdirectory = None
        if "#subdirectory=" in uri:
            uri, sub_part = uri.split("#subdirectory=", 1)
            subdirectory = sub_part

        # Split on @ for ref
        ref = "main"
        if "@" in uri:
            # Find last @ (in case URL has @ in it)
            parts = uri.rsplit("@", 1)
            uri, ref = parts[0], parts[1]

        return cls(url=uri, ref=ref, subdirectory=subdirectory)

    def resolve(self) -> Path:
        """Resolve to cached git repository path.

        Returns:
            Path to cached module directory

        Raises:
            ModuleNotFoundError: Git clone failed
        """
        # Generate cache key
        cache_key = hashlib.sha256(f"{self.url}@{self.ref}".encode()).hexdigest()[:12]
        cache_path = self.cache_dir / cache_key / self.ref

        # Add subdirectory if specified
        final_path = cache_path / self.subdirectory if self.subdirectory else cache_path

        # Check cache
        if cache_path.exists() and self._is_valid_cache(cache_path):
            logger.debug(f"Using cached git module: {cache_path}")
            return final_path

        # Download
        logger.info(f"Downloading git module: {self.url}@{self.ref}")
        try:
            self._download_via_uv(cache_path)
        except subprocess.CalledProcessError as e:
            raise ModuleNotFoundError(f"Failed to download {self.url}@{self.ref}: {e}")

        if not final_path.exists():
            raise ModuleNotFoundError(f"Subdirectory not found after download: {self.subdirectory}")

        return final_path

    def _is_valid_cache(self, cache_path: Path) -> bool:
        """Check if cache directory contains valid module."""
        return any(cache_path.glob("**/*.py"))

    def _download_via_uv(self, target: Path) -> None:
        """Download git repo using uv.

        Args:
            target: Target directory for download

        Raises:
            subprocess.CalledProcessError: Download failed
        """
        target.parent.mkdir(parents=True, exist_ok=True)

        # Build git URL
        git_url = f"git+{self.url}@{self.ref}"
        if self.subdirectory:
            git_url += f"#subdirectory={self.subdirectory}"

        # Use uv to download
        cmd = [
            "uv",
            "pip",
            "install",
            "--target",
            str(target),
            "--no-deps",  # Don't install dependencies
            git_url,
        ]

        logger.debug(f"Running: {' '.join(cmd)}")
        subprocess.run(cmd, check=True, capture_output=True, text=True)

    def __repr__(self) -> str:
        sub = f"#{self.subdirectory}" if self.subdirectory else ""
        return f"GitSource({self.url}@{self.ref}{sub})"


class PackageSource(ModuleSource):
    """Installed Python package source."""

    def __init__(self, package_name: str):
        """Initialize with package name.

        Args:
            package_name: Python package name
        """
        self.package_name = package_name

    def resolve(self) -> Path:
        """Resolve to installed package path.

        Returns:
            Path to installed package

        Raises:
            ModuleNotFoundError: Package not installed
        """
        try:
            import importlib.metadata

            dist = importlib.metadata.distribution(self.package_name)
            # Get package location
            if dist.files:
                # Get first file's parent to find package root
                package_path = Path(dist.locate_file(dist.files[0])).parent
                return package_path
            # Fallback: use locate_file with empty string
            return Path(dist.locate_file(""))
        except importlib.metadata.PackageNotFoundError:
            raise ModuleNotFoundError(
                f"Package '{self.package_name}' not installed. Install with: uv pip install {self.package_name}"
            )

    def __repr__(self) -> str:
        return f"PackageSource({self.package_name})"


# ============================================================================
# ModuleSourceResolver Protocol and Reference Implementation
# ============================================================================


class ModuleSourceResolver(Protocol):
    """Protocol for module source resolution strategies.

    Implementations decide WHERE to find modules based on module ID.
    This is app-layer policy - different apps can use different strategies.
    """

    def resolve(self, module_id: str, profile_hint=None) -> ModuleSource:
        """Resolve module ID to a source.

        Args:
            module_id: Module identifier (e.g., "tool-bash")
            profile_hint: Optional hint from profile (app-defined format)

        Returns:
            ModuleSource that can be resolved to a path

        Raises:
            ModuleNotFoundError: Module cannot be found
        """
        ...


class StandardModuleSourceResolver:
    """Reference implementation with 6-layer fallback.

    Resolution order (first match wins):
    1. Environment variable (AMPLIFIER_MODULE_<ID>)
    2. Workspace convention (.amplifier/modules/<id>/)
    3. Project config (.amplifier/sources.yaml)
    4. User config (~/.amplifier/sources.yaml)
    5. Profile source (profile_hint)
    6. Installed package (amplifier-module-<id> or <id>)
    """

    def resolve(self, module_id: str, profile_hint=None) -> ModuleSource:
        """Resolve module through 6-layer fallback."""

        # Layer 1: Environment variable
        env_key = f"AMPLIFIER_MODULE_{module_id.upper().replace('-', '_')}"
        if env_value := os.getenv(env_key):
            logger.debug(f"[module:resolve] {module_id} -> env var ({env_value})")
            return self._parse_source(env_value, module_id)

        # Layer 2: Workspace convention
        if workspace_source := self._check_workspace(module_id):
            logger.debug(f"[module:resolve] {module_id} -> workspace")
            return workspace_source

        # Layer 3: Project configuration
        if project_source := self._read_yaml_source(Path(".amplifier/sources.yaml"), module_id):
            logger.debug(f"[module:resolve] {module_id} -> project config")
            return self._parse_source(project_source, module_id)

        # Layer 4: User configuration
        user_config = Path.home() / ".amplifier" / "sources.yaml"
        if user_source := self._read_yaml_source(user_config, module_id):
            logger.debug(f"[module:resolve] {module_id} -> user config")
            return self._parse_source(user_source, module_id)

        # Layer 5: Profile source
        if profile_hint:
            logger.debug(f"[module:resolve] {module_id} -> profile")
            return self._parse_source(profile_hint, module_id)

        # Layer 6: Installed package (fallback)
        logger.debug(f"[module:resolve] {module_id} -> package")
        return self._resolve_package(module_id)

    def _parse_source(self, source, module_id: str) -> ModuleSource:
        """Parse source (string URI or object) into ModuleSource.

        Args:
            source: String URI or dict (MCP-aligned object format)
            module_id: Module ID (for error messages)

        Returns:
            ModuleSource instance

        Raises:
            ValueError: Invalid source format
        """
        # Object format (MCP-aligned)
        if isinstance(source, dict):
            source_type = source.get("type")
            if source_type == "git":
                return GitSource(
                    url=source["url"], ref=source.get("ref", "main"), subdirectory=source.get("subdirectory")
                )
            if source_type == "file":
                return FileSource(source["path"])
            if source_type == "package":
                return PackageSource(source["name"])
            raise ValueError(f"Invalid source type '{source_type}' for module '{module_id}'")

        # String format
        source = str(source)

        if source.startswith("git+"):
            return GitSource.from_uri(source)
        if source.startswith("file://") or source.startswith("/") or source.startswith("."):
            return FileSource(source)
        # Assume package name
        return PackageSource(source)

    def _check_workspace(self, module_id: str) -> FileSource | None:
        """Check workspace convention for module.

        Args:
            module_id: Module identifier

        Returns:
            FileSource if found and valid, None otherwise
        """
        workspace_path = Path(".amplifier/modules") / module_id

        if not workspace_path.exists():
            return None

        # Check for empty submodule (has .git but no code)
        if self._is_empty_submodule(workspace_path):
            logger.debug(f"Module {module_id} workspace dir is empty submodule, skipping")
            return None

        # Check if valid module
        if not any(workspace_path.glob("**/*.py")):
            logger.warning(f"Module {module_id} in workspace but contains no Python files, skipping")
            return None

        return FileSource(workspace_path)

    def _is_empty_submodule(self, path: Path) -> bool:
        """Check if directory is uninitialized git submodule.

        Args:
            path: Directory to check

        Returns:
            True if empty submodule, False otherwise
        """
        # Has .git file (submodule marker) but no Python files
        git_file = path / ".git"
        return git_file.exists() and git_file.is_file() and not any(path.glob("**/*.py"))

    def _read_yaml_source(self, config_path: Path, module_id: str) -> str | dict | None:
        """Read module source from YAML config file.

        Args:
            config_path: Path to YAML config file
            module_id: Module identifier

        Returns:
            Source string/dict if found, None otherwise
        """
        if not config_path.exists():
            return None

        if yaml is None:
            logger.warning(f"PyYAML not installed, cannot read {config_path}")
            return None

        try:
            with open(config_path) as f:
                config = yaml.safe_load(f)

            if not config or "sources" not in config:
                return None

            return config["sources"].get(module_id)

        except Exception as e:
            logger.warning(f"Failed to read {config_path}: {e}")
            return None

    def _resolve_package(self, module_id: str) -> PackageSource:
        """Resolve to installed package using fallback logic.

        Tries:
        1. Exact module ID as package name
        2. amplifier-module-<id> convention

        Args:
            module_id: Module identifier

        Returns:
            PackageSource

        Raises:
            ModuleNotFoundError: Neither package exists
        """
        import importlib.metadata

        # Try exact ID
        try:
            importlib.metadata.distribution(module_id)
            return PackageSource(module_id)
        except importlib.metadata.PackageNotFoundError:
            pass

        # Try convention
        convention_name = f"amplifier-module-{module_id}"
        try:
            importlib.metadata.distribution(convention_name)
            return PackageSource(convention_name)
        except importlib.metadata.PackageNotFoundError:
            pass

        # Both failed
        raise ModuleNotFoundError(
            f"Module '{module_id}' not found\n\n"
            f"Resolution attempted:\n"
            f"  1. Environment: AMPLIFIER_MODULE_{module_id.upper().replace('-', '_')} (not set)\n"
            f"  2. Workspace: .amplifier/modules/{module_id} (not found)\n"
            f"  3. Project: .amplifier/sources.yaml (no entry)\n"
            f"  4. User: ~/.amplifier/sources.yaml (no entry)\n"
            f"  5. Profile: (no source specified)\n"
            f"  6. Package: Tried '{module_id}' and '{convention_name}' (neither installed)\n\n"
            f"Suggestions:\n"
            f"  - Add source to profile: source: git+https://...\n"
            f"  - Install package: uv pip install <package-name>\n"
            f"  - Link local version: amplifier module link {module_id} /path"
        )

    def __repr__(self) -> str:
        return "StandardModuleSourceResolver(6-layer)"


# ============================================================================
# Entry Point Resolver (Kernel Fallback)
# ============================================================================


class EntryPointResolver:
    """Kernel-provided fallback using Python entry points.

    Used when no custom resolver is mounted.
    """

    def resolve(self, module_id: str, profile_hint=None) -> ModuleSource:
        """Resolve using entry points.

        Args:
            module_id: Module identifier
            profile_hint: Ignored (not used in entry point resolution)

        Returns:
            PackageSource for module

        Raises:
            ModuleNotFoundError: Entry point not found
        """
        import importlib.metadata

        # Try to find entry point
        try:
            eps = importlib.metadata.entry_points(group="amplifier.modules")
            # Look for entry point with name = module_id
            for ep in eps:
                if ep.name == module_id:
                    # Return package source
                    return PackageSource(ep.dist.name)

            # Not found
            raise ModuleNotFoundError(
                f"Module '{module_id}' not found in entry points. "
                f"Install with: uv pip install amplifier-module-{module_id}"
            )

        except Exception as e:
            raise ModuleNotFoundError(f"Entry point lookup failed for '{module_id}': {e}")

    def __repr__(self) -> str:
        return "EntryPointResolver(kernel-default)"
