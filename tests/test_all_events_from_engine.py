"""
Test that ALL_EVENTS in events.py is imported from _engine (not locally defined).

This is a structural test: it verifies we don't have a duplicated local list
that can drift from the Rust-authoritative source.
"""

from amplifier_core import _engine
from amplifier_core import events


def test_all_events_is_imported_from_engine():
    """ALL_EVENTS must be the object exported by the Rust _engine, not a local copy."""
    assert events.ALL_EVENTS is _engine.ALL_EVENTS, (
        "events.ALL_EVENTS is a local copy, not imported from _engine. "
        "Remove the local list and add ALL_EVENTS to the _engine import."
    )
