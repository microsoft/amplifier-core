"""
Canonical event names for Amplifier (desired-state, 2025-10-11).
Stable surface for hooks and observability.
"""

# Session lifecycle
SESSION_START = "session:start"
SESSION_END = "session:end"
SESSION_FORK = "session:fork"

# Prompt lifecycle
PROMPT_SUBMIT = "prompt:submit"
PROMPT_COMPLETE = "prompt:complete"

# Planning (optional orchestration phases)
PLAN_START = "plan:start"
PLAN_END = "plan:end"

# Provider calls (LLMs)
PROVIDER_REQUEST = "provider:request"
PROVIDER_RESPONSE = "provider:response"
PROVIDER_ERROR = "provider:error"

# Content Block Events (for real-time display)
CONTENT_BLOCK_START = "content_block:start"
CONTENT_BLOCK_DELTA = "content_block:delta"
CONTENT_BLOCK_END = "content_block:end"

# Tool invocations
TOOL_PRE = "tool:pre"
TOOL_POST = "tool:post"
TOOL_ERROR = "tool:error"

# Context management
CONTEXT_PRE_COMPACT = "context:pre_compact"
CONTEXT_POST_COMPACT = "context:post_compact"
CONTEXT_COMPACTION = "context:compaction"

# Orchestrator lifecycle
ORCHESTRATOR_COMPLETE = "orchestrator:complete"

# User notifications
USER_NOTIFICATION = "user:notification"

# Artifacts (files, diffs, external blobs)
ARTIFACT_WRITE = "artifact:write"
ARTIFACT_READ = "artifact:read"

# Policy / approvals
POLICY_VIOLATION = "policy:violation"
APPROVAL_REQUIRED = "approval:required"
APPROVAL_GRANTED = "approval:granted"
APPROVAL_DENIED = "approval:denied"

# Cancellation lifecycle
CANCEL_REQUESTED = "cancel:requested"  # Cancellation initiated (graceful or immediate)
CANCEL_COMPLETED = "cancel:completed"  # Cancellation finalized, session stopping

SESSION_RESUME = "session:resume"
LLM_REQUEST = "llm:request"
LLM_REQUEST_DEBUG = "llm:request:debug"
LLM_REQUEST_RAW = "llm:request:raw"
LLM_RESPONSE = "llm:response"
LLM_RESPONSE_DEBUG = "llm:response:debug"
LLM_RESPONSE_RAW = "llm:response:raw"
THINKING_DELTA = "thinking:delta"
THINKING_FINAL = "thinking:final"
CONTEXT_INCLUDE = "context:include"

# All canonical events (for iteration and validation)
ALL_EVENTS = [
    SESSION_START,
    SESSION_END,
    SESSION_FORK,
    SESSION_RESUME,
    PROMPT_SUBMIT,
    PROMPT_COMPLETE,
    PLAN_START,
    PLAN_END,
    PROVIDER_REQUEST,
    PROVIDER_RESPONSE,
    PROVIDER_ERROR,
    LLM_REQUEST,
    LLM_REQUEST_DEBUG,
    LLM_REQUEST_RAW,
    LLM_RESPONSE,
    LLM_RESPONSE_DEBUG,
    LLM_RESPONSE_RAW,
    CONTENT_BLOCK_START,
    CONTENT_BLOCK_DELTA,
    CONTENT_BLOCK_END,
    THINKING_DELTA,
    THINKING_FINAL,
    TOOL_PRE,
    TOOL_POST,
    TOOL_ERROR,
    CONTEXT_PRE_COMPACT,
    CONTEXT_POST_COMPACT,
    CONTEXT_COMPACTION,
    CONTEXT_INCLUDE,
    ORCHESTRATOR_COMPLETE,
    USER_NOTIFICATION,
    ARTIFACT_WRITE,
    ARTIFACT_READ,
    POLICY_VIOLATION,
    APPROVAL_REQUIRED,
    APPROVAL_GRANTED,
    APPROVAL_DENIED,
    CANCEL_REQUESTED,
    CANCEL_COMPLETED,
]
