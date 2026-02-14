"""
Approval system protocol for kernel.

Kernel provides mechanism (Protocol interface).
App layer provides policy (CLI, web, API implementations).
"""

from typing import Literal
from typing import Protocol


class ApprovalTimeoutError(Exception):
    """Raised when user approval times out."""

    pass


class ApprovalSystem(Protocol):
    """
    Pluggable approval interface for different environments.

    Implementations provided by app layer:
    - CLI: Terminal-based with rich formatting
    - Web: WebSocket-based with browser UI
    - API: HTTP callback or stored decision
    """

    async def request_approval(
        self, prompt: str, options: list[str], timeout: float, default: Literal["allow", "deny"]
    ) -> str:
        """
        Request user approval with timeout.

        Args:
            prompt: Question to ask user
            options: Available choices
            timeout: Seconds to wait for response
            default: Action to take on timeout

        Returns:
            Selected option string (one of options)

        Raises:
            ApprovalTimeoutError: User didn't respond within timeout
        """
        ...
