"""
Tests for capabilities taxonomy integrity.
Catches runtime bugs that aren't obvious from reading the code.
"""

import re

from amplifier_core import capabilities


def test_well_known_capabilities_is_list():
    """ALL_WELL_KNOWN_CAPABILITIES is a list (from Rust via PyO3)."""
    assert isinstance(capabilities.ALL_WELL_KNOWN_CAPABILITIES, (list, frozenset)), (
        f"ALL_WELL_KNOWN_CAPABILITIES should be list or frozenset, got {type(capabilities.ALL_WELL_KNOWN_CAPABILITIES).__name__}"
    )


def test_all_capability_constants_in_list():
    """Verify every capability constant is in ALL_WELL_KNOWN_CAPABILITIES (catches forgotten additions)."""
    # Capability constants are uppercase, not starting with _, not ALL_* or COST_* or MODEL_*
    skip_prefixes = ("ALL_", "COST_", "MODEL_")
    capability_constants = [
        getattr(capabilities, name)
        for name in dir(capabilities)
        if name.isupper()
        and not name.startswith("_")
        and not any(name.startswith(p) for p in skip_prefixes)
    ]

    missing = [
        c
        for c in capability_constants
        if c not in capabilities.ALL_WELL_KNOWN_CAPABILITIES
    ]
    assert len(missing) == 0, (
        f"Capability constants not in ALL_WELL_KNOWN_CAPABILITIES: {missing}"
    )

    # Verify count matches
    assert len(capability_constants) == len(capabilities.ALL_WELL_KNOWN_CAPABILITIES), (
        f"Mismatch: {len(capability_constants)} constants vs "
        f"{len(capabilities.ALL_WELL_KNOWN_CAPABILITIES)} in ALL_WELL_KNOWN_CAPABILITIES"
    )


def test_capabilities_are_lowercase_snake_case():
    """Verify all capability values are lowercase snake_case strings."""
    pattern = re.compile(r"^[a-z][a-z0-9]*(_[a-z0-9]+)*$")
    for cap in capabilities.ALL_WELL_KNOWN_CAPABILITIES:
        assert isinstance(cap, str), f"Capability {cap!r} is not a string"
        assert pattern.match(cap), (
            f"Capability {cap!r} does not match lowercase snake_case pattern"
        )
