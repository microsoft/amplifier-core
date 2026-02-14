"""
Pytest plugin for Amplifier module validation.

Enables modules to run behavioral validation tests as part of their normal pytest suite.
Auto-detects module type from directory structure and provides necessary fixtures.

Usage:
    In a module repo, tests automatically get:
    - `module_path` fixture: Path to the module's Python package
    - `module_type` fixture: Detected type (provider, tool, hook, etc.)
    - `coordinator` fixture: TestCoordinator for mounting modules
    - `provider_module`, `tool_module`, etc.: Mounted module instances

    Modules can inherit from base test classes:
    ```python
    from amplifier_core.validation.behavioral import ProviderBehaviorTests

    class TestMyProviderBehavior(ProviderBehaviorTests):
        pass  # Inherits all standard tests
    ```

The plugin detects modules by looking for:
    1. Current directory named `amplifier-module-{type}-{name}`
    2. Or a subdirectory named `amplifier_module_{type}_{name}`
"""

import importlib
import importlib.util
import inspect
import re
from collections.abc import AsyncGenerator
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest
import pytest_asyncio


def _detect_module_info(start_path: Path) -> tuple[Path | None, str | None]:
    """
    Detect module path and type from directory structure.

    Looks for:
    - amplifier-module-{type}-{name} parent directories
    - amplifier_module_{type}_{name} Python package directories

    Returns:
        Tuple of (module_path, module_type) or (None, None) if not detected
    """
    # Pattern for module directory names
    dir_pattern = re.compile(r"amplifier-module-(\w+)-")
    pkg_pattern = re.compile(r"amplifier_module_(\w+)_")

    # Check current directory name
    if dir_pattern.match(start_path.name):
        match = dir_pattern.match(start_path.name)
        if match:
            module_type = match.group(1)
            # Find the Python package
            for child in start_path.iterdir():
                if child.is_dir() and pkg_pattern.match(child.name):
                    return child, module_type

    # Check parent directories
    for parent in start_path.parents:
        if dir_pattern.match(parent.name):
            match = dir_pattern.match(parent.name)
            if match:
                module_type = match.group(1)
                for child in parent.iterdir():
                    if child.is_dir() and pkg_pattern.match(child.name):
                        return child, module_type

    # Check for Python package in current directory
    for child in start_path.iterdir():
        if child.is_dir() and pkg_pattern.match(child.name):
            match = pkg_pattern.match(child.name)
            if match:
                return child, match.group(1)

    return None, None


def _normalize_module_type(raw_type: str | None) -> str | None:
    """Normalize module type to canonical form."""
    if not raw_type:
        return None

    # Map variations to canonical types
    type_mappings = {
        "hooks": "hook",
        "hook": "hook",
        "loop": "orchestrator",
        "orchestrator": "orchestrator",
        "provider": "provider",
        "tool": "tool",
        "context": "context",
    }

    return type_mappings.get(raw_type, raw_type)


def _infer_type_from_name(name: str) -> str | None:
    """Infer module type from directory/package name."""
    type_patterns = {
        "provider": ["provider"],
        "tool": ["tool"],
        "hook": ["hooks", "hook"],
        "orchestrator": ["loop", "orchestrator"],
        "context": ["context"],
    }

    for module_type, patterns in type_patterns.items():
        for pattern in patterns:
            if pattern in name:
                return module_type
    return None


class AmplifierModulePlugin:
    """Pytest plugin for Amplifier module validation."""

    def __init__(self) -> None:
        self.module_path: Path | None = None
        self.module_type: str | None = None
        self._detected = False

    def detect(self, config: Any) -> None:
        """Detect module info from pytest invocation context."""
        if self._detected:
            return
        self._detected = True

        # Try multiple detection strategies
        detection_paths = [
            Path.cwd(),  # Current working directory
            Path(config.rootdir),  # Pytest rootdir
        ]

        # Also check test paths from config.args
        for arg in config.args:
            arg_path = Path(arg)
            if arg_path.exists():
                if arg_path.is_file():
                    detection_paths.append(arg_path.parent)
                else:
                    detection_paths.append(arg_path)

        # Try each path until we find a module
        for path in detection_paths:
            self.module_path, self.module_type = _detect_module_info(path)
            if self.module_path:
                break

        # Also try to infer type from path if detection didn't find it
        if self.module_path and not self.module_type:
            self.module_type = _infer_type_from_name(str(self.module_path))

        # Normalize the module type (hooks -> hook, loop -> orchestrator, etc.)
        self.module_type = _normalize_module_type(self.module_type)


