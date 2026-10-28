"""Cancellation through the actual native session's Python orchestrator bridge."""

import asyncio
import importlib

import pytest
from amplifier_core import AmplifierSession


@pytest.mark.asyncio
async def test_cancel_native_session_stops_only_its_owned_orchestrator(
    tmp_path, monkeypatch
):
    modules = {
        "amplifier_module_fixture_cancel_orch": """
import asyncio
__amplifier_module_type__ = 'orchestrator'
entered, stopped, release = {}, {}, {}
class Orchestrator:
    async def execute(self, prompt, **kwargs):
        entered[prompt] = asyncio.Event()
        stopped[prompt] = asyncio.Event()
        release[prompt] = asyncio.Event()
        entered[prompt].set()
        try:
            await release[prompt].wait()
            return prompt
        finally:
            stopped[prompt].set()
async def mount(coordinator, config):
    await coordinator.mount('orchestrator', Orchestrator())
""",
        "amplifier_module_fixture_cancel_context": """
__amplifier_module_type__ = 'context'
async def mount(coordinator, config):
    await coordinator.mount('context', object())
""",
    }
    for name, source in modules.items():
        package = tmp_path / name
        package.mkdir()
        (package / "__init__.py").write_text(source)
    monkeypatch.syspath_prepend(str(tmp_path))
    config = {
        "session": {
            "orchestrator": "fixture-cancel-orch",
            "context": "fixture-cancel-context",
        }
    }
    sessions = [AmplifierSession(config) for _ in range(2)]
    tasks = []
    fixture = None
    try:
        for session in sessions:
            await session.initialize()
        fixture = importlib.import_module("amplifier_module_fixture_cancel_orch")
        tasks = [
            asyncio.create_task(session.execute(label))
            for session, label in zip(sessions, ("cancel", "sibling"))
        ]
        async with asyncio.timeout(2):
            while set(fixture.entered) != {"cancel", "sibling"}:
                await asyncio.sleep(0.01)
        tasks[0].cancel()
        with pytest.raises(asyncio.CancelledError):
            await tasks[0]
        await asyncio.wait_for(fixture.stopped["cancel"].wait(), 2)
        assert not fixture.stopped["sibling"].is_set()
        assert not tasks[1].done()
        fixture.release["sibling"].set()
        assert await tasks[1] == "sibling"
    finally:
        # A failing baseline must not leave its orphaned callback running.
        if fixture is not None:
            for event in fixture.release.values():
                event.set()
            for event in fixture.stopped.values():
                await asyncio.wait_for(event.wait(), 2)
        await asyncio.gather(*tasks, return_exceptions=True)
        for session in sessions:
            await session.cleanup()
        import sys

        for name in modules:
            sys.modules.pop(name, None)
