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


async def emit_debug_events(
    coordinator: Any,
    config: dict,
    session_id: str,
    event_debug: str,
    event_raw: str,
) -> None:
    """Emit debug/raw events if debug flags are set in config.

    Separated from Rust because it needs Python utilities
    (redact_secrets, truncate_values).
    """
    from .utils import redact_secrets, truncate_values

    session_config = config.get("session", {})
    debug = session_config.get("debug", False)
    raw_debug = session_config.get("raw_debug", False)

    if debug:
        mount_plan_safe = redact_secrets(truncate_values(config))
        await coordinator.hooks.emit(
            event_debug,
            {
                "lvl": "DEBUG",
                "session_id": session_id,
                "mount_plan": mount_plan_safe,
            },
        )

    if debug and raw_debug:
        mount_plan_redacted = redact_secrets(config)
        await coordinator.hooks.emit(
            event_raw,
            {
                "lvl": "DEBUG",
                "session_id": session_id,
                "mount_plan": mount_plan_redacted,
            },
        )
