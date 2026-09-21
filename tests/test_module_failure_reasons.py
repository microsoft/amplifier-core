"""Safe categories at the failure origin and event boundary; no text parsing."""
import json
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock

import pytest

from amplifier_core._session_init import _emit_module_load_failed
from amplifier_core.loader import ModuleLoader, ModuleValidationError, module_failure_reason
from amplifier_core.module_sources import ModuleNotFoundError as SourceNotFoundError


@pytest.mark.asyncio
@pytest.mark.parametrize('content,expected', [
    (None, 'invalid_package_layout'),
    ('__amplifier_module_type__ = "tool"\n', 'invalid_entry_point'),
    ('__amplifier_module_type__ = "secret-invalid-type"\n', 'invalid_module_metadata'),
    ('__amplifier_module_type__ = {"secret": "value"}\n', 'invalid_module_metadata'),
])
async def test_loader_assigns_safe_reason_at_source(tmp_path, monkeypatch, content, expected):
    name = 'amplifier_module_tool_fixture'
    package = tmp_path / name
    if content is not None:
        package.mkdir()
        (package / '__init__.py').write_text(content)
    monkeypatch.syspath_prepend(str(tmp_path))
    monkeypatch.delitem(__import__('sys').modules, name, raising=False)
    with pytest.raises(ModuleValidationError) as caught:
        await ModuleLoader()._validate_module('tool-fixture', tmp_path)
    assert module_failure_reason(caught.value) == expected
    monkeypatch.delitem(__import__('sys').modules, name, raising=False)


@pytest.mark.asyncio
async def test_missing_source_and_unknown_exception_have_stable_categories(tmp_path):
    with pytest.raises(ModuleValidationError) as caught:
        await ModuleLoader()._validate_module('tool-fixture', tmp_path / 'absent')
    assert module_failure_reason(caught.value) == 'missing_source'
    assert module_failure_reason(SourceNotFoundError('/private/secret?token=credential')) == 'missing_source'
    error = RuntimeError('invalid package layout /private/token')
    error.reason_code = 'invalid_package_layout'
    assert module_failure_reason(error) == 'unknown'
    assert module_failure_reason(ModuleValidationError('secret', reason_code='https://secret')) == 'unknown'


@pytest.mark.asyncio
async def test_event_adds_only_allowlisted_reason_without_changing_legacy_error():
    coordinator = MagicMock()
    coordinator.hooks.emit = AsyncMock()
    error = ModuleValidationError('private legacy error', reason_code='invalid_entry_point')
    await _emit_module_load_failed(coordinator, 'tool', 'tool-fixture', error)
    event, data = coordinator.hooks.emit.call_args.args
    assert event == 'module:load_failed'
    assert data['error'] == 'private legacy error'  # Existing event remains compatible.
    assert data['reason_code'] == 'invalid_entry_point'
    assert json.dumps({'reason_code': data['reason_code']}) == '{"reason_code": "invalid_entry_point"}'
