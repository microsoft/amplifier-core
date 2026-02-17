"""
Thin Python wrappers around Rust PyO3 types.

These add Python-specific behaviors that don't belong in the Rust kernel:
- process_hook_result (calls approval_system, display_system)

The top-level `from amplifier_core import ModuleCoordinator` returns
this wrapper class. The submodule `from amplifier_core.coordinator import
ModuleCoordinator` still gives the pure-Python version.
"""

import logging
from datetime import datetime

from ._engine import RustCoordinator
from .approval import ApprovalTimeoutError
from .models import HookResult

logger = logging.getLogger(__name__)


class ModuleCoordinator(RustCoordinator):
    """Rust-backed coordinator with Python hook dispatch and process_hook_result.

    Extends RustCoordinator with:
    - A Python HookRegistry for hook dispatch (handles async handlers natively)
    - process_hook_result (calls approval_system, display_system)

    The Python HookRegistry is used instead of the Rust RustHookRegistry because
    all hook handlers in the current ecosystem are Python async functions. The Rust
    HookRegistry requires PyO3 async bridging (run_coroutine_threadsafe) which is
    fragile inside a running asyncio event loop. The Python HookRegistry uses
    native async/await and works reliably.
    """

    _py_hooks = None
    _current_turn_injections = 0

    @property
    def hooks(self):
        """Return the Python HookRegistry for this coordinator.
        
        Overrides the Rust hooks property to use Python's native async dispatch.
        The Python HookRegistry is created lazily on first access and stored
        in mount_points["hooks"] for ecosystem compatibility.
        """
        if self._py_hooks is None:
            from .hooks import HookRegistry as PyHookRegistry
            self._py_hooks = PyHookRegistry()
            # Copy default fields from the Rust hook registry if set
            # The Rust session constructor sets session_id and parent_id as defaults
            try:
                rust_hooks = super().hooks
                # Transfer any defaults that were set on the Rust registry
                # by reading the session_id from the coordinator
                self._py_hooks.set_default_fields(
                    session_id=self.session_id,
                    parent_id=self.parent_id,
                )
            except Exception:
                pass
            # Also store in mount_points for ecosystem access
            self.mount_points["hooks"] = self._py_hooks
        return self._py_hooks

    async def process_hook_result(
        self, result: HookResult, event: str, hook_name: str = "unknown"
    ) -> HookResult:
        """Process HookResult and route actions to appropriate subsystems.

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

            # "Allow once" or "Allow always" -> proceed
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
