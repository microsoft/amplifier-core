"""Tests verifying AmplifierSession.initialize() delegates to _session_init.initialize_session()."""

from unittest.mock import AsyncMock, patch

import pytest

from amplifier_core.session import AmplifierSession as PyAmplifierSession


@pytest.fixture
def minimal_config():
    """Minimal valid configuration for delegation tests."""
    return {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        },
        "providers": [],
        "tools": [],
        "hooks": [],
    }


@pytest.mark.asyncio
async def test_initialize_delegates_to_session_init(minimal_config):
    """initialize() calls _session_init.initialize_session() with correct args."""
    session = PyAmplifierSession(minimal_config)

    with patch(
        "amplifier_core.session.initialize_session", new_callable=AsyncMock
    ) as mock_init:
        await session.initialize()

        mock_init.assert_called_once_with(
            minimal_config,
            session.coordinator,
            session.session_id,
            session.parent_id,
        )


@pytest.mark.asyncio
async def test_initialize_is_idempotent(minimal_config):
    """Calling initialize() twice only delegates once."""
    session = PyAmplifierSession(minimal_config)

    with patch(
        "amplifier_core.session.initialize_session", new_callable=AsyncMock
    ) as mock_init:
        await session.initialize()
        await session.initialize()

        mock_init.assert_called_once()


@pytest.mark.asyncio
async def test_initialize_sets_initialized_flag(minimal_config):
    """After successful delegation _initialized is True."""
    session = PyAmplifierSession(minimal_config)

    with patch("amplifier_core.session.initialize_session", new_callable=AsyncMock):
        assert not session._initialized
        await session.initialize()
        assert session._initialized


@pytest.mark.asyncio
async def test_initialize_propagates_errors(minimal_config):
    """If _session_init raises, error propagates and _initialized stays False."""
    session = PyAmplifierSession(minimal_config)

    with patch(
        "amplifier_core.session.initialize_session",
        new_callable=AsyncMock,
        side_effect=RuntimeError("init failed"),
    ):
        with pytest.raises(RuntimeError, match="init failed"):
            await session.initialize()

        assert not session._initialized
