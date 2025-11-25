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
from .message_models import ChatRequest
from .message_models import ChatResponse
from .message_models import Degradation
from .message_models import ImageBlock
from .message_models import Message
from .message_models import ReasoningBlock
from .message_models import RedactedThinkingBlock
from .message_models import ResponseFormat
from .message_models import ResponseFormatJson
from .message_models import ResponseFormatJsonSchema
from .message_models import ResponseFormatText
from .message_models import TextBlock
from .message_models import ThinkingBlock
from .message_models import ToolCallBlock
from .message_models import ToolResultBlock
from .message_models import ToolSpec
from .message_models import Usage
from .models import HookResult
from .models import ModelInfo
from .models import ModuleInfo
from .models import ProviderInfo
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
    "ModelInfo",
    "ModuleInfo",
    "ProviderInfo",
    "SessionStatus",
    "ApprovalRequest",
    "ApprovalResponse",
    "Orchestrator",
    "Provider",
    "Tool",
    "ContextManager",
    "HookHandler",
    "ApprovalProvider",
    "ChatRequest",
    "ChatResponse",
    "Message",
    "TextBlock",
    "ThinkingBlock",
    "RedactedThinkingBlock",
    "ToolCallBlock",
    "ToolResultBlock",
    "ImageBlock",
    "ReasoningBlock",
    "ToolSpec",
    "Usage",
    "Degradation",
    "ResponseFormat",
    "ResponseFormatText",
    "ResponseFormatJson",
    "ResponseFormatJsonSchema",
]