# Global plugin instance
_plugin = AmplifierModulePlugin()


def pytest_addoption(parser: Any) -> None:
    """Register pytest command-line options."""
    parser.addoption(
        "--module-path",
        action="store",
        default=None,
        help="Path to module directory for behavioral validation",
    )


def pytest_configure(config: Any) -> None:
    """Configure the plugin when pytest starts."""
    _plugin.detect(config)

    # Register markers
    config.addinivalue_line(
        "markers",
        "module_validation: mark test as module validation test",
    )


@pytest.fixture
def module_path(request: Any) -> Path | None:
    """
    Provide the path to the module under test.

    Auto-detected from the test file's directory structure.
    Returns None if not in a module directory.
    Can be overridden by --module-path CLI option.

    Supports pattern:
    - amplifier-module-{type}-{name}/ (standalone modules)
    """
    # Check for CLI override first
    cli_path = request.config.getoption("--module-path", default=None)
    if cli_path:
        return Path(cli_path)

    # Detect module path from the test file's location
    # This allows running tests from multiple modules in a single pytest run
    test_file = Path(request.fspath)
    test_dir = test_file.parent

    # Walk up to find module root
    current = test_dir
    while current.parent != current:
        # Check for amplifier-module-* naming pattern (standalone modules)
        if current.name.startswith("amplifier-module-"):
            # Found module root, now find the Python package inside
            # Look for amplifier_module_* or amplifier_* package (not tests, etc.)
            for child in current.iterdir():
                if child.is_dir() and child.name.startswith("amplifier_"):
                    init_file = child / "__init__.py"
                    if init_file.exists():
                        return child
            break



        current = current.parent

    # Fall back to global detection if test file-based detection fails
    return _plugin.module_path


@pytest.fixture
def module_type(request: Any) -> str | None:
    """
    Provide the type of module under test.

    Auto-detected from directory name (provider, tool, hook, orchestrator, context).

    Supports pattern:
    - amplifier-module-{type}-{name}/ (standalone modules)
    """
    # Detect module type from the test file's location
    test_file = Path(request.fspath)
    test_dir = test_file.parent

    type_map = {
        "provider": "provider",
        "tool": "tool",
        "hooks": "hook",
        "loop": "orchestrator",
        "context": "context",
    }

    # Walk up to find module root
    current = test_dir
    while current.parent != current:
        name = current.name
        if name.startswith("amplifier-module-"):
            # Extract type from directory name
            # Pattern: amplifier-module-{type}-{name} or amplifier-module-{type}
            suffix = name[len("amplifier-module-") :]
            parts = suffix.split("-", 1)
            if parts:
                return type_map.get(parts[0])



        current = current.parent

    # Fall back to global detection
    return _plugin.module_type


@pytest.fixture
def is_module_context() -> bool:
    """Return True if running within a detected Amplifier module."""
    return _plugin.module_path is not None


