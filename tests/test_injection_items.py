"""Regression coverage for lossless mixed hook context injections."""

from amplifier_core.models import ContextInjection, HookResult


def test_mixed_durable_and_ephemeral_injections_remain_distinct() -> None:
    result = HookResult(
        action="inject_context",
        context_injections=[
            ContextInjection(content="durable", role="assistant", ephemeral=False),
            ContextInjection(content="temporary", role="user", ephemeral=True),
        ],
    )

    assert [item.content for item in result.context_injections] == [
        "durable",
        "temporary",
    ]
    assert [item.ephemeral for item in result.context_injections] == [False, True]
    assert [item.role for item in result.context_injections] == ["assistant", "user"]