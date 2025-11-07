"""
Approval system for hook-requested user approvals.
Provides pluggable interface with CLI implementation.
"""

import asyncio
import logging
from typing import Literal
from typing import Protocol

from rich.console import Console
from rich.prompt import Prompt

logger = logging.getLogger(__name__)


class ApprovalTimeoutError(Exception):
    """Raised when user approval times out."""

    pass


class ApprovalSystem(Protocol):
    """Pluggable approval interface for different environments."""

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
            ApprovalTimeout: User didn't respond within timeout
        """
        ...


class CLIApprovalSystem:
    """Terminal-based approval with Rich formatting and timeout."""

    def __init__(self):
        self.console = Console()
        self.cache: dict[str, str] = {}  # Session-scoped approval cache

    async def request_approval(
        self, prompt: str, options: list[str], timeout: float, default: Literal["allow", "deny"]
    ) -> str:
        """
        Show approval prompt in terminal with timeout.

        Args:
            prompt: Question to ask user
            options: Available choices
            timeout: Seconds to wait
            default: Action on timeout ("allow" or "deny")

        Returns:
            Selected option

        Raises:
            ApprovalTimeout: User didn't respond within timeout
        """
        # Check cache (for "Allow always" decisions)
        cache_key = f"{prompt}:{','.join(options)}"
        if cache_key in self.cache:
            cached_decision = self.cache[cache_key]
            self.console.print(f"[dim]Using cached approval decision: {cached_decision}[/dim]")
            return cached_decision

        # Display prompt
        self.console.print()
        self.console.print("[yellow]⚠️  Hook Approval Required[/yellow]")
        self.console.print(f"\n{prompt}")
        self.console.print(f"\nOptions: {', '.join(options)}")
        self.console.print(f"[dim]Timeout in {timeout}s, defaults to: {default}[/dim]")
        self.console.print()

        # Get user input with timeout
        try:
            async with asyncio.timeout(timeout):
                choice = await asyncio.to_thread(Prompt.ask, "Your choice", choices=options)

                # Cache "Allow always" decisions
                if choice == "Allow always":
                    self.cache[cache_key] = "Allow once"  # Cache as simplified "allow"
                    self.console.print("[green]✓ Approval cached for this session[/green]")

                return choice

        except TimeoutError:
            self.console.print(f"\n[yellow]⏱  Timeout ({timeout}s) - using default: {default}[/yellow]")
            raise ApprovalTimeoutError(f"User approval timeout after {timeout}s")
