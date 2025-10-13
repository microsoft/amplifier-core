"""
Smoke tests for event emission and canonical event names.
Verifies that canonical events are properly defined and consistent.
"""

from amplifier_core import events


def test_canonical_events_defined():
    """Verify canonical event names are defined in events module."""
    required_events = [
        "session:start",
        "session:end",
        "prompt:submit",
        "prompt:complete",
        "plan:start",
        "plan:end",
        "provider:request",
        "provider:response",
        "provider:error",
        "content_block:start",
        "content_block:delta",
        "content_block:end",
        "tool:pre",
        "tool:post",
        "tool:error",
        "context:pre_compact",
        "context:post_compact",
        "artifact:write",
        "artifact:read",
        "policy:violation",
        "approval:required",
        "approval:granted",
        "approval:denied",
    ]

    for event in required_events:
        # Check if event constant exists
        constant_name = event.replace(":", "_").replace("-", "_").upper()
        assert hasattr(events, constant_name), f"Missing event constant: {constant_name}"


def test_event_constants_match_strings():
    """Verify event constants match expected string values."""
    assert events.SESSION_START == "session:start"
    assert events.SESSION_END == "session:end"
    assert events.PROMPT_SUBMIT == "prompt:submit"
    assert events.PROMPT_COMPLETE == "prompt:complete"
    assert events.PLAN_START == "plan:start"
    assert events.PLAN_END == "plan:end"
    assert events.PROVIDER_REQUEST == "provider:request"
    assert events.PROVIDER_RESPONSE == "provider:response"
    assert events.PROVIDER_ERROR == "provider:error"
    assert events.CONTENT_BLOCK_START == "content_block:start"
    assert events.CONTENT_BLOCK_DELTA == "content_block:delta"
    assert events.CONTENT_BLOCK_END == "content_block:end"
    assert events.TOOL_PRE == "tool:pre"
    assert events.TOOL_POST == "tool:post"
    assert events.TOOL_ERROR == "tool:error"
    assert events.CONTEXT_PRE_COMPACT == "context:pre_compact"
    assert events.CONTEXT_POST_COMPACT == "context:post_compact"
    assert events.ARTIFACT_WRITE == "artifact:write"
    assert events.ARTIFACT_READ == "artifact:read"
    assert events.POLICY_VIOLATION == "policy:violation"
    assert events.APPROVAL_REQUIRED == "approval:required"
    assert events.APPROVAL_GRANTED == "approval:granted"
    assert events.APPROVAL_DENIED == "approval:denied"


def test_all_events_list_complete():
    """Verify ALL_EVENTS list contains all canonical events."""
    # Get all event constants from the module
    event_constants = [
        getattr(events, name)
        for name in dir(events)
        if name.isupper() and not name.startswith("_") and name != "ALL_EVENTS"
    ]

    # Verify ALL_EVENTS contains them all
    for event in event_constants:
        assert event in events.ALL_EVENTS, f"Event {event} not in ALL_EVENTS list"


def test_all_events_no_duplicates():
    """Verify ALL_EVENTS list has no duplicate entries."""
    assert len(events.ALL_EVENTS) == len(set(events.ALL_EVENTS)), "ALL_EVENTS contains duplicates"


def test_event_naming_convention():
    """Verify all events follow the namespace:action naming convention."""
    for event in events.ALL_EVENTS:
        # Events should be lowercase with : or _ separators
        assert event.islower() or "_" in event or ":" in event, f"Event {event} doesn't follow naming convention"

        # Events should have a namespace prefix (before :)
        if ":" in event:
            namespace, action = event.split(":", 1)
            assert namespace, f"Event {event} missing namespace"
            assert action, f"Event {event} missing action"
