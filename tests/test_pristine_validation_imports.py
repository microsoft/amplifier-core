"""Regression for v1.4.0: validators must not require pytest at import time.

The shipped v1.4.0 wheel had ``validation/structural/__init__.py`` eagerly
import test base classes (``test_context.py`` etc.) that did ``import pytest``
at module top level. The 5 type validators imported ``check_on_session_ready``
from ``.structural`` rather than ``.base``, so any production code path that
loaded a validator transitively pulled in ``pytest`` — which is not declared
as a runtime dependency, so a clean ``pip install amplifier-core`` failed
``amplifier`` startup with ``ModuleNotFoundError: No module named 'pytest'``.

This test runs the production import path in a subprocess with ``pytest``
poisoned (``sys.modules['pytest'] = None``) and asserts the imports succeed.
A subprocess is required because pytest is already imported into the parent
test process; only a fresh interpreter sees the poisoned state.

If this test starts failing, it means somebody put a ``from .structural``
back into a validator (or added a new pytest-dependent import to the
structural-package init), which would re-introduce the v1.4.0 regression.

See ``context/release-mandate.md`` Incident History entry for v1.4.0.
"""

import subprocess
import sys
import textwrap


def _run_in_pristine_subprocess(script: str) -> subprocess.CompletedProcess:
    """Run a Python snippet in a subprocess with no pytest available."""
    return subprocess.run(
        [sys.executable, "-c", script],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )


def test_validators_import_without_pytest() -> None:
    """The 5 type validators must be importable without pytest installed."""
    script = textwrap.dedent("""
        import sys
        sys.modules['pytest'] = None  # poison: any `import pytest` now fails

        from amplifier_core.validation import (
            HookValidator,
            ToolValidator,
            OrchestratorValidator,
            ProviderValidator,
            ContextValidator,
        )
        # Touch each so the import isn't optimised away.
        for cls in (HookValidator, ToolValidator, OrchestratorValidator,
                    ProviderValidator, ContextValidator):
            assert cls.__name__
        print("OK")
    """)
    result = _run_in_pristine_subprocess(script)
    assert result.returncode == 0, (
        f"Validators failed to import without pytest.\n"
        f"stdout={result.stdout!r}\n"
        f"stderr={result.stderr!r}"
    )
    assert "OK" in result.stdout


def test_check_on_session_ready_importable_without_pytest() -> None:
    """``check_on_session_ready`` must be importable from base without pytest.

    The function lives in ``validation.base`` so per-type validators can call
    it without dragging in the pytest-dependent test classes from
    ``validation.structural``.
    """
    script = textwrap.dedent("""
        import sys
        sys.modules['pytest'] = None

        from amplifier_core.validation.base import check_on_session_ready
        assert callable(check_on_session_ready)
        print("OK")
    """)
    result = _run_in_pristine_subprocess(script)
    assert result.returncode == 0, (
        f"check_on_session_ready not importable from base without pytest.\n"
        f"stdout={result.stdout!r}\n"
        f"stderr={result.stderr!r}"
    )
    assert "OK" in result.stdout


def test_session_init_importable_without_pytest() -> None:
    """``amplifier_core._session_init`` must be importable without pytest.

    This is the actual production code path that runs at every ``amplifier``
    startup. The v1.4.0 failure surfaced when ``initialize_session()``
    triggered loader → validators → structural → test_*.py → ``import pytest``.
    """
    script = textwrap.dedent("""
        import sys
        sys.modules['pytest'] = None

        import amplifier_core._session_init  # noqa: F401
        import amplifier_core.loader          # noqa: F401
        import amplifier_core.coordinator     # noqa: F401
        print("OK")
    """)
    result = _run_in_pristine_subprocess(script)
    assert result.returncode == 0, (
        f"Session-init import path failed without pytest.\n"
        f"stdout={result.stdout!r}\n"
        f"stderr={result.stderr!r}"
    )
    assert "OK" in result.stdout
