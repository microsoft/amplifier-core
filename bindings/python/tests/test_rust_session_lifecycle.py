"""Tests for Rust-driven session lifecycle (Task 8: initialize() in Rust).

Verifies that PySession.initialize():
1. Sets the initialized flag to True after successful init
2. Is idempotent (second call is a no-op)
3. Delegates module loading to the Python helper
4. Propagates errors from module loading (initialized stays False)
"""

import pytest
from unittest.mock import AsyncMock, patch

from amplifier_core._engine import RustSession


@pytest.mark.asyncio
async def test_initialize_sets_initialized_flag():
    """After successful initialize(), session.initialized should be True."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert session.initialized is False

    # Mock the Python init helper so we don't need real modules installed
    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()

    assert session.initialized is True


@pytest.mark.asyncio
async def test_initialize_is_idempotent():
    """Calling initialize() twice only runs module loading once."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)

    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()
        await session.initialize()  # Second call should be a no-op

    mock_init.assert_called_once()


@pytest.mark.asyncio
async def test_initialize_delegates_to_python_helper():
    """Rust initialize() passes config, coordinator, session_id, parent_id to Python."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(
        config=config, session_id="test-rust-init", parent_id="parent-42"
    )

    mock_init = AsyncMock()
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        await session.initialize()

    mock_init.assert_called_once()
    args = mock_init.call_args[0]
    # args[0] = config dict, args[1] = coordinator, args[2] = session_id, args[3] = parent_id
    assert args[2] == "test-rust-init"
    assert args[3] == "parent-42"


@pytest.mark.asyncio
async def test_initialize_error_keeps_initialized_false():
    """If module loading fails, initialized stays False."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)

    mock_init = AsyncMock(side_effect=RuntimeError("Module not found"))
    with patch("amplifier_core._session_init.initialize_session", mock_init):
        with pytest.raises(Exception):
            await session.initialize()

    assert session.initialized is False
