"""
Amplifier Core - Ultra-thin coordination layer for modular AI agents.

Switchover: Top-level imports now return Rust-backed types from the _engine
extension module. Submodule paths (e.g. `from amplifier_core.session import
AmplifierSession`) still give the pure-Python implementations.
"""

__version__ = "1.0.0"

# --- Rust-backed primary types (THE SWITCHOVER) ---
# These four were previously imported from their Python submodules.
# Now they come from the Rust engine / thin Python wrappers.
from ._engine import RustCancellationToken as CancellationToken
from ._engine import RustHookRegistry as HookRegistry
from ._engine import RustSession as AmplifierSession
from ._rust_wrappers import ModuleCoordinator  # RustCoordinator + process_hook_result

# --- Pure-Python types that have no Rust equivalent yet ---
from .cancellation import CancellationState
from .content_models import ContentBlock
from .content_models import ContentBlockType
from .content_models import TextContent
from .content_models import ThinkingContent
from .content_models import ToolCallContent
from .content_models import ToolResultContent
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
from .llm_errors import AccessDeniedError
from .llm_errors import NetworkError
from .llm_errors import QuotaExceededError
from .llm_errors import NotFoundError
from .llm_errors import StreamError
from .llm_errors import AbortError
from .llm_errors import InvalidToolCallError
from .llm_errors import ConfigurationError
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

# --- Testing utilities (must come after Rust type imports) ---
from .testing import EventRecorder
from .testing import MockContextManager
from .testing import MockTool
from .testing import ScriptedOrchestrator
from .testing import TestCoordinator
from .testing import create_test_coordinator
from .testing import wait_for
from .utils.retry import classify_error_message
from .utils.retry import RetryConfig
from .utils.retry import retry_with_backoff

# --- Rust engine types re-exported under original names for direct access ---
from ._engine import (
    RUST_AVAILABLE,
    RustCancellationToken,
    RustCoordinator,
    RustHookRegistry,
    RustSession,
)

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
    "AccessDeniedError",
    "NetworkError",
    "QuotaExceededError",
    "NotFoundError",
    "StreamError",
    "AbortError",
    "InvalidToolCallError",
    "ConfigurationError",
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
    # Retry utilities
    "RetryConfig",
    "retry_with_backoff",
    "classify_error_message",
    # Rust engine types
    "RUST_AVAILABLE",
    "RustSession",
    "RustHookRegistry",
    "RustCancellationToken",
    "RustCoordinator",
]
