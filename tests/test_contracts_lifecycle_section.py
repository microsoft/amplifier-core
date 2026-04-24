"""
TDD test for 'Module Lifecycle Methods' section in CONTRACTS.md.

Tests verify:
1. The section exists
2. It is positioned correctly (between Python-Only Types and Event Constants)
3. It documents mount() and on_session_ready() with appropriate detail
4. Contract table contains required rows
5. 'When to use' guidance is present
6. Code example is present
7. Polyglot note is present
"""

import os
import re

CONTRACTS_MD_PATH = os.path.join(os.path.dirname(__file__), "..", "CONTRACTS.md")

SECTION_HEADER = "## Module Lifecycle Methods"
AFTER_SECTION = "## Python-Only Types"
BEFORE_SECTION = "## Event Constants"


def read_doc():
    with open(CONTRACTS_MD_PATH, "r") as f:
        return f.read()


def get_section_text(content):
    """Extract the Module Lifecycle Methods section text."""
    start = content.find(SECTION_HEADER)
    if start == -1:
        return None
    end = content.find("\n## ", start + 1)
    if end == -1:
        return content[start:]
    return content[start:end]


# ---------------------------------------------------------------------------
# Structural / placement tests
# ---------------------------------------------------------------------------


def test_section_exists():
    """'## Module Lifecycle Methods' section must exist in CONTRACTS.md."""
    content = read_doc()
    assert SECTION_HEADER in content, (
        f"Section '{SECTION_HEADER}' not found in CONTRACTS.md"
    )


def test_section_between_python_only_types_and_event_constants():
    """Section must appear AFTER Python-Only Types and BEFORE Event Constants."""
    content = read_doc()
    lifecycle_pos = content.find(SECTION_HEADER)
    python_only_pos = content.find("## Python-Only Types")
    event_constants_pos = content.find("## Event Constants")

    assert lifecycle_pos != -1, f"'{SECTION_HEADER}' not found"
    assert python_only_pos != -1, "'## Python-Only Types' not found"
    assert event_constants_pos != -1, "'## Event Constants' not found"

    assert python_only_pos < lifecycle_pos, (
        "Module Lifecycle Methods must appear AFTER Python-Only Types"
    )
    assert lifecycle_pos < event_constants_pos, (
        "Module Lifecycle Methods must appear BEFORE Event Constants"
    )


# ---------------------------------------------------------------------------
# Content tests: mount() documentation
# ---------------------------------------------------------------------------


def test_mount_method_documented():
    """Section must document the mount() method."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert "mount" in section, (
        "mount() method not documented in Module Lifecycle Methods"
    )


def test_mount_is_required():
    """mount() must be documented as required."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    # Should mention it's required or existing
    assert re.search(r"\b(required|existing)\b", section, re.IGNORECASE), (
        "mount() should be documented as required/existing"
    )


def test_mount_signature_documented():
    """mount(coordinator, config) signature must be present."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    # Should show async def mount with coordinator and config
    assert re.search(r"mount\s*\(.*coordinator.*config", section), (
        "mount(coordinator, config) signature not found in section"
    )


# ---------------------------------------------------------------------------
# Content tests: on_session_ready() documentation
# ---------------------------------------------------------------------------


def test_on_session_ready_method_documented():
    """Section must document the on_session_ready() method."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert "on_session_ready" in section, (
        "on_session_ready() method not documented in Module Lifecycle Methods"
    )


def test_on_session_ready_is_optional():
    """on_session_ready() must be documented as optional."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    # Find the part about on_session_ready and check it mentions optional
    osr_pos = section.find("on_session_ready")
    assert osr_pos != -1
    surrounding = section[max(0, osr_pos - 200) : osr_pos + 500]
    assert "optional" in surrounding.lower(), (
        "on_session_ready() should be documented as optional"
    )


def test_on_session_ready_signature_documented():
    """async def on_session_ready(coordinator) -> None signature must be present."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"on_session_ready\s*\(.*coordinator.*\)", section), (
        "on_session_ready(coordinator) signature not found in section"
    )


def test_on_session_ready_called_after_all_modules_complete_mount():
    """on_session_ready() must be documented as called after ALL modules complete mount()."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    # Should mention being called after all modules complete mount
    assert re.search(
        r"all (modules|phases).*(complete|finish|mount)", section, re.IGNORECASE
    ) or re.search(r"after all", section, re.IGNORECASE), (
        "on_session_ready() timing (after all modules complete mount) not documented"
    )


# ---------------------------------------------------------------------------
# Contract table tests
# ---------------------------------------------------------------------------


def test_contract_table_has_presence_row():
    """Contract table must include Presence row (optional)."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"[Pp]resence", section), "Contract table missing 'Presence' row"
    assert "optional" in section.lower(), (
        "Contract table 'Presence' row should indicate 'optional'"
    )


