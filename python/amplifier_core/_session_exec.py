"""
Session execution helper for the Rust PyO3 bridge.

Thin helper that handles the orchestrator call boundary.
Rust owns the control flow (initialization check, event emission,
cancellation checking, error handling). This helper handles:
- Getting mount points from the coordinator
- Calling orchestrator.execute() with the correct kwargs
"""

import logging
from typing import Any

logger = logging.getLogger(__name__)


async def run_orchestrator(coordinator: Any, prompt: str) -> str:
    """Call the mounted orchestrator's execute() method.

    This is the Python boundary call. Rust handles everything else
    (initialization check, event emission, cancellation, errors,
    and mount-point validation).

    Args:
        coordinator: The coordinator with mounted modules.
        prompt: User input prompt.

    Returns:
        Final response string from the orchestrator.
    """
    # Mount-point presence is validated by Rust PySession::execute()
    # before this function is called. We just retrieve and call.
    orchestrator = coordinator.get("orchestrator")
    context = coordinator.get("context")
    providers = coordinator.get("providers") or {}
    tools = coordinator.get("tools") or {}
    hooks = coordinator.hooks

    logger.debug(f"Passing providers to orchestrator: {list(providers.keys())}")
    for name, provider in providers.items():
        logger.debug(f"  Provider '{name}': type={type(provider).__name__}")

    result = await orchestrator.execute(
        prompt=prompt,
        context=context,
        providers=providers,
        tools=tools,
        hooks=hooks,
        coordinator=coordinator,
    )

    return result


async def emit_raw_field_if_configured(
    coordinator: Any,
    config: dict,
    session_id: str,
    event_base: str,
) -> None:
    """Emit a dedicated session:config event with the full redacted config.

    When session.raw=true, a redacted copy of the full config is emitted
    as a separate ``session:config`` event.  This avoids duplicating the
    base session event (e.g. ``session:start``) which the Rust kernel
    already emitted synchronously before calling this helper.

    The ``event_base`` parameter is retained for API compatibility but is
    NOT used as the emitted event name.  Consumers that need the raw mount
    plan should subscribe to ``session:config`` rather than ``session:start``.

    Args:
        coordinator: The coordinator with hooks.
        config: Full session mount plan.
        session_id: Current session ID.
        event_base: Reserved (kept for API compatibility; not used as event name).
    """
    from .utils import redact_secrets

    session_config = config.get("session", {})
    raw = session_config.get("raw", False)

    if raw:
        raw_payload = redact_secrets(config)
        await coordinator.hooks.emit(
            "session:config",
            {
                "session_id": session_id,
                "raw": raw_payload,
            },
        )
