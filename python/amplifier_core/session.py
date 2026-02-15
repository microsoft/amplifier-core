"""
Amplifier session management.
The main entry point for using the Amplifier system.
"""

import logging
import uuid
from typing import TYPE_CHECKING
from typing import Any

from .coordinator import ModuleCoordinator
from .loader import ModuleLoader
from .models import SessionStatus
from .utils import redact_secrets, truncate_values

if TYPE_CHECKING:
    from .approval import ApprovalSystem
    from .display import DisplaySystem

logger = logging.getLogger(__name__)


def _safe_exception_str(e: BaseException) -> str:
    """
    CRITICAL: Explicitly handle exception string conversion for Windows cp1252 compatibility.
    Default encoding can fail on non-cp1252 characters, causing a crash during error handling.
    We fall back to repr() which is safer as it escapes problematic characters.
    """
    try:
        return str(e)
    except UnicodeDecodeError:
        return repr(e)


class AmplifierSession:
    """
    A single Amplifier session tying everything together.
    This is the main entry point for users.
    """

    def __init__(
        self,
        config: dict[str, Any],
        loader: ModuleLoader | None = None,
        session_id: str | None = None,
        parent_id: str | None = None,
        approval_system: "ApprovalSystem | None" = None,
        display_system: "DisplaySystem | None" = None,
        is_resumed: bool = False,
    ):
        """
        Initialize an Amplifier session with explicit configuration.

        Args:
            config: Required mount plan with orchestrator and context
            loader: Optional module loader (creates default if None)
            session_id: Optional session ID (generates UUID if not provided)
            parent_id: Optional parent session ID (None for top-level, UUID for child sessions)
            approval_system: Optional approval system (app-layer policy)
            display_system: Optional display system (app-layer policy)
            is_resumed: Whether this session is being resumed (vs newly created).
                        Controls whether session:start or session:resume events are emitted.

        Raises:
            ValueError: If config missing required fields

        When parent_id is set, the session is a child session (forked from parent).
        The kernel will emit a session:fork event during initialization and include
        parent_id in all events for lineage tracking.
        """
        # Validate required config fields
        if not config:
            raise ValueError("Configuration is required")
        if not config.get("session", {}).get("orchestrator"):
            raise ValueError("Configuration must specify session.orchestrator")
        if not config.get("session", {}).get("context"):
            raise ValueError("Configuration must specify session.context")

        # Use provided session_id or generate a new one
        # Track whether this is a resumed session (explicit parameter from app layer)
        self._is_resumed = is_resumed
        self.session_id = session_id if session_id else str(uuid.uuid4())
        self.parent_id = parent_id  # Track parent for child sessions
        self.config = config
        self.status = SessionStatus(session_id=self.session_id)
        self._initialized = False

        # Create coordinator with infrastructure context and injected UX systems
        self.coordinator = ModuleCoordinator(
            session=self,
            approval_system=approval_system,
            display_system=display_system,
        )

        # Set default fields for all events (infrastructure propagation)
        self.coordinator.hooks.set_default_fields(
            session_id=self.session_id, parent_id=self.parent_id
        )

        # Create loader with coordinator (for resolver injection)
        self.loader = loader or ModuleLoader(coordinator=self.coordinator)

    def _merge_configs(
        self, base: dict[str, Any], overlay: dict[str, Any]
    ) -> dict[str, Any]:
        """Deep merge two config dicts."""
        result = base.copy()

        for key, value in overlay.items():
            if (
                key in result
                and isinstance(result[key], dict)
                and isinstance(value, dict)
            ):
                result[key] = self._merge_configs(result[key], value)
            else:
                result[key] = value

        return result

    async def initialize(self) -> None:
        """
        Load and mount all configured modules.
        The orchestrator module determines behavior.
        """
        if self._initialized:
            return

        # Note: Module source resolver should be mounted by app layer before initialization
        # The loader will use entry point fallback if no resolver is mounted

        try:
            # Load orchestrator (required)
            # Handle both dict (ModuleConfig) and string formats
            orchestrator_spec = self.config.get("session", {}).get(
                "orchestrator", "loop-basic"
            )
            if isinstance(orchestrator_spec, dict):
                orchestrator_id = orchestrator_spec.get("module", "loop-basic")
                orchestrator_source = orchestrator_spec.get("source")
                orchestrator_config = orchestrator_spec.get("config", {})
            else:
                orchestrator_id = orchestrator_spec
                orchestrator_source = self.config.get("session", {}).get(
                    "orchestrator_source"
                )
                orchestrator_config = self.config.get("orchestrator", {}).get(
                    "config", {}
                )

            logger.info(f"Loading orchestrator: {orchestrator_id}")

            try:
                orchestrator_mount = await self.loader.load(
                    orchestrator_id,
                    orchestrator_config,
                    source_hint=orchestrator_source,
                )
                # Note: config is already embedded in orchestrator_mount by the loader
                cleanup = await orchestrator_mount(self.coordinator)
                if cleanup:
                    self.coordinator.register_cleanup(cleanup)
            except Exception as e:
                logger.error(
                    f"Failed to load orchestrator '{orchestrator_id}': {_safe_exception_str(e)}"
                )
                raise RuntimeError(
                    f"Cannot initialize without orchestrator: {_safe_exception_str(e)}"
                )

            # Load context manager (required)
            # Handle both dict (ModuleConfig) and string formats
            context_spec = self.config.get("session", {}).get(
                "context", "context-simple"
            )
            if isinstance(context_spec, dict):
                context_id = context_spec.get("module", "context-simple")
                context_source = context_spec.get("source")
                context_config = context_spec.get("config", {})
            else:
                context_id = context_spec
                context_source = self.config.get("session", {}).get("context_source")
                context_config = self.config.get("context", {}).get("config", {})

            logger.info(f"Loading context manager: {context_id}")

            try:
                context_mount = await self.loader.load(
                    context_id, context_config, source_hint=context_source
                )
                cleanup = await context_mount(self.coordinator)
                if cleanup:
                    self.coordinator.register_cleanup(cleanup)
            except Exception as e:
                logger.error(
                    f"Failed to load context manager '{context_id}': {_safe_exception_str(e)}"
                )
                raise RuntimeError(
                    f"Cannot initialize without context manager: {_safe_exception_str(e)}"
                )

            # Load providers
            for provider_config in self.config.get("providers", []):
                module_id = provider_config.get("module")
                if not module_id:
                    continue

                try:
                    logger.info(f"Loading provider: {module_id}")
                    provider_mount = await self.loader.load(
                        module_id,
                        provider_config.get("config", {}),
                        source_hint=provider_config.get("source"),
                    )
                    cleanup = await provider_mount(self.coordinator)
                    if cleanup:
                        self.coordinator.register_cleanup(cleanup)
                except Exception as e:
                    logger.warning(
                        f"Failed to load provider '{module_id}': {_safe_exception_str(e)}",
                        exc_info=True,
                    )

            # Load tools
            for tool_config in self.config.get("tools", []):
                module_id = tool_config.get("module")
                if not module_id:
                    continue

                try:
                    logger.info(f"Loading tool: {module_id}")
                    tool_mount = await self.loader.load(
                        module_id,
                        tool_config.get("config", {}),
                        source_hint=tool_config.get("source"),
                    )
                    cleanup = await tool_mount(self.coordinator)
                    if cleanup:
                        self.coordinator.register_cleanup(cleanup)
                except Exception as e:
                    logger.warning(
                        f"Failed to load tool '{module_id}': {_safe_exception_str(e)}",
                        exc_info=True,
                    )

            # Note: agents section is app-layer data (config overlays), not modules to mount
            # The kernel passes agents through in the mount plan without interpretation

            # Load hooks
            for hook_config in self.config.get("hooks", []):
                module_id = hook_config.get("module")
                if not module_id:
                    continue

                try:
                    logger.info(f"Loading hook: {module_id}")
                    hook_mount = await self.loader.load(
                        module_id,
                        hook_config.get("config", {}),
                        source_hint=hook_config.get("source"),
                    )
                    cleanup = await hook_mount(self.coordinator)
                    if cleanup:
                        self.coordinator.register_cleanup(cleanup)
                except Exception as e:
                    logger.warning(
                        f"Failed to load hook '{module_id}': {_safe_exception_str(e)}",
                        exc_info=True,
                    )

            self._initialized = True

            # Emit session:fork event if this is a child session
            if self.parent_id:
                from .events import SESSION_FORK, SESSION_FORK_DEBUG, SESSION_FORK_RAW

                await self.coordinator.hooks.emit(
                    SESSION_FORK,
                    {
                        "parent": self.parent_id,
                        "session_id": self.session_id,
                    },
                )

                # Debug config from mount plan
                session_config = self.config.get("session", {})
                debug = session_config.get("debug", False)
                raw_debug = session_config.get("raw_debug", False)

                if debug:
                    mount_plan_safe = redact_secrets(truncate_values(self.config))
                    await self.coordinator.hooks.emit(
                        SESSION_FORK_DEBUG,
                        {
                            "lvl": "DEBUG",
                            "parent": self.parent_id,
                            "session_id": self.session_id,
                            "mount_plan": mount_plan_safe,
                        },
                    )

                if debug and raw_debug:
                    mount_plan_redacted = redact_secrets(self.config)
                    await self.coordinator.hooks.emit(
                        SESSION_FORK_RAW,
                        {
                            "lvl": "DEBUG",
                            "parent": self.parent_id,
                            "session_id": self.session_id,
                            "mount_plan": mount_plan_redacted,
                        },
                    )

            logger.info(f"Session {self.session_id} initialized successfully")

        except Exception as e:
            logger.error(f"Session initialization failed: {_safe_exception_str(e)}")
            raise

    async def execute(self, prompt: str) -> str:
        """
        Execute a prompt using the mounted orchestrator.

        Args:
            prompt: User input prompt

        Returns:
            Final response string
        """
        if not self._initialized:
            await self.initialize()

        from .events import (
            SESSION_RESUME,
            SESSION_RESUME_DEBUG,
            SESSION_RESUME_RAW,
            SESSION_START,
            SESSION_START_DEBUG,
            SESSION_START_RAW,
        )

        # Choose event type based on whether this is a new or resumed session
        if self._is_resumed:
            event_base = SESSION_RESUME
            event_debug = SESSION_RESUME_DEBUG
            event_raw = SESSION_RESUME_RAW
        else:
            event_base = SESSION_START
            event_debug = SESSION_START_DEBUG
            event_raw = SESSION_START_RAW

        # Emit session lifecycle event from kernel (single source of truth)
        await self.coordinator.hooks.emit(
            event_base,
            {
                "session_id": self.session_id,
                "parent_id": self.parent_id,
            },
        )

        session_config = self.config.get("session", {})
        debug = session_config.get("debug", False)
        raw_debug = session_config.get("raw_debug", False)

        if debug:
            mount_plan_safe = redact_secrets(truncate_values(self.config))
            await self.coordinator.hooks.emit(
                event_debug,
                {
                    "lvl": "DEBUG",
                    "session_id": self.session_id,
                    "mount_plan": mount_plan_safe,
                },
            )

        if debug and raw_debug:
            mount_plan_redacted = redact_secrets(self.config)
            await self.coordinator.hooks.emit(
                event_raw,
                {
                    "lvl": "DEBUG",
                    "session_id": self.session_id,
                    "mount_plan": mount_plan_redacted,
                },
            )

        orchestrator = self.coordinator.get("orchestrator")
        if not orchestrator:
            raise RuntimeError("No orchestrator module mounted")

        context = self.coordinator.get("context")
        if not context:
            raise RuntimeError("No context manager mounted")

        providers = self.coordinator.get("providers")
        if not providers:
            raise RuntimeError("No providers mounted")

        # Debug: Log what we're passing to orchestrator
        logger.debug(f"Passing providers to orchestrator: {list(providers.keys())}")
        for name, provider in providers.items():
            logger.debug(f"  Provider '{name}': type={type(provider).__name__}")

        tools = self.coordinator.get("tools") or {}
        hooks = self.coordinator.get("hooks")

        try:
            self.status.status = "running"

            result = await orchestrator.execute(
                prompt=prompt,
                context=context,
                providers=providers,
                tools=tools,
                hooks=hooks,
                coordinator=self.coordinator,  # NEW: Pass coordinator for hook result processing
            )

            # Check if session was cancelled during execution
            if self.coordinator.cancellation.is_cancelled:
                self.status.status = "cancelled"
                # Emit cancel:completed event
                from .events import CANCEL_COMPLETED

                await self.coordinator.hooks.emit(
                    CANCEL_COMPLETED,
                    {
                        "was_immediate": self.coordinator.cancellation.is_immediate,
                    },
                )
            else:
                self.status.status = "completed"
            return result

        except BaseException as e:
            # Catch BaseException to handle asyncio.CancelledError (a BaseException
            # subclass since Python 3.9). All paths re-raise after status tracking.
            if self.coordinator.cancellation.is_cancelled:
                self.status.status = "cancelled"
                from .events import CANCEL_COMPLETED

                await self.coordinator.hooks.emit(
                    CANCEL_COMPLETED,
                    {
                        "was_immediate": self.coordinator.cancellation.is_immediate,
                        "error": _safe_exception_str(e),
                    },
                )
                logger.info(f"Execution cancelled: {_safe_exception_str(e)}")
                raise
            else:
                self.status.status = "failed"
                self.status.last_error = {"message": _safe_exception_str(e)}
                logger.error(f"Execution failed: {_safe_exception_str(e)}")
                raise

    async def cleanup(self: "AmplifierSession") -> None:
        """Clean up session resources."""
        try:
            await self.coordinator.cleanup()
        finally:
            # Clean up sys.path modifications - must always run even if
            # coordinator cleanup raises (e.g., asyncio.CancelledError)
            if self.loader:
                self.loader.cleanup()

    async def __aenter__(self: "AmplifierSession"):
        """Async context manager entry."""
        await self.initialize()
        return self

    async def __aexit__(self: "AmplifierSession", exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.cleanup()
