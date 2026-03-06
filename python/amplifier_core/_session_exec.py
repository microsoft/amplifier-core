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
    (initialization check, event emission, cancellation, errors).

    Args:
        coordinator: The coordinator with mounted modules.
        prompt: User input prompt.

    Returns:
        Final response string from the orchestrator.

    Raises:
        RuntimeError: If required mount points are missing.
    """
    orchestrator = coordinator.get("orchestrator")
    if not orchestrator:
        raise RuntimeError("No orchestrator module mounted")

    context = coordinator.get("context")
    if not context:
        raise RuntimeError("No context manager mounted")

    providers = coordinator.get("providers")
    if not providers:
        raise RuntimeError("No providers mounted")

    # Debug: Log what we're passing to orchestrator
    logger.debug(f"Passing providers to orchestrator: {list(providers.keys())}")
    for name, provider in providers.items():
        logger.debug(f"  Provider '{name}': type={type(provider).__name__}")

    tools = coordinator.get("tools") or {}
    hooks = coordinator.hooks

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
    """Emit the base session event with an optional raw field.

    When session.raw=true, a redacted copy of the full config is included
    as the 'raw' field on the base event.

    This helper is called from the Rust PyO3 bridge's execute() path and
    handles the Python utilities (redact_secrets) needed for raw payloads.

    Args:
        coordinator: The coordinator with hooks.
        config: Full session mount plan.
        session_id: Current session ID.
        event_base: The base event name (e.g. 'session:start' or 'session:resume').
    """
    from .utils import redact_secrets

    session_config = config.get("session", {})
    raw = session_config.get("raw", False)

    if raw:
        raw_payload = redact_secrets(config)
        await coordinator.hooks.emit(
            event_base,
            {
                "session_id": session_id,
                "raw": raw_payload,
            },
        )
