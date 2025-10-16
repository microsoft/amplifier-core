"""
Amplifier session management.
The main entry point for using the Amplifier system.
"""

import logging
import uuid
from typing import Any

from .coordinator import ModuleCoordinator
from .loader import ModuleLoader
from .models import SessionStatus

logger = logging.getLogger(__name__)


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
    ):
        """
        Initialize an Amplifier session with explicit configuration.

        Args:
            config: Required mount plan with orchestrator and context
            loader: Optional module loader (creates default if None)
            session_id: Optional session ID (generates UUID if not provided)
            parent_id: Optional parent session ID (None for top-level, UUID for child sessions)

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
        self.session_id = session_id if session_id else str(uuid.uuid4())
        self.parent_id = parent_id  # Track parent for child sessions
        self.config = config
        self.status = SessionStatus(session_id=self.session_id)
        self._initialized = False

        # Create coordinator with infrastructure context (provides IDs, config, session to modules)
        self.coordinator = ModuleCoordinator(session=self)

        # Set default fields for all events (infrastructure propagation)
        self.coordinator.hooks.set_default_fields(session_id=self.session_id, parent_id=self.parent_id)

        # Create loader with coordinator (for resolver injection)
        # Note: Resolver will be mounted during initialize() since mount() is async
        self.loader = loader or ModuleLoader(coordinator=self.coordinator)

    def _merge_configs(self, base: dict[str, Any], overlay: dict[str, Any]) -> dict[str, Any]:
        """Deep merge two config dicts."""
        result = base.copy()

        for key, value in overlay.items():
            if key in result and isinstance(result[key], dict) and isinstance(value, dict):
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
            orchestrator_id = self.config.get("session", {}).get("orchestrator", "loop-basic")
            orchestrator_source = self.config.get("session", {}).get("orchestrator_source")
            logger.info(f"Loading orchestrator: {orchestrator_id}")

            try:
                # Get orchestrator config if present
                orchestrator_config = self.config.get("orchestrator", {}).get("config", {})
                orchestrator_mount = await self.loader.load(
                    orchestrator_id, orchestrator_config, profile_source=orchestrator_source
                )
                # Note: config is already embedded in orchestrator_mount by the loader
                cleanup = await orchestrator_mount(self.coordinator)
                if cleanup:
                    self.coordinator.register_cleanup(cleanup)
            except Exception as e:
                logger.error(f"Failed to load orchestrator '{orchestrator_id}': {e}")
                raise RuntimeError(f"Cannot initialize without orchestrator: {e}")

            # Load context manager (required)
            context_id = self.config.get("session", {}).get("context", "context-simple")
            context_source = self.config.get("session", {}).get("context_source")
            logger.info(f"Loading context manager: {context_id}")

            try:
                context_config = self.config.get("context", {}).get("config", {})
                context_mount = await self.loader.load(context_id, context_config, profile_source=context_source)
                cleanup = await context_mount(self.coordinator)
                if cleanup:
                    self.coordinator.register_cleanup(cleanup)
            except Exception as e:
                logger.error(f"Failed to load context manager '{context_id}': {e}")
                raise RuntimeError(f"Cannot initialize without context manager: {e}")

            # Load providers
            for provider_config in self.config.get("providers", []):
                module_id = provider_config.get("module")
                if not module_id:
                    continue

                try:
                    logger.info(f"Loading provider: {module_id}")
                    provider_mount = await self.loader.load(
                        module_id, provider_config.get("config", {}), profile_source=provider_config.get("source")
                    )
                    cleanup = await provider_mount(self.coordinator)
                    if cleanup:
                        self.coordinator.register_cleanup(cleanup)
                except Exception as e:
                    logger.warning(f"Failed to load provider '{module_id}': {e}")

            # Load tools
            for tool_config in self.config.get("tools", []):
                module_id = tool_config.get("module")
                if not module_id:
                    continue

                try:
                    logger.info(f"Loading tool: {module_id}")
                    tool_mount = await self.loader.load(
                        module_id, tool_config.get("config", {}), profile_source=tool_config.get("source")
                    )
                    cleanup = await tool_mount(self.coordinator)
                    if cleanup:
                        self.coordinator.register_cleanup(cleanup)
                except Exception as e:
                    logger.warning(f"Failed to load tool '{module_id}': {e}")

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
                        module_id, hook_config.get("config", {}), profile_source=hook_config.get("source")
                    )
                    cleanup = await hook_mount(self.coordinator)
                    if cleanup:
                        self.coordinator.register_cleanup(cleanup)
                except Exception as e:
                    logger.warning(f"Failed to load hook '{module_id}': {e}")

            self._initialized = True

            # Emit session:fork event if this is a child session
            if self.parent_id:
                from .events import SESSION_FORK

                await self.coordinator.hooks.emit(SESSION_FORK, {"data": {"parent": self.parent_id}})

            logger.info(f"Session {self.session_id} initialized successfully")

        except Exception as e:
            logger.error(f"Session initialization failed: {e}")
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
                prompt=prompt, context=context, providers=providers, tools=tools, hooks=hooks
            )

            self.status.status = "completed"
            return result

        except Exception as e:
            self.status.status = "failed"
            self.status.last_error = {"message": str(e)}
            logger.error(f"Execution failed: {e}")
            raise

    async def cleanup(self: "AmplifierSession") -> None:
        """Clean up session resources."""
        await self.coordinator.cleanup()

    async def __aenter__(self: "AmplifierSession"):
        """Async context manager entry."""
        await self.initialize()
        return self

    async def __aexit__(self: "AmplifierSession", exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.cleanup()
