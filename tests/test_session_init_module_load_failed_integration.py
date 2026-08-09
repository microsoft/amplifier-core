"""
Integration test: real session init loading pipeline, real ModuleLoader,
a tool module whose mount() raises.

This is the reproduction shape for the silent-failure defect: a tool module
that deliberately raises during mount() (exactly as a real tool module would
when it detects it cannot serve the current platform and fails loud by
design) must be observable through the kernel's event surface, not just a
WARNING log line nobody watches. Exercises the real ModuleLoader.load() ->
source resolution -> filesystem discovery -> mount path WITHOUT mocking the
loader, mirroring test_session_init_integration.py's real-loader pattern.
"""

import importlib
import os
import shutil
import sys
import tempfile

import pytest
from amplifier_core._session_init import initialize_session
from amplifier_core.events import MODULE_LOAD_FAILED
from amplifier_core.loader import ModuleLoader
from amplifier_core.testing import MockCoordinator

# ---------------------------------------------------------------------------
# Fixture helpers
# ---------------------------------------------------------------------------

ORCH_MODULE_NAME = "amplifier_module_lf_test_orch"
CTX_MODULE_NAME = "amplifier_module_lf_test_ctx"
FAILING_TOOL_MODULE_NAME = "amplifier_module_lf_test_failing_tool"

ORCH_INIT_PY = """\
__amplifier_module_type__ = "orchestrator"


async def mount(coordinator, config=None):
    class FakeOrch:
        async def execute(self, prompt, context, providers, tools, hooks, **kwargs):
            return f"echo: {prompt}"

    await coordinator.mount("orchestrator", FakeOrch())
    return None
"""

CTX_INIT_PY = """\
__amplifier_module_type__ = "context"


async def mount(coordinator, config=None):
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
    return None
"""

# A tool module whose mount() deliberately raises -- exactly the shape of a
# real-world tool that detects it cannot serve the current platform and
# fails loud by design (per the incident this fix responds to).
FAILING_TOOL_INIT_PY = """\
__amplifier_module_type__ = "tool"


class UnsupportedPlatformError(RuntimeError):
    pass


async def mount(coordinator, config=None):
    raise UnsupportedPlatformError(
        "cannot mount: this tool does not support the current platform"
    )
"""


@pytest.fixture
def fixture_dir():
    """Create a temp directory with orchestrator, context, and a tool module
    whose mount() raises."""
    tmp = tempfile.mkdtemp(prefix="amp_integ_load_failed_test_")

    for pkg_name, init_py in (
        (ORCH_MODULE_NAME, ORCH_INIT_PY),
        (CTX_MODULE_NAME, CTX_INIT_PY),
        (FAILING_TOOL_MODULE_NAME, FAILING_TOOL_INIT_PY),
    ):
        pkg = os.path.join(tmp, pkg_name)
        os.makedirs(pkg)
        with open(os.path.join(pkg, "__init__.py"), "w") as fh:
            fh.write(init_py)

    sys.path.insert(0, tmp)
    importlib.invalidate_caches()

    yield tmp

    try:
        sys.path.remove(tmp)
    except ValueError:
        pass
    for name in [ORCH_MODULE_NAME, CTX_MODULE_NAME, FAILING_TOOL_MODULE_NAME]:
        sys.modules.pop(name, None)

    shutil.rmtree(tmp, ignore_errors=True)


# ---------------------------------------------------------------------------
# The reproduction test
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_real_loader_tool_mount_failure_is_surfaced_not_swallowed(fixture_dir):
    """A tool module's mount() raising, loaded through a REAL session (real
    ModuleLoader, real filesystem discovery, real coordinator), must surface
    as a module:load_failed event -- not just a log line.

    Without the fix in _session_init.py's tool-loading loop, this assertion
    fails: zero events are emitted for a tool load failure (the exception is
    caught and only logger.warning() sees it), which is the exact silent
    failure this PR addresses.
    """
    config = {
        "session": {
            "orchestrator": "lf-test-orch",
            "context": "lf-test-ctx",
        },
        "providers": [],
        "tools": [{"module": "lf-test-failing-tool"}],
        "hooks": [],
    }

    coordinator = MockCoordinator()
    loader = ModuleLoader(coordinator=coordinator)
    coordinator.loader = loader

    captured: list[dict] = []

    def capture_load_failed(event, data):
        captured.append(dict(data))

    coordinator.hooks.register(
        MODULE_LOAD_FAILED, capture_load_failed, 0, name="test-capture"
    )

    # Must not raise: a broken optional tool must not abort session init.
    await initialize_session(
        config, coordinator, session_id="integ-load-failed", parent_id=None
    )

    # The orchestrator and context must still have mounted successfully.
    assert coordinator.get("orchestrator") is not None
    assert coordinator.get("context") is not None

    # The tool must NOT be mounted (its mount() raised).
    tools = coordinator.get("tools") or {}
    assert "lf-test-failing-tool" not in tools

    # The failure must be observable via the event surface, not just logs.
    assert len(captured) == 1, (
        f"Expected exactly one module:load_failed event, got: {captured}"
    )
    assert captured[0]["module_type"] == "tool"
    assert captured[0]["module_id"] == "lf-test-failing-tool"
    assert "cannot mount" in captured[0]["error"]
