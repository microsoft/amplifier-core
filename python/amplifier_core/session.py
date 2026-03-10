"""
Amplifier session management.
The main entry point for using the Amplifier system.
"""

import logging
import uuid
from typing import TYPE_CHECKING
from typing import Any

from ._session_init import initialize_session
from .coordinator import ModuleCoordinator
from .loader import ModuleLoader
from .models import SessionStatus
from .utils import redact_secrets

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
        """Delegates to _session_init.initialize_session() — the single
        implementation shared by both AmplifierSession and RustSession."""
        if self._initialized:
            return
        # Propagate session's loader to coordinator so initialize_session()
        # uses it (RustSession sets coordinator.loader directly instead).
        self.coordinator.loader = self.loader
        await initialize_session(
            self.config, self.coordinator, self.session_id, self.parent_id
        )
        self._initialized = True

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

        from .events import SESSION_RESUME, SESSION_START

        # Choose event type based on whether this is a new or resumed session
        event_base = SESSION_RESUME if self._is_resumed else SESSION_START

        # Emit session lifecycle event from kernel (single source of truth)
        session_config = self.config.get("session", {})
        session_metadata = session_config.get("metadata", {})
        raw = session_config.get("raw", False)

        payload: dict = {
            "session_id": self.session_id,
            "parent_id": self.parent_id,
            **({"metadata": session_metadata} if session_metadata else {}),
        }
        if raw:
            payload["raw"] = redact_secrets(self.config)
        await self.coordinator.hooks.emit(event_base, payload)

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
            # Emit SESSION_END before coordinator cleanup (matches Rust behavior)
            if self._initialized:
                try:
                    from .events import SESSION_END

                    await self.coordinator.hooks.emit(
                        SESSION_END,
                        {
                            "session_id": self.session_id,
                            "status": self.status.status,
                        },
                    )
                except Exception:
                    logger.debug(
                        "Failed to emit SESSION_END during cleanup", exc_info=True
                    )

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
