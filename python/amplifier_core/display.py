"""
Display system protocol for kernel.

Kernel provides mechanism (Protocol interface).
App layer provides policy (CLI, web, API implementations).
"""

from typing import Literal
from typing import Protocol


class DisplaySystem(Protocol):
    """
    Pluggable display interface for different environments.

    Implementations provided by app layer:
    - CLI: Terminal output with rich formatting
    - Web: WebSocket messages to browser
    - API: Logging or structured response
    """

    def show_message(self, message: str, level: Literal["info", "warning", "error"], source: str = "hook"):
        """
        Display message to user.

        Args:
            message: Message text
            level: Severity level
            source: Message source (for context)
        """
        ...
