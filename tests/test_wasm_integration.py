"""Integration tests — load WASM fixtures through load_and_mount_wasm for all 6 module types.

Uses real Rust _engine module (no mocks). May be slow on ARM64 due to WASM compilation.
"""

import json
import os
import shutil
import tempfile
from pathlib import Path

import pytest

FIXTURES_DIR = Path(__file__).parent / "fixtures" / "wasm"

# Module-level skip if WASM fixtures not found
if not FIXTURES_DIR.exists():
    pytest.skip(
        "WASM fixtures not found in tests/fixtures/wasm/", allow_module_level=True
    )

try:
    from amplifier_core._engine import RustCoordinator, load_and_mount_wasm  # type: ignore[reportAttributeAccessIssue]
except ImportError:
    pytest.skip(
        "Rust _engine module not available (load_and_mount_wasm missing)",
        allow_module_level=True,
    )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _get_coordinator():
    """Create a real RustCoordinator with a fake session.

    session_id='test-session', parent_id=None, config={}.
    """

    class _FakeSession:
        session_id = "test-session"
        parent_id = None
        config = {}

    return RustCoordinator(session=_FakeSession())


def _isolated_wasm_dir(wasm_filename: str) -> str:
    """Create a temp directory containing only the given .wasm fixture (symlink).

    Returns the temp directory path.  Caller must clean up.
    Skips the test if the fixture file does not exist.
    """
    src = FIXTURES_DIR / wasm_filename
    if not src.exists():
        pytest.skip(f"WASM fixture not found: {src}")
    tmpdir = tempfile.mkdtemp(prefix=f"wasm_{wasm_filename.replace('.wasm', '')}_")
    os.symlink(str(src.resolve()), os.path.join(tmpdir, wasm_filename))
    return tmpdir


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.slow
@pytest.mark.asyncio
async def test_load_echo_tool_wasm():
    """Load echo-tool.wasm via load_and_mount_wasm — tool module."""
    tmpdir = _isolated_wasm_dir("echo-tool.wasm")
    try:
        coord = _get_coordinator()
        result = load_and_mount_wasm(coord, tmpdir)

        assert result["status"] == "mounted"
        assert result["module_type"] == "tool"
        assert result["name"] == "echo-tool"

        tool = coord.mount_points["tools"]["echo-tool"]
        assert hasattr(tool, "name")
        assert hasattr(tool, "get_spec")
        assert hasattr(tool, "execute")
        assert tool.name == "echo-tool"
        execution = await tool.execute({"message": "hello"})
        assert execution["content"] == [
            {"type": "text", "text": "echo result"},
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "AA==",
                },
            },
        ]
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


@pytest.mark.slow
@pytest.mark.asyncio
async def test_load_echo_provider_wasm():
    """Load echo-provider.wasm via load_and_mount_wasm — provider module."""
    tmpdir = _isolated_wasm_dir("echo-provider.wasm")
    try:
        coord = _get_coordinator()
        result = load_and_mount_wasm(coord, tmpdir)

        assert result["status"] == "mounted"
        assert result["module_type"] == "provider"
        assert len(coord.mount_points["providers"]) > 0
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


@pytest.mark.slow
@pytest.mark.asyncio
async def test_load_memory_context_wasm():
    """Load memory-context.wasm via load_and_mount_wasm — context module."""
    tmpdir = _isolated_wasm_dir("memory-context.wasm")
    try:
        coord = _get_coordinator()
        result = load_and_mount_wasm(coord, tmpdir)

        assert result["status"] == "mounted"
        assert result["module_type"] == "context"
        assert coord.mount_points["context"] is not None
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


@pytest.mark.slow
@pytest.mark.asyncio
async def test_load_passthrough_orchestrator_wasm():
    """Load passthrough-orchestrator.wasm via load_and_mount_wasm — orchestrator module."""
    tmpdir = _isolated_wasm_dir("passthrough-orchestrator.wasm")
    try:
        coord = _get_coordinator()
        result = load_and_mount_wasm(coord, tmpdir)

        assert result["status"] == "mounted"
        assert result["module_type"] == "orchestrator"
        assert coord.mount_points["orchestrator"] is not None
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


@pytest.mark.asyncio
async def test_passthrough_orchestrator_emits_async_python_hook():
    """A WASM host callback preserves an async Python hook's owning loop."""
    import asyncio

    tmpdir = _isolated_wasm_dir("passthrough-orchestrator.wasm")
    try:
        coord = _get_coordinator()
        result = load_and_mount_wasm(coord, tmpdir)
        assert result["status"] == "mounted"

        callback_calls = []

        async def async_deny_handler(event, data):
            await asyncio.sleep(0)
            callback_calls.append((event, data))
            return {"action": "deny", "reason": "async Python denial"}

        coord.hooks.register(
            "test:async-hook",
            async_deny_handler,
            0,
            name="async-python-deny",
        )
        mounted = coord.mount_points["orchestrator"]
        response = await mounted.execute("test:emit-hook")
        hook_result = json.loads(response)

        assert hook_result["action"] == "deny"
        assert hook_result["reason"] == "async Python denial"
        assert len(callback_calls) == 1
        event, payload = callback_calls[0]
        assert event == "test:async-hook"
        # The kernel adds its standard event timestamp to the empty input.
        assert isinstance(payload["timestamp"], str)
        assert {key: value for key, value in payload.items() if key != "timestamp"} == {}
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


@pytest.mark.asyncio
async def test_load_deny_hook_wasm():
    """Load deny-hook.wasm via load_and_mount_wasm — hook module."""
    tmpdir = _isolated_wasm_dir("deny-hook.wasm")
    try:
        coord = _get_coordinator()
        result = load_and_mount_wasm(coord, tmpdir)

        assert result["module_type"] == "hook"
        assert result["status"] == "mounted"
        assert result["subscriptions_count"] == 1
        assert coord.hooks.list_handlers("tool:pre") == {"tool:pre": ["deny-all"]}
        assert coord.mount_points["hooks"] is coord.hooks

        denial = await coord.hooks.emit("tool:pre", {"tool": "test-tool"})
        assert denial.action == "deny"
        assert denial.reason == "Denied by WASM hook"

        # A separately constructed registry must not inherit coordinator hooks.
        from amplifier_core._engine import RustHookRegistry

        standalone = RustHookRegistry()
        assert standalone.list_handlers("tool:pre") == {"tool:pre": []}
        assert (await standalone.emit("tool:pre", {})).action == "continue"
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


@pytest.mark.slow
@pytest.mark.asyncio
async def test_load_auto_approve_wasm():
    """Load auto-approve.wasm via load_and_mount_wasm — approval module."""
    tmpdir = _isolated_wasm_dir("auto-approve.wasm")
    try:
        coord = _get_coordinator()
        result = load_and_mount_wasm(coord, tmpdir)

        assert result["module_type"] == "approval"
        assert result["status"] == "loaded"
        assert "wrapper" in result
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)