def pytest_collection_modifyitems(
    session: Any,
    config: Any,
    items: list[Any],
) -> None:
    """
    Modify test collection based on module context.

    When running in a module directory:
    - Skip behavioral tests for other module types
    - Auto-skip tests that require module_path if not in module context
    """
    if not _plugin.module_path:
        # Not in a module context - skip all tests that need module_path
        skip_marker = pytest.mark.skip(reason="Not running in Amplifier module context")
        for item in items:
            # Skip behavioral tests from amplifier-core that need module_path
            if "module_path" in getattr(item, "fixturenames", []) and "amplifier_core/validation/behavioral" in str(
                item.fspath
            ):
                item.add_marker(skip_marker)
        return

    # In a module context - filter behavioral tests by type
    detected_type = _plugin.module_type
    if not detected_type:
        return

    # Map module types to their test file names
    type_to_test_file = {
        "provider": "test_provider.py",
        "tool": "test_tool.py",
        "hook": "test_hook.py",
        "orchestrator": "test_orchestrator.py",
        "context": "test_context.py",
    }

    expected_test_file = type_to_test_file.get(detected_type)
    if not expected_test_file:
        return

    skip_wrong_type = pytest.mark.skip(reason=f"Test for different module type (detected: {detected_type})")

    for item in items:
        # Only filter behavioral tests from amplifier-core
        if "amplifier_core/validation/behavioral" not in str(item.fspath):
            continue

        test_filename = Path(item.fspath).name

        # Skip tests for other module types
        if test_filename.startswith("test_") and test_filename != expected_test_file:
            item.add_marker(skip_wrong_type)


# =============================================================================
# Behavioral Test Fixtures
# =============================================================================
# These fixtures support the inherited behavioral test pattern where modules
# inherit from base test classes (e.g., ProviderBehaviorTests) and the fixtures
# are provided by this plugin.


async def _load_module(
    module_path: Path,
    coordinator: Any,
    config: dict[str, Any] | None = None,
) -> Callable[[], None] | None:
    """
    Load a module dynamically and call its mount() function.

    Args:
        module_path: Path to module directory
        coordinator: Test coordinator to mount into
        config: Optional configuration dict

    Returns:
        Cleanup function if mount() returned one, None otherwise
    """
    if config is None:
        config = {}

    path = Path(module_path)
    if not path.exists():
        raise FileNotFoundError(f"Module path not found: {path}")

    # Load the module
    if path.is_dir():
        init_file = path / "__init__.py"
        if not init_file.exists():
            raise FileNotFoundError(f"No __init__.py found in {path}")
        spec = importlib.util.spec_from_file_location(path.name, init_file)
    else:
        spec = importlib.util.spec_from_file_location(path.stem, path)

    if spec is None or spec.loader is None:
        raise ImportError(f"Could not load spec for {path}")

    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    # Get and call mount()
    mount_fn = getattr(module, "mount", None)
    if mount_fn is None:
        raise AttributeError("Module has no mount() function")

    result = await mount_fn(coordinator, config)
    if callable(result):
        cleanup: Callable[[], None] = result  # type: ignore[assignment]
        return cleanup
    return None


@pytest.fixture
def coordinator() -> Any:
    """Create a fresh test coordinator for module testing."""
    from amplifier_core.testing import TestCoordinator

    return TestCoordinator()


@pytest.fixture
def mock_deps(coordinator: Any) -> tuple[Any, dict[str, Any], dict[str, Any], Any]:
    """Bundle mock dependencies for orchestrator tests."""
    from amplifier_core.testing import EventRecorder
    from amplifier_core.testing import MockContextManager
    from amplifier_core.testing import MockTool

    mock_context = MockContextManager()
    mock_tool = MockTool(name="test_tool", output="test result")
    event_recorder = EventRecorder()

    # Create a mock provider that returns scripted responses
    class MockProvider:
        """Minimal mock provider for orchestrator testing."""

        name = "mock"

        def get_info(self) -> Any:
            from amplifier_core.models import ProviderInfo

            return ProviderInfo(id="mock", display_name="Mock Provider")

        async def list_models(self) -> list[Any]:
            return []

        async def complete(self, request: Any, **kwargs: Any) -> Any:
            from amplifier_core.message_models import ChatResponse
            from amplifier_core.message_models import TextBlock

            return ChatResponse(
                content=[TextBlock(text="Mock response")],
            )

        def parse_tool_calls(self, response: Any) -> list[Any]:
            return []

    return (
        mock_context,
        {"default": MockProvider()},
        {"test_tool": mock_tool},
        event_recorder,
    )


