"""
Module coordination system - the heart of amplifier-core.

Coordinator provides infrastructure context to all modules including:
- Identity: session_id, parent_id (and future: turn_id, span_id)
- Configuration: mount plan access
- Session reference: for spawning child sessions
- Module loader: for dynamic loading
- Hook result processing: routing hook actions to subsystems

This embodies kernel philosophy's "minimal context plumbing" - providing
identifiers and basic state necessary to make module boundaries work.
"""

import asyncio
import inspect
import logging
from collections.abc import Awaitable
from collections.abc import Callable
from datetime import datetime
from typing import TYPE_CHECKING
from typing import Any

from .approval import ApprovalSystem
from .approval import ApprovalTimeoutError
from .cancellation import CancellationToken
from .display import DisplaySystem
from .hooks import HookRegistry
from .models import HookResult

if TYPE_CHECKING:
    from .loader import ModuleLoader
    from .session import AmplifierSession

logger = logging.getLogger(__name__)

# Injection limits are configurable policy via session config
# Default: None (unlimited) - kernel provides mechanism, not policy


class ModuleCoordinator:
    """
    Central coordination and infrastructure context for all modules.

    Provides:
    - Mount points for module attachment
    - Infrastructure context (IDs, config, session reference)
    - Capability registry for inter-module communication
    - Event system with default field injection
    """

    def __init__(
        self: "ModuleCoordinator",
        session: "AmplifierSession",
        approval_system: "ApprovalSystem | None" = None,
        display_system: "DisplaySystem | None" = None,
    ):
        """
        Initialize coordinator with session providing infrastructure context.

        Args:
            session: Parent AmplifierSession providing infrastructure
            approval_system: Optional approval system (app-layer policy)
            display_system: Optional display system (app-layer policy)
        """
        self._session = session  # Infrastructure reference

        self.mount_points = {
            "orchestrator": None,  # Single orchestrator
            "providers": {},  # Multiple providers by name
            "tools": {},  # Multiple tools by name
            "context": None,  # Single context manager
            "hooks": HookRegistry(),  # Hook registry (built-in)
            "module-source-resolver": None,  # Optional custom source resolver (kernel extension point)
        }
        self._cleanup_functions = []
        self._capabilities = {}  # Capability registry for inter-module communication
        self.channels: dict[
            str, list[dict]
        ] = {}  # Contribution channels for aggregation

        # Make hooks accessible as an attribute for backward compatibility
        self.hooks = self.mount_points["hooks"]

        # Hook result processing subsystems (injected by app layer)
        self.approval_system = approval_system
        self.display_system = display_system
        self._current_turn_injections = 0  # Token budget tracking

        # Cancellation support - cooperative cancellation mechanism
        # Kernel provides the token (mechanism), app layer decides when to cancel (policy)
        self.cancellation = CancellationToken()

        # Log warnings if systems not provided (kernel doesn't decide fallback - that's policy)
        if self.approval_system is None:
            logger.warning("No approval system provided - approval requests will fail")
        if self.display_system is None:
            logger.warning(
                "No display system provided - hook messages will be logged only"
            )

    @property
    def session(self) -> "AmplifierSession":
        """Parent session reference (infrastructure for spawning children)."""
        return self._session

    @property
    def session_id(self) -> str:
        """Current session ID (infrastructure for persistence/correlation)."""
        return self._session.session_id

    @property
    def parent_id(self) -> str | None:
        """Parent session ID for child sessions (infrastructure for lineage tracking)."""
        return self._session.parent_id

    @property
    def injection_budget_per_turn(self) -> int | None:
        """
        Get injection budget from session config (policy).

        Returns:
            Token budget per turn, or None for unlimited.
            Default: None (unlimited) - kernel provides mechanism, not policy.
        """
        return self._session.config.get("session", {}).get("injection_budget_per_turn")

    @property
    def injection_size_limit(self) -> int | None:
        """
        Get per-injection size limit from session config (policy).

        Returns:
            Byte limit per injection, or None for unlimited.
            Default: None (unlimited) - kernel provides mechanism, not policy.
        """
        return self._session.config.get("session", {}).get("injection_size_limit")

    @property
    def config(self) -> dict:
        """
        Session configuration/mount plan (infrastructure).

        Includes:
        - session: orchestrator and context settings
        - providers, tools, hooks: module configurations
        - agents: config overlays for sub-session spawning (app-layer data)
        """
        return self._session.config

    @property
    def loader(self) -> "ModuleLoader":
        """Module loader (infrastructure for dynamic module loading)."""
        return self._session.loader

    async def mount(
        self, mount_point: str, module: Any, name: str | None = None
    ) -> None:
        """
        Mount a module at a specific mount point.

        Args:
            mount_point: Where to mount ('orchestrator', 'providers', 'tools', etc.)
            module: The module instance to mount
            name: Optional name for multi-module mount points
        """
        if mount_point not in self.mount_points:
            raise ValueError(f"Unknown mount point: {mount_point}")

        if mount_point in ["orchestrator", "context", "module-source-resolver"]:
            # Single module mount points
            if self.mount_points[mount_point] is not None:
                logger.warning(f"Replacing existing {mount_point}")
            self.mount_points[mount_point] = module
            logger.info(f"Mounted {module.__class__.__name__} at {mount_point}")

        elif mount_point in ["providers", "tools", "agents"]:
            # Multi-module mount points
            if name is None:
                # Try to get name from module
                if hasattr(module, "name"):
                    name = module.name
                else:
                    raise ValueError(f"Name required for {mount_point}")

            self.mount_points[mount_point][name] = module
            logger.info(
                f"Mounted {module.__class__.__name__} '{name}' at {mount_point}"
            )

        elif mount_point == "hooks":
            raise ValueError(
                "Hooks should be registered directly with the HookRegistry"
            )

    async def unmount(self, mount_point: str, name: str | None = None) -> None:
        """
        Unmount a module from a mount point.

        Args:
            mount_point: Where to unmount from
            name: Name for multi-module mount points
        """
        if mount_point not in self.mount_points:
            raise ValueError(f"Unknown mount point: {mount_point}")

        if mount_point in ["orchestrator", "context", "module-source-resolver"]:
            self.mount_points[mount_point] = None
            logger.info(f"Unmounted {mount_point}")

        elif mount_point in ["providers", "tools", "agents"]:
            if name is None:
                raise ValueError(f"Name required to unmount from {mount_point}")
            if name in self.mount_points[mount_point]:
                del self.mount_points[mount_point][name]
                logger.info(f"Unmounted '{name}' from {mount_point}")

    def get(self, mount_point: str, name: str | None = None) -> Any:
        """
        Get a mounted module.

        Args:
            mount_point: Mount point to get from
            name: Name for multi-module mount points

        Returns:
            The mounted module or dict of modules
        """
        if mount_point not in self.mount_points:
            raise ValueError(f"Unknown mount point: {mount_point}")

        if mount_point in [
            "orchestrator",
            "context",
            "hooks",
            "module-source-resolver",
        ]:
            return self.mount_points[mount_point]

        if mount_point in ["providers", "tools", "agents"]:
            if name is None:
                # Return all modules at this mount point
                return self.mount_points[mount_point]
            return self.mount_points[mount_point].get(name)
        return None

    def register_cleanup(self, cleanup_fn):
        """Register a cleanup function to be called on shutdown."""
        self._cleanup_functions.append(cleanup_fn)

    def register_capability(self, name: str, value: Any) -> None:
        """
        Register a capability that other modules can access.

        Capabilities provide a mechanism for inter-module communication
        without direct dependencies.

        Args:
            name: Capability name (e.g., 'agents.list', 'agents.get')
            value: The capability (typically a callable)
        """
        self._capabilities[name] = value
        logger.debug(f"Registered capability: {name}")

    def get_capability(self, name: str) -> Any | None:
        """
        Get a registered capability.

        Args:
            name: Capability name

        Returns:
            The capability if registered, None otherwise
        """
        return self._capabilities.get(name)

    def register_contributor(
        self,
        channel: str,
        name: str,
        callback: Callable[[], Any] | Callable[[], Awaitable[Any]],
    ) -> None:
        """
        Register contributor to named channel.

        Generic mechanism - kernel doesn't interpret channels or contributions.

        Args:
            channel: Channel name (e.g., 'observability.events', 'capabilities')
            name: Module name for debugging (e.g., 'tool-filesystem')
            callback: Callable that returns contribution (or None). Can be sync or async.

        Example:
            coordinator.register_contributor(
                'observability.events',
                'tool-task',
                lambda: ['task:agent_spawned', 'task:agent_completed']
            )
        """
        if channel not in self.channels:
            self.channels[channel] = []

        self.channels[channel].append({"name": name, "callback": callback})

        logger.debug(f"Registered contributor '{name}' to channel '{channel}'")

    async def collect_contributions(self, channel: str) -> list[Any]:
        """
        Collect contributions from channel.

        Returns raw contributions - caller interprets.

        Args:
            channel: Channel name

        Returns:
            List of contributions (None filtered out)

        Example:
            events = await coordinator.collect_contributions('observability.events')
            # Returns: [['task:spawned'], ['session:start'], ...]
        """
        contributions = []

        for contributor in self.channels.get(channel, []):
            try:
                callback = contributor["callback"]
                # Handle both sync and async callables
                if inspect.iscoroutinefunction(callback):
                    result = await callback()
                else:
                    result = callback()
                    # If the result is a coroutine, await it
                    if inspect.iscoroutine(result):
                        result = await result

                if result is not None:
                    contributions.append(result)
            except asyncio.CancelledError:
                # CancelledError is a BaseException (Python 3.9+) - catch specifically.
                # Stop collecting (honor cancellation signal) and return what we have.
                logger.warning(
                    f"Collection cancelled during contributor "
                    f"'{contributor['name']}' on channel '{channel}'"
                )
                break
            except Exception as e:
                logger.warning(
                    f"Contributor '{contributor['name']}' on channel '{channel}' failed: {e}"
                )

        return contributions

    async def cleanup(self):
        """Call all registered cleanup functions."""
        first_fatal = None
        for cleanup_fn in reversed(self._cleanup_functions):
            try:
                if callable(cleanup_fn):
                    if inspect.iscoroutinefunction(cleanup_fn):
                        await cleanup_fn()
                    else:
                        result = cleanup_fn()
                        if inspect.iscoroutine(result):
                            await result
            except BaseException as e:
                # Catch BaseException to survive asyncio.CancelledError (a BaseException
                # subclass since Python 3.9) so remaining cleanup functions still run.
                # Track fatal exceptions (KeyboardInterrupt, SystemExit) for re-raise
                # after all cleanup completes.
                logger.error(f"Error during cleanup: {e}")
                if first_fatal is None and not isinstance(e, Exception):
                    first_fatal = e
        if first_fatal is not None:
            raise first_fatal

    def reset_turn(self):
        """Reset per-turn tracking. Call at turn boundaries."""
        self._current_turn_injections = 0
        # Note: We do NOT reset cancellation here - cancellation persists across turns
        # The app layer decides when to reset cancellation (e.g., on new session)

    async def request_cancel(self, immediate: bool = False) -> None:
        """
        Request session cancellation.

        This is the kernel MECHANISM for cancellation. The app layer (CLI)
        decides WHEN to call this (e.g., on SIGINT).

        Args:
            immediate: If True, stop immediately (synthesize tool results).
                       If False, wait for current tools to complete gracefully.

        Emits:
            cancel:requested event with level and running tool info
        """
        from .events import CANCEL_REQUESTED

        if immediate:
            changed = self.cancellation.request_immediate()
            level = "immediate"
        else:
            changed = self.cancellation.request_graceful()
            level = "graceful"

        if changed:
            await self.hooks.emit(
                CANCEL_REQUESTED,
                {
                    "level": level,
                    "running_tools": list(self.cancellation.running_tools),
                    "running_tool_names": self.cancellation.running_tool_names,
                },
            )

            # Trigger any registered cancellation callbacks
            await self.cancellation.trigger_callbacks()

    async def process_hook_result(
        self, result: HookResult, event: str, hook_name: str = "unknown"
    ) -> HookResult:
        """
        Process HookResult and route actions to appropriate subsystems.

        Handles:
        - Context injection (route to context manager)
        - Approval requests (delegate to approval system)
        - User messages (route to display system)
        - Output suppression (set flag for filtering)

        Args:
            result: HookResult from hook execution
            event: Event name that triggered hook
            hook_name: Name of hook for logging/audit

        Returns:
            Processed HookResult (may be modified by approval flow)
        """
        # 1. Handle context injection
        if result.action == "inject_context" and result.context_injection:
            await self._handle_context_injection(result, hook_name, event)

        # 2. Handle approval request
        if result.action == "ask_user":
            return await self._handle_approval_request(result, hook_name)

        # 3. Handle user message (separate from context injection)
        if result.user_message:
            self._handle_user_message(result, hook_name)

        # 4. Output suppression handled by orchestrator (just log)
        if result.suppress_output:
            logger.debug(f"Hook '{hook_name}' requested output suppression")

        return result

    async def _handle_context_injection(
        self, result: HookResult, hook_name: str, event: str
    ):
        """Handle context injection action."""
        content = result.context_injection
        if not content:
            return

        # 1. Validate size
        size_limit = self.injection_size_limit
        if size_limit is not None and len(content) > size_limit:
            logger.error(
                f"Hook injection too large: {hook_name}",
                extra={"size": len(content), "limit": size_limit},
            )
            raise ValueError(f"Context injection exceeds {size_limit} bytes")

        # 2. Check budget (policy from session config)
        budget = self.injection_budget_per_turn
        tokens = len(content) // 4  # Rough estimate

        # If budget is None, no limit (unlimited policy)
        if budget is not None and self._current_turn_injections + tokens > budget:
            logger.warning(
                "Warning: Hook injection budget exceeded",
                extra={
                    "hook": hook_name,
                    "current": self._current_turn_injections,
                    "attempted": tokens,
                    "budget": budget,
                },
            )

        self._current_turn_injections += tokens

        # 3. Add to context with provenance (ONLY if not ephemeral)
        if not result.ephemeral:
            context = self.mount_points["context"]
            if context and hasattr(context, "add_message"):
                message = {
                    "role": result.context_injection_role,
                    "content": content,
                    "metadata": {
                        "source": "hook",
                        "hook_name": hook_name,
                        "event": event,
                        "timestamp": datetime.now().isoformat(),
                    },
                }

                await context.add_message(message)

        # 4. Audit log
        logger.info(
            "Hook context injection",
            extra={
                "hook": hook_name,
                "event": event,
                "size": len(content),
                "role": result.context_injection_role,
                "tokens": tokens,
                "ephemeral": result.ephemeral,
            },
        )

    async def _handle_approval_request(
        self, result: HookResult, hook_name: str
    ) -> HookResult:
        """Handle approval request action."""
        prompt = result.approval_prompt or "Allow this operation?"
        options = result.approval_options or ["Allow", "Deny"]

        # Log request
        logger.info(
            "Approval requested",
            extra={
                "hook": hook_name,
                "prompt": prompt,
                "options": options,
                "timeout": result.approval_timeout,
                "default": result.approval_default,
            },
        )

        # Check if approval system is available
        if self.approval_system is None:
            logger.error(
                "Approval requested but no approval system provided",
                extra={"hook": hook_name},
            )
            return HookResult(action="deny", reason="No approval system available")

        try:
            # Request approval from user
            decision = await self.approval_system.request_approval(
                prompt=prompt,
                options=options,
                timeout=result.approval_timeout,
                default=result.approval_default,
            )

            # Log decision
            logger.info(
                "Approval decision", extra={"hook": hook_name, "decision": decision}
            )

            # Process decision
            if decision == "Deny":
                return HookResult(action="deny", reason=f"User denied: {prompt}")

            # "Allow once" or "Allow always" → proceed
            return HookResult(action="continue")

        except ApprovalTimeoutError:
            # Log timeout
            logger.warning(
                "Approval timeout",
                extra={"hook": hook_name, "default": result.approval_default},
            )

            # Apply default
            if result.approval_default == "deny":
                return HookResult(
                    action="deny",
                    reason=f"Approval timeout - denied by default: {prompt}",
                )
            return HookResult(action="continue")

    def _handle_user_message(self, result: HookResult, hook_name: str):
        """Handle user message display."""
        if not result.user_message:
            return

        # Use user_message_source if provided, otherwise fall back to hook_name
        source_name = result.user_message_source or hook_name

        # Check if display system is available
        if self.display_system is None:
            # Fallback to logging if no display system provided
            logger.info(
                f"Hook message ({result.user_message_level}): {result.user_message}",
                extra={"hook": source_name},
            )
            return

        self.display_system.show_message(
            message=result.user_message,
            level=result.user_message_level,
            source=f"hook:{source_name}",
        )
