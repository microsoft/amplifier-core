"""Tests for event constants exposed via the _engine PyO3 module."""

import pytest


# All 51 event constant names that should be importable from _engine
ALL_EVENT_NAMES = [
    "SESSION_START",
    "SESSION_START_DEBUG",
    "SESSION_START_RAW",
    "SESSION_END",
    "SESSION_FORK",
    "SESSION_FORK_DEBUG",
    "SESSION_FORK_RAW",
    "SESSION_RESUME",
    "SESSION_RESUME_DEBUG",
    "SESSION_RESUME_RAW",
    "PROMPT_SUBMIT",
    "PROMPT_COMPLETE",
    "PLAN_START",
    "PLAN_END",
    "PROVIDER_REQUEST",
    "PROVIDER_RESPONSE",
    "PROVIDER_RETRY",
    "PROVIDER_ERROR",
    "PROVIDER_THROTTLE",
    "PROVIDER_TOOL_SEQUENCE_REPAIRED",
    "PROVIDER_RESOLVE",
    "LLM_REQUEST",
    "LLM_REQUEST_DEBUG",
    "LLM_REQUEST_RAW",
    "LLM_RESPONSE",
    "LLM_RESPONSE_DEBUG",
    "LLM_RESPONSE_RAW",
    "CONTENT_BLOCK_START",
    "CONTENT_BLOCK_DELTA",
    "CONTENT_BLOCK_END",
    "THINKING_DELTA",
    "THINKING_FINAL",
    "TOOL_PRE",
    "TOOL_POST",
    "TOOL_ERROR",
    "CONTEXT_PRE_COMPACT",
    "CONTEXT_POST_COMPACT",
    "CONTEXT_COMPACTION",
    "CONTEXT_INCLUDE",
    "ORCHESTRATOR_COMPLETE",
    "EXECUTION_START",
    "EXECUTION_END",
    "USER_NOTIFICATION",
    "ARTIFACT_WRITE",
    "ARTIFACT_READ",
    "POLICY_VIOLATION",
    "APPROVAL_REQUIRED",
    "APPROVAL_GRANTED",
    "APPROVAL_DENIED",
    "CANCEL_REQUESTED",
    "CANCEL_COMPLETED",
]


class TestAllEventConstantsImportable:
    """Test that all 51 event constants are importable from _engine and are strings."""

    @pytest.mark.parametrize("name", ALL_EVENT_NAMES)
    def test_event_constant_importable_and_is_string(self, name):
        import amplifier_core._engine as engine

        value = getattr(engine, name)
        assert isinstance(value, str), f"{name} should be a string, got {type(value)}"
        assert ":" in value, f"{name} should follow namespace:action pattern"


class TestNewProviderEvents:
    """Test the 3 new provider events from Task 1."""

    def test_provider_throttle(self):
        from amplifier_core._engine import PROVIDER_THROTTLE

        assert PROVIDER_THROTTLE == "provider:throttle"

    def test_provider_resolve(self):
        from amplifier_core._engine import PROVIDER_RESOLVE

        assert PROVIDER_RESOLVE == "provider:resolve"

    def test_provider_tool_sequence_repaired(self):
        from amplifier_core._engine import PROVIDER_TOOL_SEQUENCE_REPAIRED

        assert PROVIDER_TOOL_SEQUENCE_REPAIRED == "provider:tool_sequence_repaired"


class TestAllEventsList:
    """Test that ALL_EVENTS is exposed as a list with all 51 items."""

    def test_all_events_is_list(self):
        from amplifier_core._engine import ALL_EVENTS

        assert isinstance(ALL_EVENTS, list), (
            f"ALL_EVENTS should be a list, got {type(ALL_EVENTS)}"
        )

    def test_all_events_count(self):
        from amplifier_core._engine import ALL_EVENTS

        assert len(ALL_EVENTS) == 51, f"Expected 51 events, got {len(ALL_EVENTS)}"

    def test_all_events_contains_all_constants(self):
        import amplifier_core._engine as engine
        from amplifier_core._engine import ALL_EVENTS

        for name in ALL_EVENT_NAMES:
            value = getattr(engine, name)
            assert value in ALL_EVENTS, f"{name}={value!r} not found in ALL_EVENTS"

    def test_all_events_all_strings(self):
        from amplifier_core._engine import ALL_EVENTS

        for event in ALL_EVENTS:
            assert isinstance(event, str), (
                f"ALL_EVENTS item should be str, got {type(event)}"
            )


class TestEventsMatchPythonModule:
    """Test that _engine event constants match the Python events module."""

    def test_shared_events_match(self):
        import amplifier_core._engine as engine
        import amplifier_core.events as py_events

        # Compare all events that exist in the Python events module
        py_event_names = [
            name
            for name in dir(py_events)
            if name.isupper() and not name.startswith("_") and name != "ALL_EVENTS"
        ]

        for name in py_event_names:
            py_value = getattr(py_events, name)
            engine_value = getattr(engine, name, None)
            assert engine_value is not None, (
                f"{name} exists in events.py but not in _engine"
            )
            assert engine_value == py_value, (
                f"{name}: _engine={engine_value!r} != events.py={py_value!r}"
            )