def test_contract_table_has_signature_row():
    """Contract table must include Signature row."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"[Ss]ignature", section), "Contract table missing 'Signature' row"


def test_contract_table_has_async_requirement():
    """Contract table must document async requirement (sync warns and skips)."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"(warn|skip)", section, re.IGNORECASE), (
        "Contract table should document that sync implementations warn and are skipped"
    )


def test_contract_table_has_return_value_row():
    """Contract table must include Return value row (ignored)."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"[Rr]eturn", section), "Contract table missing 'Return' row"


def test_contract_table_has_exceptions_row():
    """Contract table must include Exceptions row (non-fatal, logged)."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"[Ee]xception", section), (
        "Contract table missing 'Exceptions' row"
    )
    assert re.search(r"(non-fatal|non fatal|warning|warn)", section, re.IGNORECASE), (
        "Exceptions should be documented as non-fatal"
    )


def test_contract_table_has_timing_row():
    """Contract table must include Timing row (before session:fork)."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"[Tt]iming", section), "Contract table missing 'Timing' row"
    assert "session:fork" in section, "Timing should mention 'session:fork'"


def test_contract_table_has_scope_row():
    """Contract table must include Scope row (Python-only)."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"[Ss]cope", section), "Contract table missing 'Scope' row"
    assert re.search(r"[Pp]ython.only", section), "Scope should indicate Python-only"


# ---------------------------------------------------------------------------
# 'When to use' section tests
# ---------------------------------------------------------------------------


def test_when_to_use_section_exists():
    """Section must include 'When to use' guidance."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"[Ww]hen to use", section), (
        "Section missing 'When to use' guidance"
    )


def test_when_to_use_mentions_cross_module_deps():
    """'When to use' must mention cross-module dependencies."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(
        r"(cross.module|cross module|dependencies|dep)", section, re.IGNORECASE
    ), "'When to use' should mention cross-module dependencies"


def test_when_to_use_mentions_dual_path_elimination():
    """'When to use' must mention dual-path elimination pattern."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"dual.path", section, re.IGNORECASE), (
        "'When to use' should mention dual-path elimination"
    )


# ---------------------------------------------------------------------------
# Code example tests
# ---------------------------------------------------------------------------


def test_code_example_exists():
    """Section must contain a Python code example."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert "```python" in section, "Section missing Python code example"


def test_code_example_has_before_after():
    """Code example must show before/after pattern (dual-path elimination)."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"(before|after|without|with)", section, re.IGNORECASE), (
        "Code example should show before/after pattern"
    )


# ---------------------------------------------------------------------------
# Polyglot note tests
# ---------------------------------------------------------------------------


def test_polyglot_note_exists():
    """Section must include polyglot note about WASM/gRPC/Rust deferral."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    assert re.search(r"(polyglot|WASM|wasm|gRPC|grpc|Rust)", section), (
        "Section missing polyglot note about WASM/gRPC/Rust deferral"
    )


def test_polyglot_note_mentions_python_only():
    """Polyglot note must clarify on_session_ready is Python-only."""
    content = read_doc()
    section = get_section_text(content)
    assert section is not None, f"'{SECTION_HEADER}' section not found"
    # The polyglot note should mention deferral or Python-only scope
    assert re.search(r"(defer|Python.only|Python only)", section, re.IGNORECASE), (
        "Polyglot note should clarify Python-only scope or deferral"
    )


# ---------------------------------------------------------------------------
# Main runner for standalone execution
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import sys

    tests = [
        test_section_exists,
        test_section_between_python_only_types_and_event_constants,
        test_mount_method_documented,
        test_mount_is_required,
        test_mount_signature_documented,
        test_on_session_ready_method_documented,
        test_on_session_ready_is_optional,
        test_on_session_ready_signature_documented,
        test_on_session_ready_called_after_all_modules_complete_mount,
        test_contract_table_has_presence_row,
        test_contract_table_has_signature_row,
        test_contract_table_has_async_requirement,
        test_contract_table_has_return_value_row,
        test_contract_table_has_exceptions_row,
        test_contract_table_has_timing_row,
        test_contract_table_has_scope_row,
        test_when_to_use_section_exists,
        test_when_to_use_mentions_cross_module_deps,
        test_when_to_use_mentions_dual_path_elimination,
        test_code_example_exists,
        test_code_example_has_before_after,
        test_polyglot_note_exists,
        test_polyglot_note_mentions_python_only,
    ]

    failed = []
    for test in tests:
        try:
            test()
            print(f"  PASS: {test.__name__}")
        except AssertionError as e:
            print(f"  FAIL: {test.__name__}: {e}")
            failed.append(test.__name__)

    if failed:
        print(f"\n{len(failed)} test(s) failed.")
        sys.exit(1)
    else:
        print(f"\nAll {len(tests)} tests passed.")
