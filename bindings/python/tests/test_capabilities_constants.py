"""Tests for capabilities constants exposed via the _engine PyO3 module."""

import pytest


# All 16 capability constant names that should be importable from _engine
CAPABILITY_NAMES = [
    "TOOLS",
    "STREAMING",
    "THINKING",
    "VISION",
    "JSON_MODE",
    "FAST",
    "CODE_EXECUTION",
    "WEB_SEARCH",
    "DEEP_RESEARCH",
    "LOCAL",
    "AUDIO",
    "IMAGE_GENERATION",
    "COMPUTER_USE",
    "EMBEDDINGS",
    "LONG_CONTEXT",
    "BATCH",
]

# Expected values for each capability constant (matches main's capabilities.py)
EXPECTED_CAPABILITY_VALUES = {
    "TOOLS": "tools",
    "STREAMING": "streaming",
    "THINKING": "thinking",
    "VISION": "vision",
    "JSON_MODE": "json_mode",
    "FAST": "fast",
    "CODE_EXECUTION": "code_execution",
    "WEB_SEARCH": "web_search",
    "DEEP_RESEARCH": "deep_research",
    "LOCAL": "local",
    "AUDIO": "audio",
    "IMAGE_GENERATION": "image_generation",
    "COMPUTER_USE": "computer_use",
    "EMBEDDINGS": "embeddings",
    "LONG_CONTEXT": "long_context",
    "BATCH": "batch",
}


class TestCapabilityConstantsImportable:
    """Test that all 16 capability constants are importable from _engine and are strings."""

    @pytest.mark.parametrize("name", CAPABILITY_NAMES)
    def test_capability_constant_importable_and_is_string(self, name):
        import amplifier_core._engine as engine

        value = getattr(engine, name)
        assert isinstance(value, str), f"{name} should be a string, got {type(value)}"
        assert len(value) > 0, f"{name} should be non-empty"


class TestAllWellKnownCapabilities:
    """Test that ALL_WELL_KNOWN_CAPABILITIES is exposed and contains all 16 capabilities."""

    def test_all_well_known_capabilities_exists(self):
        from amplifier_core._engine import ALL_WELL_KNOWN_CAPABILITIES

        assert isinstance(ALL_WELL_KNOWN_CAPABILITIES, list), (
            f"ALL_WELL_KNOWN_CAPABILITIES should be a list, got {type(ALL_WELL_KNOWN_CAPABILITIES)}"
        )

    def test_all_well_known_capabilities_count(self):
        from amplifier_core._engine import ALL_WELL_KNOWN_CAPABILITIES

        assert len(ALL_WELL_KNOWN_CAPABILITIES) == 16, (
            f"Expected 16 capabilities, got {len(ALL_WELL_KNOWN_CAPABILITIES)}"
        )

    def test_all_well_known_capabilities_contains_all(self):
        import amplifier_core._engine as engine
        from amplifier_core._engine import ALL_WELL_KNOWN_CAPABILITIES

        for name in CAPABILITY_NAMES:
            value = getattr(engine, name)
            assert value in ALL_WELL_KNOWN_CAPABILITIES, (
                f"{name}={value!r} not found in ALL_WELL_KNOWN_CAPABILITIES"
            )

    def test_all_well_known_capabilities_all_strings(self):
        from amplifier_core._engine import ALL_WELL_KNOWN_CAPABILITIES

        for cap in ALL_WELL_KNOWN_CAPABILITIES:
            assert isinstance(cap, str), (
                f"ALL_WELL_KNOWN_CAPABILITIES item should be str, got {type(cap)}"
            )


class TestCapabilityValuesMatchMain:
    """Test that capability constant values match what's defined in main's capabilities.py."""

    @pytest.mark.parametrize("name,expected", list(EXPECTED_CAPABILITY_VALUES.items()))
    def test_capability_value(self, name, expected):
        import amplifier_core._engine as engine

        value = getattr(engine, name)
        assert value == expected, f"{name}: expected {expected!r}, got {value!r}"
