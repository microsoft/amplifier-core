"""
Tests for event taxonomy integrity.
Catches runtime bugs that aren't obvious from reading the code.
"""

from amplifier_core import events


def test_no_duplicate_events_in_all_events():
    """Verify ALL_EVENTS contains no duplicates (catches copy-paste errors)."""
    duplicates = [e for e in events.ALL_EVENTS if events.ALL_EVENTS.count(e) > 1]
    assert len(duplicates) == 0, f"ALL_EVENTS contains duplicates: {set(duplicates)}"


def test_all_event_constants_in_all_events():
    """Verify every event constant is in ALL_EVENTS (catches forgotten additions)."""
    event_constants = [
        getattr(events, name)
        for name in dir(events)
        if name.isupper() and not name.startswith("_") and name != "ALL_EVENTS"
    ]

    missing = [e for e in event_constants if e not in events.ALL_EVENTS]
    assert len(missing) == 0, f"Event constants not in ALL_EVENTS: {missing}"

    # Verify count matches (catches constants in ALL_EVENTS that don't exist)
    assert len(event_constants) == len(events.ALL_EVENTS), (
        f"Mismatch: {len(event_constants)} constants vs {len(events.ALL_EVENTS)} in ALL_EVENTS"
    )


def test_events_follow_naming_convention():
    """Verify all events follow namespace:action convention (catches typos)."""
    for event in events.ALL_EVENTS:
        assert ":" in event, f"Event {event} missing namespace separator ':'"
        parts = event.split(":")
        assert len(parts) == 2, f"Event {event} has multiple ':' separators"
        namespace, action = parts
        assert namespace, f"Event {event} has empty namespace"
        assert action, f"Event {event} has empty action"
        # Allow lowercase with underscores
        assert event.islower() or "_" in event, f"Event {event} not lowercase/snake_case"
