"""
Session initialization helper for the Rust PyO3 bridge.

Extracts the module-loading logic from AmplifierSession.initialize()
so the Rust wrapper can call it without reimplementing Python-specific
loader logic in Rust.
"""

import logging
from typing import Any

logger = logging.getLogger(__name__)


def _safe_exception_str(e: BaseException) -> str:
    try:
        return str(e)
    except UnicodeDecodeError:
        return repr(e)


async def initialize_session(
    config: dict[str, Any],
    coordinator: Any,
    session_id: str,
    parent_id: str | None,
) -> None:
    """Load and mount all configured modules.

    This is the module-loading logic extracted from AmplifierSession.initialize().
    The Rust session wrapper calls this to perform Python-side initialization.

    Args:
        config: The session configuration dict.
        coordinator: The RustCoordinator instance.
        session_id: The session ID.
        parent_id: The parent session ID (or None).
    """
    # Get or create the loader from the coordinator
    loader = coordinator.loader
    if loader is None:
        from .loader import ModuleLoader

        loader = ModuleLoader(coordinator=coordinator)
        coordinator.loader = loader

    # Load orchestrator (required)
    orchestrator_spec = config.get("session", {}).get("orchestrator", "loop-basic")
    if isinstance(orchestrator_spec, dict):
        orchestrator_id = orchestrator_spec.get("module", "loop-basic")
        orchestrator_source = orchestrator_spec.get("source")
        orchestrator_config = orchestrator_spec.get("config", {})
    else:
        orchestrator_id = orchestrator_spec
        orchestrator_source = config.get("session", {}).get("orchestrator_source")
        orchestrator_config = config.get("orchestrator", {}).get("config", {})

    logger.info(f"Loading orchestrator: {orchestrator_id}")
    try:
        orchestrator_mount = await loader.load(
            orchestrator_id,
            orchestrator_config,
            source_hint=orchestrator_source,
        )
        cleanup = await orchestrator_mount(coordinator)
        if cleanup:
            coordinator.register_cleanup(cleanup)
    except Exception as e:
        raise RuntimeError(
            f"Cannot initialize without orchestrator: {_safe_exception_str(e)}"
        )

    # Load context manager (required)
    context_spec = config.get("session", {}).get("context", "context-simple")
    if isinstance(context_spec, dict):
        context_id = context_spec.get("module", "context-simple")
        context_source = context_spec.get("source")
        context_config = context_spec.get("config", {})
    else:
        context_id = context_spec
        context_source = config.get("session", {}).get("context_source")
        context_config = config.get("context", {}).get("config", {})

    logger.info(f"Loading context manager: {context_id}")
    try:
        context_mount = await loader.load(
            context_id, context_config, source_hint=context_source
        )
        cleanup = await context_mount(coordinator)
        if cleanup:
            coordinator.register_cleanup(cleanup)
    except Exception as e:
        raise RuntimeError(
            f"Cannot initialize without context manager: {_safe_exception_str(e)}"
        )

    # Load providers
    for provider_config in config.get("providers", []):
        module_id = provider_config.get("module")
        if not module_id:
            continue
        try:
            logger.info(f"Loading provider: {module_id}")
            provider_mount = await loader.load(
                module_id,
                provider_config.get("config", {}),
                source_hint=provider_config.get("source"),
            )
            cleanup = await provider_mount(coordinator)
            if cleanup:
                coordinator.register_cleanup(cleanup)
        except Exception as e:
            logger.warning(
                f"Failed to load provider '{module_id}': {_safe_exception_str(e)}",
                exc_info=True,
            )

    # Load tools
    for tool_config in config.get("tools", []):
        module_id = tool_config.get("module")
        if not module_id:
            continue
        try:
            logger.info(f"Loading tool: {module_id}")
            tool_mount = await loader.load(
                module_id,
                tool_config.get("config", {}),
                source_hint=tool_config.get("source"),
            )
            cleanup = await tool_mount(coordinator)
            if cleanup:
                coordinator.register_cleanup(cleanup)
        except Exception as e:
            logger.warning(
                f"Failed to load tool '{module_id}': {_safe_exception_str(e)}",
                exc_info=True,
            )

    # Load hooks
    for hook_config in config.get("hooks", []):
        module_id = hook_config.get("module")
        if not module_id:
            continue
        try:
            logger.info(f"Loading hook: {module_id}")
            hook_mount = await loader.load(
                module_id,
                hook_config.get("config", {}),
                source_hint=hook_config.get("source"),
            )
            cleanup = await hook_mount(coordinator)
            if cleanup:
                coordinator.register_cleanup(cleanup)
        except Exception as e:
            logger.warning(
                f"Failed to load hook '{module_id}': {_safe_exception_str(e)}",
                exc_info=True,
            )

    # Emit session:fork event if this is a child session
    if parent_id:
        from .events import SESSION_FORK, SESSION_FORK_DEBUG, SESSION_FORK_RAW
        from .utils import redact_secrets, truncate_values

        await coordinator.hooks.emit(
            SESSION_FORK,
            {
                "parent": parent_id,
                "session_id": session_id,
            },
        )

        session_config = config.get("session", {})
        debug = session_config.get("debug", False)
        raw_debug = session_config.get("raw_debug", False)

        if debug:
            mount_plan_safe = redact_secrets(truncate_values(config))
            await coordinator.hooks.emit(
                SESSION_FORK_DEBUG,
                {
                    "lvl": "DEBUG",
                    "parent": parent_id,
                    "session_id": session_id,
                    "mount_plan": mount_plan_safe,
                },
            )

        if debug and raw_debug:
            mount_plan_redacted = redact_secrets(config)
            await coordinator.hooks.emit(
                SESSION_FORK_RAW,
                {
                    "lvl": "DEBUG",
                    "parent": parent_id,
                    "session_id": session_id,
                    "mount_plan": mount_plan_redacted,
                },
            )

    logger.info(f"Session {session_id} initialized successfully")


async def _wrap_initialize(coro):
    """Wrapper that awaits the initialization coroutine.

    Called by the Rust PySession.initialize() to wrap the async
    initialize_session() call. This is needed because Rust returns
    the coroutine to Python for awaiting.
    """
    await coro


async def _session_aenter(session):
    """Async context manager entry for RustSession.

    Calls session.initialize() and returns the session.
    """
    await session.initialize()
    return session
