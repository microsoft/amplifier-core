"""
Tests that _session_init.initialize_session() routes through
loader_dispatch.load_module() instead of calling loader.load() directly.

This ensures WASM and gRPC modules can load in a real session, because
loader_dispatch inspects amplifier.toml to pick the right transport.
"""

from unittest.mock import AsyncMock, MagicMock, patch

import pytest


def _make_config():
    """Minimal config with orchestrator, context, and one tool."""
    return {
        "session": {
            "orchestrator": {
                "module": "loop-basic",
                "source": "/fake/orchestrator",
                "config": {},
            },
            "context": {
                "module": "context-simple",
                "source": "/fake/context",
                "config": {},
            },
        },
        "providers": [],
        "tools": [
            {
                "module": "tool-echo",
                "source": "/fake/tool-echo",
                "config": {},
            },
        ],
        "hooks": [],
    }


def _make_coordinator():
    """Minimal mock coordinator with a loader attribute."""
    coordinator = MagicMock()
    coordinator.loader = MagicMock()
    coordinator.register_cleanup = MagicMock()
    return coordinator


@pytest.mark.asyncio
async def test_initialize_session_calls_load_module():
    """initialize_session must call loader_dispatch.load_module for each module."""
    config = _make_config()
    coordinator = _make_coordinator()

    # Mock mount function returned by load_module
    mock_mount = AsyncMock(return_value=None)
    mock_load_module = AsyncMock(return_value=mock_mount)

    # Patch at the source — initialize_session does
    # `from .loader_dispatch import load_module` inside the function body,
    # so patching the source module intercepts the import at call time.
    with patch("amplifier_core.loader_dispatch.load_module", mock_load_module):
        from amplifier_core._session_init import initialize_session

        await initialize_session(
            config=config,
            coordinator=coordinator,
            session_id="test-session-001",
            parent_id=None,
        )

    # orchestrator + context + 1 tool = 3 calls
    assert mock_load_module.call_count == 3, (
        f"Expected 3 calls to load_module, got {mock_load_module.call_count}"
    )


@pytest.mark.asyncio
async def test_initialize_session_passes_correct_args_to_load_module():
    """load_module receives (module_id, config, source_path, coordinator)."""
    config = _make_config()
    coordinator = _make_coordinator()

    mock_mount = AsyncMock(return_value=None)
    mock_load_module = AsyncMock(return_value=mock_mount)

    with patch("amplifier_core.loader_dispatch.load_module", mock_load_module):
        from amplifier_core._session_init import initialize_session

        await initialize_session(
            config=config,
            coordinator=coordinator,
            session_id="test-session-002",
            parent_id=None,
        )

    # Check the orchestrator call
    calls = mock_load_module.call_args_list
    orch_call = calls[0]
    assert orch_call[0][0] == "loop-basic"  # module_id
    assert orch_call[0][1] == {}  # config
    assert orch_call[1]["source_path"] == "/fake/orchestrator"
    assert orch_call[1]["coordinator"] is coordinator

    # Check the context call
    ctx_call = calls[1]
    assert ctx_call[0][0] == "context-simple"
    assert ctx_call[0][1] == {}
    assert ctx_call[1]["source_path"] == "/fake/context"
    assert ctx_call[1]["coordinator"] is coordinator

    # Check the tool call
    tool_call = calls[2]
    assert tool_call[0][0] == "tool-echo"
    assert tool_call[0][1] == {}
    assert tool_call[1]["source_path"] == "/fake/tool-echo"
    assert tool_call[1]["coordinator"] is coordinator


@pytest.mark.asyncio
async def test_initialize_session_does_not_call_loader_load():
    """loader.load() must NOT be called — all loading goes through load_module."""
    config = _make_config()
    coordinator = _make_coordinator()

    mock_mount = AsyncMock(return_value=None)
    mock_load_module = AsyncMock(return_value=mock_mount)

    with patch("amplifier_core.loader_dispatch.load_module", mock_load_module):
        from amplifier_core._session_init import initialize_session

        await initialize_session(
            config=config,
            coordinator=coordinator,
            session_id="test-session-003",
            parent_id=None,
        )

    # The old loader.load() path should never be called
    coordinator.loader.load.assert_not_called()
