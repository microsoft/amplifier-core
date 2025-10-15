"""
Amplifier Core - Ultra-thin coordination layer for modular AI agents.
"""

__version__ = "1.0.0"

from .coordinator import ModuleCoordinator
from .hooks import HookRegistry
from .interfaces import ApprovalProvider
from .interfaces import ApprovalRequest
from .interfaces import ApprovalResponse
from .interfaces import ContextManager
from .interfaces import HookHandler
from .interfaces import Orchestrator
from .interfaces import Provider
from .interfaces import Tool
from .loader import ModuleLoader
from .models import HookResult
from .models import ModuleInfo
from .models import ProviderResponse
from .models import SessionStatus
from .models import ToolCall
from .models import ToolResult
from .session import AmplifierSession

__all__ = [
    "AmplifierSession",
    "ModuleCoordinator",
    "ModuleLoader",
    "HookRegistry",
    "ToolCall",
    "ToolResult",
    "HookResult",
    "ProviderResponse",
    "ModuleInfo",
    "SessionStatus",
    "ApprovalRequest",
    "ApprovalResponse",
    "Orchestrator",
    "Provider",
    "Tool",
    "ContextManager",
    "HookHandler",
    "ApprovalProvider",
]
