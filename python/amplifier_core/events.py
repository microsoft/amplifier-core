"""Event name constants for the Amplifier kernel.

All constants are defined in the Rust kernel and re-exported here
for backward compatibility.

CP-V BREAKING CHANGE: The 10 tiered :debug and :raw event constants
(SESSION_START_DEBUG, SESSION_START_RAW, SESSION_FORK_DEBUG, etc.) have been
removed. Callers that subscribed to those event names will no longer receive
events. Use session.raw: true in config to get raw payloads on the base events.
"""

from amplifier_core._engine import (
    # Session lifecycle (base events only — no :debug or :raw tiers)
    SESSION_START,
    SESSION_END,
    SESSION_FORK,
    SESSION_RESUME,
    # Prompt lifecycle
    PROMPT_SUBMIT,
    PROMPT_COMPLETE,
    # Planning
    PLAN_START,
    PLAN_END,
    # Provider calls
    PROVIDER_REQUEST,
    PROVIDER_RESPONSE,
    PROVIDER_RETRY,
    PROVIDER_ERROR,
    PROVIDER_THROTTLE,
    PROVIDER_TOOL_SEQUENCE_REPAIRED,
    PROVIDER_RESOLVE,
    # LLM events (base events only — no :debug or :raw tiers)
    LLM_REQUEST,
    LLM_RESPONSE,
    # Content block events
    CONTENT_BLOCK_START,
    CONTENT_BLOCK_DELTA,
    CONTENT_BLOCK_END,
    # Thinking events
    THINKING_DELTA,
    THINKING_FINAL,
    # Tool invocations
    TOOL_PRE,
    TOOL_POST,
    TOOL_ERROR,
    # Context management
    CONTEXT_PRE_COMPACT,
    CONTEXT_POST_COMPACT,
    CONTEXT_COMPACTION,
    CONTEXT_INCLUDE,
    # Orchestrator lifecycle
    ORCHESTRATOR_COMPLETE,
    EXECUTION_START,
    EXECUTION_END,
    # User notifications
    USER_NOTIFICATION,
    # Artifacts
    ARTIFACT_WRITE,
    ARTIFACT_READ,
    # Policy / approvals
    POLICY_VIOLATION,
    APPROVAL_REQUIRED,
    APPROVAL_GRANTED,
    APPROVAL_DENIED,
    # Cancellation lifecycle
    CANCEL_REQUESTED,
    CANCEL_COMPLETED,
)

# Build ALL_EVENTS locally from the 41 remaining constants.
# Do NOT import ALL_EVENTS from _engine — the compiled extension may still list
# 51 entries if it hasn't been rebuilt since CP-V. This list is authoritative.
ALL_EVENTS: list[str] = [
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
    PROVIDER_RETRY,
    PROVIDER_ERROR,
    PROVIDER_THROTTLE,
    PROVIDER_TOOL_SEQUENCE_REPAIRED,
    PROVIDER_RESOLVE,
    LLM_REQUEST,
    LLM_RESPONSE,
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
    EXECUTION_START,
    EXECUTION_END,
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

__all__ = [
    "SESSION_START",
    "SESSION_END",
    "SESSION_FORK",
    "SESSION_RESUME",
    "PROMPT_SUBMIT",
    "PROMPT_COMPLETE",
    "PLAN_START",
    "PLAN_END",
    "PROVIDER_REQUEST",
    "PROVIDER_RESPONSE",
    "PROVIDER_RETRY",
    "PROVIDER_ERROR",
    "PROVIDER_THROTTLE",
    "PROVIDER_TOOL_SEQUENCE_REPAIRED",
    "PROVIDER_RESOLVE",
    "LLM_REQUEST",
    "LLM_RESPONSE",
    "CONTENT_BLOCK_START",
    "CONTENT_BLOCK_DELTA",
    "CONTENT_BLOCK_END",
    "THINKING_DELTA",
    "THINKING_FINAL",
    "TOOL_PRE",
    "TOOL_POST",
    "TOOL_ERROR",
    "CONTEXT_PRE_COMPACT",
    "CONTEXT_POST_COMPACT",
    "CONTEXT_COMPACTION",
    "CONTEXT_INCLUDE",
    "ORCHESTRATOR_COMPLETE",
    "EXECUTION_START",
    "EXECUTION_END",
    "USER_NOTIFICATION",
    "ARTIFACT_WRITE",
    "ARTIFACT_READ",
    "POLICY_VIOLATION",
    "APPROVAL_REQUIRED",
    "APPROVAL_GRANTED",
    "APPROVAL_DENIED",
    "CANCEL_REQUESTED",
    "CANCEL_COMPLETED",
    "ALL_EVENTS",
]
