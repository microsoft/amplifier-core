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

import inspect
import logging
from datetime import datetime
from typing import TYPE_CHECKING
from typing import Any

from .approval import ApprovalTimeoutError
from .approval import CLIApprovalSystem
from .display import CLIDisplaySystem
from .hooks import HookRegistry
from .models import HookResult

if TYPE_CHECKING:
    from .loader import ModuleLoader
    from .session import AmplifierSession

logger = logging.getLogger(__name__)

# Context injection size limit (kernel safety invariant)
# Budget per turn is configurable policy via session.injection_budget_per_turn
MAX_INJECTION_SIZE = 10 * 1024  # 10KB hard limit per injection


class ModuleCoordinator:
    """
    Central coordination and infrastructure context for all modules.

    Provides:
    - Mount points for module attachment
    - Infrastructure context (IDs, config, session reference)
    - Capability registry for inter-module communication
    - Event system with default field injection
    """

    def __init__(self: "ModuleCoordinator", session: "AmplifierSession"):
        """
        Initialize coordinator with session providing infrastructure context.

        Args:
            session: Parent AmplifierSession providing infrastructure
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

        # Make hooks accessible as an attribute for backward compatibility
        self.hooks = self.mount_points["hooks"]

        # Hook result processing subsystems
        self.approval_system = CLIApprovalSystem()
        self.display_system = CLIDisplaySystem()
        self._current_turn_injections = 0  # Token budget tracking

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
            Default: 10,000 tokens if not configured.
        """
        return self._session.config.get("session", {}).get("injection_budget_per_turn", 10_000)

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

    async def mount(self, mount_point: str, module: Any, name: str | None = None) -> None:
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
            logger.info(f"Mounted {module.__class__.__name__} '{name}' at {mount_point}")

        elif mount_point == "hooks":
            raise ValueError("Hooks should be registered directly with the HookRegistry")

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

        if mount_point in ["orchestrator", "context", "hooks", "module-source-resolver"]:
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

    async def cleanup(self):
        """Call all registered cleanup functions."""
        for cleanup_fn in reversed(self._cleanup_functions):
            try:
                if callable(cleanup_fn):
                    if inspect.iscoroutinefunction(cleanup_fn):
                        await cleanup_fn()
                    else:
                        result = cleanup_fn()
                        if inspect.iscoroutine(result):
                            await result
            except Exception as e:
                logger.error(f"Error during cleanup: {e}")

    def reset_turn(self):
        """Reset per-turn tracking. Call at turn boundaries."""
        self._current_turn_injections = 0

    async def process_hook_result(self, result: HookResult, event: str, hook_name: str = "unknown") -> HookResult:
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

    async def _handle_context_injection(self, result: HookResult, hook_name: str, event: str):
        """Handle context injection action."""
        content = result.context_injection
        if not content:
            return

        # 1. Validate size
        if len(content) > MAX_INJECTION_SIZE:
            logger.error(f"Hook injection too large: {hook_name}", extra={"size": len(content)})
            raise ValueError(f"Context injection exceeds {MAX_INJECTION_SIZE} bytes")

        # 2. Check budget (policy from session config)
        budget = self.injection_budget_per_turn
        tokens = len(content) // 4  # Rough estimate

        # If budget is None, no limit (unlimited policy)
        if budget is not None and self._current_turn_injections + tokens > budget:
            logger.warning(
                "Hook injection budget exceeded",
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

    async def _handle_approval_request(self, result: HookResult, hook_name: str) -> HookResult:
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

        try:
            # Request approval from user
            decision = await self.approval_system.request_approval(
                prompt=prompt, options=options, timeout=result.approval_timeout, default=result.approval_default
            )

            # Log decision
            logger.info("Approval decision", extra={"hook": hook_name, "decision": decision})

            # Process decision
            if decision == "Deny":
                return HookResult(action="deny", reason=f"User denied: {prompt}")

            # "Allow once" or "Allow always" → proceed
            return HookResult(action="continue")

        except ApprovalTimeoutError:
            # Log timeout
            logger.warning("Approval timeout", extra={"hook": hook_name, "default": result.approval_default})

            # Apply default
            if result.approval_default == "deny":
                return HookResult(action="deny", reason=f"Approval timeout - denied by default: {prompt}")
            return HookResult(action="continue")

    def _handle_user_message(self, result: HookResult, hook_name: str):
        """Handle user message display."""
        if not result.user_message:
            return

        self.display_system.show_message(
            message=result.user_message, level=result.user_message_level, source=f"hook:{hook_name}"
        )
