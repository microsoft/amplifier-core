"""
Amplifier Core - Ultra-thin coordination layer for modular AI agents.
"""

__version__ = "1.0.0"

from .cancellation import CancellationState
from .cancellation import CancellationToken
from .content_models import ContentBlock
from .content_models import ContentBlockType
from .content_models import TextContent
from .content_models import ThinkingContent
from .content_models import ToolCallContent
from .content_models import ToolResultContent
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
from .llm_errors import AuthenticationError
from .llm_errors import ContentFilterError
from .llm_errors import ContextLengthError
from .llm_errors import InvalidRequestError
from .llm_errors import LLMError
from .llm_errors import LLMTimeoutError
from .llm_errors import ProviderUnavailableError
from .llm_errors import RateLimitError
from .loader import ModuleLoader
from .loader import ModuleValidationError
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
from .message_models import ToolCall
from .message_models import ToolCallBlock
from .message_models import ToolResultBlock
from .message_models import ToolSpec
from .message_models import Usage
from .models import ConfigField
from .models import HookResult
from .models import ModelInfo
from .models import ModuleInfo
from .models import ProviderInfo
from .models import SessionStatus
from .models import ToolResult
from .session import AmplifierSession
from .testing import EventRecorder
from .testing import MockContextManager
from .testing import MockTool
from .testing import ScriptedOrchestrator
from .testing import TestCoordinator
from .testing import create_test_coordinator
from .testing import wait_for

__all__ = [
    "AmplifierSession",
    # Cancellation primitives
    "CancellationState",
    "CancellationToken",
    "ModuleCoordinator",
    "ModuleLoader",
    "ModuleValidationError",
    "HookRegistry",
    "ToolCall",
    "ToolResult",
    "HookResult",
    "ConfigField",
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
    # LLM error taxonomy
    "LLMError",
    "RateLimitError",
    "AuthenticationError",
    "ContextLengthError",
    "ContentFilterError",
    "InvalidRequestError",
    "ProviderUnavailableError",
    "LLMTimeoutError",
    # Content models for provider streaming
    "ContentBlock",
    "ContentBlockType",
    "TextContent",
    "ThinkingContent",
    "ToolCallContent",
    "ToolResultContent",
    # Testing utilities
    "TestCoordinator",
    "MockTool",
    "MockContextManager",
    "EventRecorder",
    "ScriptedOrchestrator",
    "create_test_coordinator",
    "wait_for",
]
