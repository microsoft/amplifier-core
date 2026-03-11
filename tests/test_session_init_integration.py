"""
Integration test: real session init loading pipeline.

Exercises the real ModuleLoader.load() → source resolution → filesystem
discovery → mount path WITHOUT mocking the loader.  This verifies that
``initialize_session`` actually wires up modules into the coordinator end-to-end.
"""

import importlib
import os
import shutil
import sys
import tempfile

import pytest

from amplifier_core._session_init import initialize_session
from amplifier_core.loader import ModuleLoader
from amplifier_core.testing import MockCoordinator


# ---------------------------------------------------------------------------
# Fixture helpers
# ---------------------------------------------------------------------------

ORCH_MODULE_NAME = "amplifier_module_test_orch"
CTX_MODULE_NAME = "amplifier_module_test_ctx"

ORCH_INIT_PY = '''\
__amplifier_module_type__ = "orchestrator"


async def mount(coordinator, config=None):
    """Mount a fake orchestrator that echoes the prompt."""

    class FakeOrch:
        async def execute(self, prompt, context, providers, tools, hooks, **kwargs):
            return f"echo: {prompt}"

    await coordinator.mount("orchestrator", FakeOrch())
    return None  # no cleanup
'''

CTX_INIT_PY = '''\
__amplifier_module_type__ = "context"


async def mount(coordinator, config=None):
    """Mount a fake context manager."""

    class FakeCtx:
        async def add_message(self, msg):
            pass

        async def get_messages(self):
            return []

        async def get_messages_for_request(self, request=None):
            return []

        async def set_messages(self, msgs):
            pass

        async def clear(self):
            pass

    await coordinator.mount("context", FakeCtx())
    return None  # no cleanup
'''


@pytest.fixture
def fixture_dir():
    """Create a temp directory with minimal orchestrator and context modules."""
    tmp = tempfile.mkdtemp(prefix="amp_integ_test_")

    # Create orchestrator package
    orch_pkg = os.path.join(tmp, ORCH_MODULE_NAME)
    os.makedirs(orch_pkg)
    with open(os.path.join(orch_pkg, "__init__.py"), "w") as fh:
        fh.write(ORCH_INIT_PY)

    # Create context package
    ctx_pkg = os.path.join(tmp, CTX_MODULE_NAME)
    os.makedirs(ctx_pkg)
    with open(os.path.join(ctx_pkg, "__init__.py"), "w") as fh:
        fh.write(CTX_INIT_PY)

    # Make modules importable
    sys.path.insert(0, tmp)
    importlib.invalidate_caches()

    yield tmp

    # Teardown: restore sys.path and evict cached modules
    try:
        sys.path.remove(tmp)
    except ValueError:
        pass
    for name in [ORCH_MODULE_NAME, CTX_MODULE_NAME]:
        sys.modules.pop(name, None)

    shutil.rmtree(tmp, ignore_errors=True)


# ---------------------------------------------------------------------------
# Integration tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_real_loader_mounts_orchestrator_and_context(fixture_dir):
    """initialize_session() with a real ModuleLoader loads and mounts both
    the orchestrator and context modules from the filesystem fixture."""
    config = {
        "session": {
            "orchestrator": "test-orch",
            "context": "test-ctx",
        },
        "providers": [],
        "tools": [],
        "hooks": [],
    }

    coordinator = MockCoordinator()
    loader = ModuleLoader(coordinator=coordinator)
    coordinator.loader = loader

    await initialize_session(
        config, coordinator, session_id="integ-test", parent_id=None
    )

    # Both modules must be mounted
    orchestrator = coordinator.get("orchestrator")
    context = coordinator.get("context")

    assert orchestrator is not None, "Orchestrator was not mounted by real loader"
    assert context is not None, "Context manager was not mounted by real loader"


@pytest.mark.asyncio
async def test_real_loader_orchestrator_execute_works(fixture_dir):
    """The mounted orchestrator's execute() method is callable and returns
    the expected echo response, proving a real object (not a mock) was wired up."""
    config = {
        "session": {
            "orchestrator": "test-orch",
            "context": "test-ctx",
        },
        "providers": [],
        "tools": [],
        "hooks": [],
    }

    coordinator = MockCoordinator()
    loader = ModuleLoader(coordinator=coordinator)
    coordinator.loader = loader

    await initialize_session(
        config, coordinator, session_id="integ-test-2", parent_id=None
    )

    orchestrator = coordinator.get("orchestrator")
    assert orchestrator is not None

    result = await orchestrator.execute(
        "hello",
        context=None,
        providers=None,
        tools=None,
        hooks=None,
    )
    assert result == "echo: hello", f"Unexpected orchestrator response: {result!r}"


@pytest.mark.asyncio
async def test_real_loader_session_init_creates_loader_if_none(fixture_dir):
    """initialize_session() auto-creates a ModuleLoader when coordinator.loader
    is None, and the pipeline still succeeds."""
    config = {
        "session": {
            "orchestrator": "test-orch",
            "context": "test-ctx",
        },
        "providers": [],
        "tools": [],
        "hooks": [],
    }

    coordinator = MockCoordinator()
    # Deliberately do NOT set coordinator.loader — let initialize_session create it
    assert coordinator.loader is None, "Expected coordinator.loader to start as None"

    await initialize_session(
        config, coordinator, session_id="integ-test-3", parent_id=None
    )

    assert coordinator.get("orchestrator") is not None, (
        "Orchestrator not mounted when loader was auto-created"
    )
    assert coordinator.get("context") is not None, (
        "Context not mounted when loader was auto-created"
    )
