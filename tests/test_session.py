"""
Tests for Amplifier core session functionality.
"""

from unittest.mock import AsyncMock
from unittest.mock import Mock

import pytest
from amplifier_core import AmplifierSession
from amplifier_core import ChatResponse
from amplifier_core import TextBlock
from amplifier_core import Usage


class MockProvider:
    """Minimal mock provider for testing."""

    name = "mock"

    def __init__(self):
        self.call_count = 0
        self.complete = AsyncMock(side_effect=self._complete)

    async def _complete(self, request, **kwargs):
        self.call_count += 1
        return ChatResponse(
            content=[TextBlock(text="Test response")],
            usage=Usage(input_tokens=10, output_tokens=20, total_tokens=30),
        )

    def parse_tool_calls(self, response):
        return []


@pytest.fixture
def minimal_config():
    """Minimal valid configuration for testing."""
    return {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        },
        "providers": [],
        "tools": [],
    }


@pytest.mark.asyncio
async def test_session_initialization(minimal_config):
    """Test session can be initialized with required config."""
    session = AmplifierSession(minimal_config)

    assert session.session_id is not None
    assert session.coordinator is not None
    assert session.loader is not None
    assert not session._initialized


@pytest.mark.asyncio
async def test_session_with_config():
    """Test session accepts configuration."""
    config = {"session": {"orchestrator": "test-orchestrator", "context": "test-context"}}

    session = AmplifierSession(config)
    assert session.config["session"]["orchestrator"] == "test-orchestrator"


@pytest.mark.asyncio
async def test_session_context_manager(minimal_config):
    """Test session works as async context manager."""
    session = AmplifierSession(minimal_config)

    # Mock initialize to avoid actual module loading
    session.initialize = AsyncMock()
    session.cleanup = AsyncMock()

    async with session:
        assert session is not None
        session.initialize.assert_called_once()

    # Cleanup should be called after exit
    session.cleanup.assert_called_once()


@pytest.mark.asyncio
async def test_session_execute_requires_modules(minimal_config):
    """Test session execution requires modules to be mounted."""
    session = AmplifierSession(minimal_config)

    # Create mock orchestrator and context to bypass loader
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="Test response")

    mock_context = Mock()
    mock_context.add_message = AsyncMock()
    mock_context.get_messages = AsyncMock(return_value=[])

    # Mount mocks directly (bypassing loader for testing)
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = mock_context
    # Don't mount any providers

    session._initialized = True

    # Config has no providers, so execution should fail when checking for providers
    with pytest.raises(RuntimeError, match="No providers mounted"):
        await session.execute("Test prompt")


@pytest.mark.asyncio
async def test_session_with_mock_modules(minimal_config):
    """Test session with mock modules."""
    # This would require setting up mock module loading
    # For now, directly mount mock modules
    session = AmplifierSession(minimal_config)

    # Create mock orchestrator
    mock_orchestrator = AsyncMock()
    mock_orchestrator.execute = AsyncMock(return_value="Test response")

    # Create mock context
    mock_context = Mock()
    mock_context.add_message = AsyncMock()
    mock_context.get_messages = AsyncMock(return_value=[])

    # Mount mocks directly (bypassing loader for testing)
    session.coordinator.mount_points["orchestrator"] = mock_orchestrator
    session.coordinator.mount_points["context"] = mock_context
    session.coordinator.mount_points["providers"] = {"mock": MockProvider()}

    session._initialized = True

    # Now execution should work
    result = await session.execute("Test prompt")
    assert result == "Test response"

    # Verify orchestrator was called
    mock_orchestrator.execute.assert_called_once()


@pytest.mark.asyncio
async def test_session_requires_config():
    """Test session requires configuration."""
    with pytest.raises(ValueError, match="Configuration is required"):
        AmplifierSession({})


@pytest.mark.asyncio
async def test_session_requires_orchestrator():
    """Test session requires orchestrator in config."""
    config = {
        "session": {
            "context": "context-simple",
        }
    }
    with pytest.raises(ValueError, match="must specify session.orchestrator"):
        AmplifierSession(config)


@pytest.mark.asyncio
async def test_session_requires_context():
    """Test session requires context in config."""
    config = {
        "session": {
            "orchestrator": "loop-basic",
        }
    }
    with pytest.raises(ValueError, match="must specify session.context"):
        AmplifierSession(config)


@pytest.mark.asyncio
async def test_session_with_custom_loader():
    """Test session accepts custom loader."""
    from pathlib import Path

    from amplifier_core import ModuleLoader

    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        }
    }

    custom_loader = ModuleLoader(search_paths=[Path("/custom/path")])
    session = AmplifierSession(config, loader=custom_loader)

    assert session.loader is custom_loader
