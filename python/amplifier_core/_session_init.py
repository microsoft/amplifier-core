"""
Session initialization helper for the Rust PyO3 bridge.

Extracts the module-loading logic from AmplifierSession.initialize()
so the Rust wrapper can call it without reimplementing Python-specific
loader logic in Rust.
"""

import logging
from copy import deepcopy
from typing import Any

logger = logging.getLogger(__name__)


def _safe_exception_str(e: BaseException) -> str:
    try:
        return str(e)
    except UnicodeDecodeError:
        return repr(e)


async def _emit_module_load_failed(
    coordinator: Any,
    module_type: str,
    module_id: str,
    error: BaseException,
    *,
    instance_id: str | None = None,
) -> None:
    """Emit the module:load_failed observability event for a provider, tool,
    or hook that raised during load/mount.

    This is a mechanism only: the kernel makes the failure observable via the
    canonical event stream. Event subscribers cannot abort initialization:
    their failures are isolated from the original WARNING log. Provider abort
    or recovery policy belongs to the optional provider.load_failure capability.
    """
    from .events import MODULE_LOAD_FAILED
    from .loader import module_failure_reason

    try:
        await coordinator.hooks.emit(
            MODULE_LOAD_FAILED,
            {
                "module_type": module_type,
                "module_id": module_id,
                "error": _safe_exception_str(error),
                "reason_code": module_failure_reason(error),
                **({"instance_id": instance_id} if instance_id else {}),
            },
        )
    except Exception:
        pass  # Event emission failure must not suppress the original warning


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
            coordinator=coordinator,
        )
        cleanup = await orchestrator_mount(coordinator)
        if cleanup:
            coordinator.register_cleanup(cleanup)
        # B1 fix: enqueue on_session_ready ONLY after successful mount
        if on_sr := getattr(orchestrator_mount, "__on_session_ready__", None):
            loader.enqueue_on_session_ready(on_sr[0], on_sr[1])
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
            context_id,
            context_config,
            source_hint=context_source,
            coordinator=coordinator,
        )
        cleanup = await context_mount(coordinator)
        if cleanup:
            coordinator.register_cleanup(cleanup)
        # B1 fix: enqueue on_session_ready ONLY after successful mount
        if on_sr := getattr(context_mount, "__on_session_ready__", None):
            loader.enqueue_on_session_ready(on_sr[0], on_sr[1])
    except Exception as e:
        raise RuntimeError(
            f"Cannot initialize without context manager: {_safe_exception_str(e)}"
        )

    # Validate multi-instance providers: at most ONE entry per module may omit instance_id.
    # That one entry is the "default" instance that keeps the provider's default mount name.
    # All additional entries must have an explicit instance_id to avoid collision.
    _provider_module_counts: dict[str, int] = {}
    _provider_no_id_counts: dict[str, int] = {}
    for _pc in config.get("providers", []):
        _mid = _pc.get("module", "")
        if _mid:
            _provider_module_counts[_mid] = _provider_module_counts.get(_mid, 0) + 1
            if not _pc.get("instance_id"):
                _provider_no_id_counts[_mid] = _provider_no_id_counts.get(_mid, 0) + 1

    for _mid, _no_id_count in _provider_no_id_counts.items():
        if _provider_module_counts.get(_mid, 0) > 1 and _no_id_count > 1:
            raise ValueError(
                f"Multi-instance providers require explicit 'instance_id' on each "
                f"additional entry. Found {_no_id_count} entries for module '{_mid}' "
                f"without instance_id (at most 1 allowed as the default instance)."
            )

    # Load providers
    for provider_config in config.get("providers", []):
        module_id = provider_config.get("module")
        if not module_id:
            continue
        instance_id = provider_config.get("instance_id")  # multi-instance support
        before_providers = dict(coordinator.get("providers") or {})
        try:
            before_load = coordinator.get_capability("provider.before_load")
            if before_load is not None:
                await before_load(coordinator, deepcopy(provider_config))
            logger.info(
                f"Loading provider: {module_id}"
                + (f" (instance: {instance_id})" if instance_id else "")
            )

            # Snapshot: save any existing provider at the default mount name before
            # loading. The new provider will self-mount there and may overwrite it.
            existing_at_default: object | None = None
            if instance_id:
                _default_name = (
                    module_id.removeprefix("provider-")
                    if module_id.startswith("provider-")
                    else module_id
                )
                _snap_dict = coordinator.get("providers") or {}
                existing_at_default = _snap_dict.get(_default_name)

            provider_mount = await loader.load(
                module_id,
                provider_config.get("config", {}),
                source_hint=provider_config.get("source"),
                coordinator=coordinator,
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
                    new_instance = providers_dict[default_name]
                    await coordinator.mount("providers", new_instance, name=instance_id)
                    # Restore the previous occupant if the self-mount overwrote it
                    if (
                        existing_at_default is not None
                        and existing_at_default is not new_instance
                    ):
                        await coordinator.mount(
                            "providers", existing_at_default, name=default_name
                        )
                    else:
                        await coordinator.unmount("providers", name=default_name)
                    logger.info(
                        f"Remapped provider '{default_name}' -> '{instance_id}'"
                    )
            # Readiness belongs to a successfully mounted and remapped instance.
            if on_sr := getattr(provider_mount, "__on_session_ready__", None):
                loader.enqueue_on_session_ready(on_sr[0], on_sr[1])
        except Exception as e:
            logger.warning(
                f"Failed to load provider '{module_id}': {_safe_exception_str(e)}",
                exc_info=True,
            )
            # A provider may mount and then raise. Roll back this attempt before
            # any host failure policy runs so an unrelated account is not lost.
            for name in set(coordinator.get("providers") or {}) - set(before_providers):
                await coordinator.unmount("providers", name=name)
            # A failed attempt can remove a previous entry as well as overwrite
            # it. Rebuild only in that case so default iteration order survives.
            if list(coordinator.get("providers") or {}) != list(before_providers):
                for name in list(coordinator.get("providers") or {}):
                    await coordinator.unmount("providers", name=name)
            for name, previous in before_providers.items():
                if (coordinator.get("providers") or {}).get(name) is not previous:
                    await coordinator.mount("providers", previous, name=name)
            await _emit_module_load_failed(
                coordinator, "provider", module_id, e, instance_id=instance_id
            )
            # Optional app policy, installed before initialization. Unlike an
            # observability subscriber, this callback can deliberately abort.
            # The kernel does not select another provider or change config.
            handler = coordinator.get_capability("provider.load_failure")
            if handler is not None:
                await handler(coordinator, deepcopy(provider_config), e)

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
                coordinator=coordinator,
            )
            cleanup = await tool_mount(coordinator)
            if cleanup:
                coordinator.register_cleanup(cleanup)
            # B1 fix: enqueue on_session_ready ONLY after successful mount
            if on_sr := getattr(tool_mount, "__on_session_ready__", None):
                loader.enqueue_on_session_ready(on_sr[0], on_sr[1])
        except Exception as e:
            logger.warning(
                f"Failed to load tool '{module_id}': {_safe_exception_str(e)}",
                exc_info=True,
            )
            await _emit_module_load_failed(coordinator, "tool", module_id, e)

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
                coordinator=coordinator,
            )
            cleanup = await hook_mount(coordinator)
            if cleanup:
                coordinator.register_cleanup(cleanup)
            # B1 fix: enqueue on_session_ready ONLY after successful mount
            if on_sr := getattr(hook_mount, "__on_session_ready__", None):
                loader.enqueue_on_session_ready(on_sr[0], on_sr[1])
        except Exception as e:
            logger.warning(
                f"Failed to load hook '{module_id}': {_safe_exception_str(e)}",
                exc_info=True,
            )
            await _emit_module_load_failed(coordinator, "hook", module_id, e)

    # Phase 6 — on_session_ready callbacks
    # Called after ALL modules have been mounted. Each callback receives the
    # fully-composed coordinator. Failures are non-fatal: caught, logged as
    # WARNING with exc_info=True, and do not block remaining callbacks.
    # B4 fix: get_on_session_ready_queue() is a sync method — call it directly.
    on_session_ready_queue = loader.get_on_session_ready_queue()
    if on_session_ready_queue:
        logger.info(
            f"Dispatching {len(on_session_ready_queue)} on_session_ready callbacks"
        )
    for module_id, on_session_ready_fn in on_session_ready_queue:
        try:
            await on_session_ready_fn(coordinator)
        except Exception as e:
            logger.warning(
                f"on_session_ready for '{module_id}' raised: {_safe_exception_str(e)}",
                exc_info=True,
            )
            from .events import MODULE_ON_SESSION_READY_FAILED

            try:
                await coordinator.hooks.emit(
                    MODULE_ON_SESSION_READY_FAILED,
                    {"module_id": module_id, "error": _safe_exception_str(e)},
                )
            except Exception:
                pass  # Event emission failure must not suppress the original warning

    # B7 fix: drain queue after dispatch to prevent double-dispatch on re-use
    loader.clear_on_session_ready_queue()

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
