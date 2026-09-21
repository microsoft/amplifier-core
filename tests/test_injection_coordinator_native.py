"""Exercise ordered injections through the production registry/coordinator."""

from types import SimpleNamespace

import pytest

from amplifier_core import HookRegistry
from amplifier_core._engine import RustCoordinator
from amplifier_core.models import ContextInjection, HookResult


class RecordingContext:
    def __init__(self):
        self.messages = []

    async def add_message(self, message):
        self.messages.append(message)


def coordinator(**settings):
    session = SimpleNamespace(
        session_id="injection-review",
        parent_id=None,
        config={"session": {"orchestrator": "loop-basic", **settings}},
    )
    return RustCoordinator(session)


@pytest.mark.asyncio
async def test_mixed_hooks_persist_only_durable_items_and_return_ephemeral_residual():
    registry = HookRegistry()

    async def durable(event, data):
        return HookResult(
            action="inject_context",
            context_injection="durable context",
            context_injection_role="assistant",
        )

    async def ephemeral(event, data):
        return HookResult(
            action="inject_context",
            context_injections=[ContextInjection(
                content="request reminder", role="user", ephemeral=True,
                append_to_last_tool_result=True,
                hook_name="forged", event="forged",
            )],
        )

    registry.register("tool:post", durable, name="durable-hook", priority=0)
    registry.register("tool:post", ephemeral, name="ephemeral-hook", priority=10)
    merged = await registry.emit("tool:post", {})
    assert [item.hook_name for item in merged.context_injections] == [
        "durable-hook", "ephemeral-hook",
    ]

    coord = coordinator()
    context = RecordingContext()
    await coord.mount("context", context)
    residual = await coord.process_hook_result(merged, event="tool:post")

    assert len(context.messages) == 1
    saved = context.messages[0]
    assert saved["content"] == "durable context"
    assert saved["role"] == "assistant"
    assert saved["metadata"]["hook_name"] == "durable-hook"
    assert saved["metadata"]["persisted"] is True
    assert saved["metadata"]["ephemeral"] is False
    assert residual.action == "inject_context"
    assert residual.context_injection == "request reminder"
    assert residual.context_injection_role == "user"
    assert residual.ephemeral is True
    assert residual.append_to_last_tool_result is True
    assert len(residual.context_injections) == 1
    assert residual.context_injections[0].hook_name == "ephemeral-hook"
    assert residual.context_injections[0].event == "tool:post"


@pytest.mark.asyncio
async def test_later_invalid_item_prevents_partial_context_write_and_budget_charge():
    coord = coordinator(injection_size_limit=10)
    context = RecordingContext()
    await coord.mount("context", context)
    result = HookResult(
        action="inject_context",
        context_injections=[
            ContextInjection(content="valid"),
            ContextInjection(content="x" * 11, ephemeral=True),
        ],
    )
    with pytest.raises(ValueError, match="exceeds 10 characters"):
        await coord.process_hook_result(result, event="tool:post")
    assert context.messages == []
    assert coord._current_turn_injections == 0


@pytest.mark.asyncio
async def test_all_durable_items_are_consumed_in_order_and_preserve_result_metadata():
    coord = coordinator()
    context = RecordingContext()
    await coord.mount("context", context)
    result = HookResult(
        action="inject_context", reason="original reason", suppress_output=True,
        approval_prompt="preserved metadata",
        context_injections=[
            ContextInjection(content="one", role="user"),
            ContextInjection(content="two", role="assistant"),
        ],
    )
    residual = await coord.process_hook_result(result, event="tool:post")
    assert [(m["content"], m["role"]) for m in context.messages] == [
        ("one", "user"), ("two", "assistant"),
    ]
    assert residual.action == "continue"
    assert residual.context_injection is None
    assert residual.context_injections == []
    assert residual.reason == "original reason"
    assert residual.suppress_output is True
    assert residual.approval_prompt == "preserved metadata"
