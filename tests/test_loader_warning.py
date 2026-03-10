"""Tests that resolve_module failures are logged at WARNING level.

When the Rust engine's resolve_module raises an unexpected exception,
the loader falls through to the Python loader. This fallback should be
logged at WARNING (not DEBUG) so operators notice manifest corruption
or other engine failures in normal log output.
"""

import logging
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from amplifier_core.loader import ModuleLoader


@pytest.mark.asyncio
async def test_resolve_module_failure_logs_warning(caplog, tmp_path):
    """resolve_module raising RuntimeError must produce a WARNING log record."""

    # -- Mock coordinator with source resolver ---------------------------------
    fake_source = MagicMock()
    fake_source.resolve.return_value = tmp_path  # any valid Path

    mock_resolver = MagicMock()
    mock_resolver.async_resolve = AsyncMock(return_value=fake_source)

    mock_coordinator = MagicMock()
    mock_coordinator.get.return_value = mock_resolver
    mock_coordinator.mount_points = {
        "orchestrator": None,
        "providers": {},
        "tools": {},
        "context": None,
        "hooks": MagicMock(),
        "module-source-resolver": mock_resolver,
    }

    loader = ModuleLoader()
    loader._coordinator = mock_coordinator

    # -- Build a mock engine where resolve_module raises -----------------------
    mock_engine = MagicMock()
    mock_engine.resolve_module.side_effect = RuntimeError("corrupt manifest")

    # Patch _load_entry_point and _load_filesystem to return None so we
    # reach the ValueError at the end (after the engine fallthrough path).
    # Patch _validate_module to be a no-op (we don't care about validation here).
    with (
        patch.object(loader, "_load_entry_point", return_value=None),
        patch.object(loader, "_load_filesystem", return_value=None),
        patch.object(loader, "_validate_module", new_callable=AsyncMock),
        patch("amplifier_core._engine", mock_engine, create=True),
        patch.dict("sys.modules", {"amplifier_core._engine": mock_engine}),
        caplog.at_level(logging.WARNING, logger="amplifier_core.loader"),
    ):
        with pytest.raises(ValueError, match="failed to load"):
            await loader.load(
                module_id="test-mod",
                config={},
                coordinator=mock_coordinator,
            )

    # -- Assert that the warning was emitted -----------------------------------
    warning_records = [
        r
        for r in caplog.records
        if r.levelno >= logging.WARNING and "resolve_module failed" in r.message
    ]
    assert len(warning_records) >= 1, (
        f"Expected at least one WARNING with 'resolve_module failed', "
        f"got records: {[(r.levelname, r.message) for r in caplog.records]}"
    )
