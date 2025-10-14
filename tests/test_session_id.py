"""Test session ID handling in AmplifierSession."""

import uuid

import pytest
from amplifier_core import AmplifierSession


def test_session_id_provided():
    """Test that provided session_id is used."""
    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        }
    }

    # Provide a specific session ID
    custom_id = "my-custom-session-123"
    session = AmplifierSession(config, session_id=custom_id)

    # Verify the session uses the provided ID
    assert session.session_id == custom_id
    assert session.status.session_id == custom_id


def test_session_id_generated():
    """Test that session_id is generated when not provided."""
    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        }
    }

    # Don't provide a session ID
    session = AmplifierSession(config)

    # Verify a UUID was generated
    assert session.session_id is not None
    # Check it's a valid UUID
    try:
        uuid.UUID(session.session_id)
    except ValueError:
        pytest.fail(f"Generated session_id is not a valid UUID: {session.session_id}")

    assert session.status.session_id == session.session_id


def test_session_id_none_generates_uuid():
    """Test that explicitly passing None generates a UUID."""
    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        }
    }

    # Explicitly pass None
    session = AmplifierSession(config, session_id=None)

    # Verify a UUID was generated
    assert session.session_id is not None
    try:
        uuid.UUID(session.session_id)
    except ValueError:
        pytest.fail(f"Generated session_id is not a valid UUID: {session.session_id}")


def test_multiple_sessions_unique_ids():
    """Test that multiple sessions without IDs get unique IDs."""
    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        }
    }

    session1 = AmplifierSession(config)
    session2 = AmplifierSession(config)

    # Verify they have different IDs
    assert session1.session_id != session2.session_id
