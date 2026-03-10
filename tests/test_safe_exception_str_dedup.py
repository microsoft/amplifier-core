"""
Tests that _safe_exception_str is not duplicated across session modules.

session.py should import _safe_exception_str from _session_init,
not define its own copy.
"""

import ast
from pathlib import Path


def test_safe_exception_str_not_defined_in_session_module():
    """session.py must not define _safe_exception_str locally."""
    session_path = (
        Path(__file__).parent.parent / "python" / "amplifier_core" / "session.py"
    )
    tree = ast.parse(session_path.read_text())
    local_defs = [
        node.name
        for node in ast.walk(tree)
        if isinstance(node, ast.FunctionDef) and node.name == "_safe_exception_str"
    ]
    assert local_defs == [], (
        f"_safe_exception_str should be imported from _session_init, "
        f"not defined locally in session.py. Found {len(local_defs)} local definition(s)."
    )


def test_session_uses_safe_exception_str_from_session_init():
    """The _safe_exception_str used in session.py must be the one from _session_init."""
    from amplifier_core import _session_init
    from amplifier_core import session

    assert hasattr(session, "_safe_exception_str"), (
        "session module must have _safe_exception_str available (via import)"
    )
    assert session._safe_exception_str is _session_init._safe_exception_str, (
        "_safe_exception_str in session.py must be the exact same object "
        "as in _session_init.py (imported, not duplicated)"
    )
