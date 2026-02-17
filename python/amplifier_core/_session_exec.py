"""
Session execution helper for the Rust PyO3 bridge.

Extracts the execute logic from AmplifierSession.execute()
so the Rust wrapper can call it via PyO3.
"""

import logging
from typing import Any

from .utils import redact_secrets, truncate_values

logger = logging.getLogger(__name__)


def _safe_exception_str(e: BaseException) -> str:
    try:
        return str(e)
    except UnicodeDecodeError:
        return repr(e)


async def execute_session(session: Any, prompt: str) -> str:
    """Execute a prompt through the mounted orchestrator.

    Args:
        session: A session-like object with .coordinator, .config,
                 .session_id, .parent_id, .is_resumed attributes.
        prompt: User input prompt.

    Returns:
        Final response string.
    """
    coordinator = session.coordinator
    config = session.config

    from .events import (
        CANCEL_COMPLETED,
        SESSION_RESUME,
        SESSION_RESUME_DEBUG,
        SESSION_RESUME_RAW,
        SESSION_START,
        SESSION_START_DEBUG,
        SESSION_START_RAW,
    )

    # Choose event type based on whether this is a new or resumed session
    if session.is_resumed:
        event_base = SESSION_RESUME
        event_debug = SESSION_RESUME_DEBUG
        event_raw = SESSION_RESUME_RAW
    else:
        event_base = SESSION_START
        event_debug = SESSION_START_DEBUG
        event_raw = SESSION_START_RAW

    # Emit session lifecycle event from kernel (single source of truth)
    await coordinator.hooks.emit(
        event_base,
        {
            "session_id": session.session_id,
            "parent_id": session.parent_id,
        },
    )

    session_config = config.get("session", {})
    debug = session_config.get("debug", False)
    raw_debug = session_config.get("raw_debug", False)

    if debug:
        mount_plan_safe = redact_secrets(truncate_values(config))
        await coordinator.hooks.emit(
            event_debug,
            {
                "lvl": "DEBUG",
                "session_id": session.session_id,
                "mount_plan": mount_plan_safe,
            },
        )

    if debug and raw_debug:
        mount_plan_redacted = redact_secrets(config)
        await coordinator.hooks.emit(
            event_raw,
            {
                "lvl": "DEBUG",
                "session_id": session.session_id,
                "mount_plan": mount_plan_redacted,
            },
        )

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
    hooks = coordinator.get("hooks")

    try:
        result = await orchestrator.execute(
            prompt=prompt,
            context=context,
            providers=providers,
            tools=tools,
            hooks=hooks,
            coordinator=coordinator,
        )

        # Check if session was cancelled during execution
        if coordinator.cancellation.is_cancelled:
            from .events import CANCEL_COMPLETED

            await coordinator.hooks.emit(
                CANCEL_COMPLETED,
                {
                    "was_immediate": coordinator.cancellation.state == "immediate",
                },
            )

        return result

    except BaseException as e:
        if coordinator.cancellation.is_cancelled:
            from .events import CANCEL_COMPLETED

            await coordinator.hooks.emit(
                CANCEL_COMPLETED,
                {
                    "was_immediate": coordinator.cancellation.state == "immediate",
                    "error": _safe_exception_str(e),
                },
            )
            logger.info(f"Execution cancelled: {_safe_exception_str(e)}")
            raise
        else:
            logger.error(f"Execution failed: {_safe_exception_str(e)}")
            raise