@pytest_asyncio.fixture
async def provider_module(
    module_path: Path | None,
    coordinator: Any,
) -> AsyncGenerator[Any, None]:
    """
    Load and return a provider module for testing.

    Skips test if no module path detected.
    Uses yield pattern for proper async cleanup.
    """
    if module_path is None:
        pytest.skip("No module path detected")

    cleanup = await _load_module(module_path, coordinator)

    # Get the mounted provider
    providers = coordinator.mount_points.get("providers", {})
    if not providers:
        pytest.fail("No provider was mounted")

    # Yield first provider for testing
    yield next(iter(providers.values()))

    # Cleanup after test (handles both sync and async cleanup functions)
    if cleanup:
        if inspect.iscoroutinefunction(cleanup):
            await cleanup()
        else:
            cleanup()


@pytest_asyncio.fixture
async def tool_module(
    module_path: Path | None,
    coordinator: Any,
) -> AsyncGenerator[Any, None]:
    """
    Load and return a tool module for testing.

    Skips test if no module path detected.
    Uses yield pattern for proper async cleanup.
    """
    if module_path is None:
        pytest.skip("No module path detected")

    cleanup = await _load_module(module_path, coordinator)

    # Get the mounted tool
    tools = coordinator.mount_points.get("tools", {})
    if not tools:
        pytest.fail("No tool was mounted")

    # Yield first tool for testing
    yield next(iter(tools.values()))

    # Cleanup after test (handles both sync and async cleanup functions)
    if cleanup:
        if inspect.iscoroutinefunction(cleanup):
            await cleanup()
        else:
            cleanup()


@pytest_asyncio.fixture
async def hook_cleanup(
    module_path: Path | None,
    coordinator: Any,
) -> AsyncGenerator[Callable[[], None] | None, None]:
    """
    Load a hook module and yield the cleanup function.

    Skips test if no module path detected.
    Uses yield pattern for proper async cleanup.
    """
    if module_path is None:
        pytest.skip("No module path detected")

    cleanup = await _load_module(module_path, coordinator)
    yield cleanup

    # Cleanup after test (handles both sync and async cleanup functions)
    if cleanup:
        if inspect.iscoroutinefunction(cleanup):
            await cleanup()
        else:
            cleanup()


@pytest_asyncio.fixture
async def orchestrator_module(
    module_path: Path | None,
    coordinator: Any,
) -> AsyncGenerator[Any, None]:
    """
    Load and return an orchestrator module for testing.

    Skips test if no module path detected.
    Uses yield pattern for proper async cleanup.
    """
    if module_path is None:
        pytest.skip("No module path detected")

    cleanup = await _load_module(module_path, coordinator)

    # Get the mounted orchestrator (single module, not a dict)
    orchestrator = coordinator.mount_points.get("orchestrator")
    if orchestrator is None:
        pytest.fail("No orchestrator was mounted")

    yield orchestrator

    # Cleanup after test (handles both sync and async cleanup functions)
    if cleanup:
        if inspect.iscoroutinefunction(cleanup):
            await cleanup()
        else:
            cleanup()


@pytest_asyncio.fixture
async def context_module(
    module_path: Path | None,
    coordinator: Any,
) -> AsyncGenerator[Any, None]:
    """
    Load and return a context manager module for testing.

    Skips test if no module path detected.
    Uses yield pattern for proper async cleanup.
    """
    if module_path is None:
        pytest.skip("No module path detected")

    cleanup = await _load_module(module_path, coordinator)

    # Get the mounted context
    context = coordinator.mount_points.get("context")
    if context is None:
        pytest.fail("No context manager was mounted")

    yield context

    # Cleanup after test (handles both sync and async cleanup functions)
    if cleanup:
        if inspect.iscoroutinefunction(cleanup):
            await cleanup()
        else:
            cleanup()
