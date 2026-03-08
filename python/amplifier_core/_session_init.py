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

    # Validate multi-instance providers have instance_id
    _provider_module_counts: dict[str, int] = {}
    for _pc in config.get("providers", []):
        _mid = _pc.get("module", "")
        if _mid:
            _provider_module_counts[_mid] = _provider_module_counts.get(_mid, 0) + 1

    for _pc in config.get("providers", []):
        _mid = _pc.get("module", "")
        if _provider_module_counts.get(_mid, 0) > 1 and not _pc.get("instance_id"):
            raise ValueError(
                f"Multi-instance providers require explicit 'instance_id' on each entry. "
                f"Found multiple entries for module '{_mid}' without instance_id."
            )

    # Load providers
    for provider_config in config.get("providers", []):
        module_id = provider_config.get("module")
        if not module_id:
            continue
        instance_id = provider_config.get("instance_id")  # NEW: multi-instance support
        try:
            logger.info(
                f"Loading provider: {module_id}"
                + (f" (instance: {instance_id})" if instance_id else "")
            )
            provider_mount = await loader.load(
                module_id,
                provider_config.get("config", {}),
                source_hint=provider_config.get("source"),
            )
            cleanup = await provider_mount(coordinator)
            if cleanup:
                coordinator.register_cleanup(cleanup)

            # Multi-instance remapping: if instance_id specified, remap mount name
            if instance_id:
                default_name = (
                    module_id.removeprefix("provider-")
                    if module_id.startswith("provider-")
                    else module_id
                )
                providers_dict = coordinator.get("providers") or {}
                if default_name in providers_dict and default_name != instance_id:
                    instance = providers_dict[default_name]
                    await coordinator.mount("providers", instance, name=instance_id)
                    await coordinator.unmount("providers", name=default_name)
                    logger.info(
                        f"Remapped provider '{default_name}' -> '{instance_id}'"
                    )
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
        from .events import SESSION_FORK
        from .utils import redact_secrets

        session_config = config.get("session", {})
        session_metadata = session_config.get("metadata", {})
        raw = session_config.get("raw", False)

        payload: dict = {
            "parent": parent_id,
            "session_id": session_id,
        }
        if session_metadata:
            payload["metadata"] = session_metadata
        if raw:
            payload["raw"] = redact_secrets(config)
        await coordinator.hooks.emit(SESSION_FORK, payload)

    logger.info(f"Session {session_id} initialized successfully")


async def _session_aenter(session):
    """Async context manager entry for RustSession.

    Calls session.initialize() and returns the session.
    """
    await session.initialize()
    return session
