"""
Canonical event names for Amplifier (desired-state, 2025-10-11).
Stable surface for hooks and observability.
"""

# Session lifecycle
SESSION_START = "session:start"
SESSION_END = "session:end"

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

# Artifacts (files, diffs, external blobs)
ARTIFACT_WRITE = "artifact:write"
ARTIFACT_READ = "artifact:read"

# Policy / approvals
POLICY_VIOLATION = "policy:violation"
APPROVAL_REQUIRED = "approval:required"
APPROVAL_GRANTED = "approval:granted"
APPROVAL_DENIED = "approval:denied"
