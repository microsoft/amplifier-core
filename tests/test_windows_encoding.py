
import asyncio
import pytest
from unittest.mock import MagicMock, patch
from amplifier_core.session import AmplifierSession

# This byte sequence caused the original error, as it's valid in utf-8 but not cp1252
# The character is U+009D OPERATING SYSTEM COMMAND
INVALID_BYTE_MESSAGE = b"an error occurred: \x9d"


class WindowsEncodingError(Exception):
    """Custom exception to simulate a UnicodeDecodeError on Windows."""

    def __init__(self, message_bytes):
        self.message_bytes = message_bytes

    def __str__(self):
        # This is the operation that fails on Windows with default encoding
        try:
            # On non-Windows, this will likely succeed with utf-8
            return self.message_bytes.decode()
        except UnicodeDecodeError:
            # On Windows, this will fail if the default is cp1252
            return self.message_bytes.decode("cp1252")


@patch("amplifier_core.session.ModuleLoader")
def test_windows_encoding_error_safely_handled(MockLoader):
    """
    Tests that a UnicodeDecodeError, typical on Windows with cp1252, is handled.
    """
    # Arrange
    config = {
        "session": {
            "orchestrator": "mock_orchestrator",
            "context": "mock_context",
        }
    }

    # Mock the coordinator and its components to isolate the session logic
    mock_coordinator = MagicMock()

    # The loader's load process is where the exception will be triggered
    async def mock_load(*args, **kwargs):
        raise WindowsEncodingError(INVALID_BYTE_MESSAGE)

    mock_loader_instance = MockLoader.return_value
    mock_loader_instance.load.side_effect = mock_load

    session = AmplifierSession(config=config, loader=mock_loader_instance)

    # Act & Assert
    # The session should catch the exception and handle it gracefully
    # without crashing due to a secondary UnicodeDecodeError.
    with pytest.raises(RuntimeError) as excinfo:
        asyncio.run(session.initialize())

    # The key is that the following line does NOT raise a UnicodeDecodeError
    error_message = str(excinfo.value)

    # Check that the error message contains a representation of the problematic byte
    # repr() will escape it, e.g., '...an error occurred: \\x9d...'
    assert "\\x9d" in error_message
    assert "Cannot initialize without orchestrator" in error_message
