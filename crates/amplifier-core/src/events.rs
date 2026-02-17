//! Canonical event name constants for the Amplifier event system.
//!
//! Every hook, log entry, and observability span in Amplifier references events
//! by these string constants. The taxonomy follows a `namespace:action` pattern
//! (e.g. `"session:start"`, `"tool:pre"`), with optional `:debug` / `:raw`
//! suffixes for verbosity tiers.
//!
//! # Categories
//!
//! | Category        | Prefix            | Description                                  |
//! |-----------------|-------------------|----------------------------------------------|
//! | Session         | `session:`        | Session lifecycle (start, end, fork, resume)  |
//! | Prompt          | `prompt:`         | Prompt submission and completion              |
//! | Planning        | `plan:`           | Optional orchestration planning phases        |
//! | Provider        | `provider:`       | High-level provider call events               |
//! | LLM             | `llm:`            | Raw LLM request/response with debug tiers     |
//! | Content Block   | `content_block:`  | Real-time streaming display events            |
//! | Thinking        | `thinking:`       | Model thinking/reasoning events               |
//! | Tool            | `tool:`           | Tool invocation lifecycle                     |
//! | Context         | `context:`        | Context management and compaction              |
//! | Orchestrator    | `orchestrator:`   | Orchestrator completion                       |
//! | Execution       | `execution:`      | Orchestrator execution boundaries             |
//! | User            | `user:`           | User-facing notifications                     |
//! | Artifact        | `artifact:`       | File/diff/blob operations                     |
//! | Policy          | `policy:`         | Policy violation events                       |
//! | Approval        | `approval:`       | Human-in-the-loop approval gates              |
//! | Cancellation    | `cancel:`         | Graceful/immediate cancellation lifecycle     |

// --- Session lifecycle ---

/// A new session has started.
pub const SESSION_START: &str = "session:start";
/// Session start with debug-level detail.
pub const SESSION_START_DEBUG: &str = "session:start:debug";
/// Session start with raw (full) detail.
pub const SESSION_START_RAW: &str = "session:start:raw";
/// A session has ended.
pub const SESSION_END: &str = "session:end";
/// A session has been forked.
pub const SESSION_FORK: &str = "session:fork";
/// Session fork with debug-level detail.
pub const SESSION_FORK_DEBUG: &str = "session:fork:debug";
/// Session fork with raw (full) detail.
pub const SESSION_FORK_RAW: &str = "session:fork:raw";
/// A session has been resumed.
pub const SESSION_RESUME: &str = "session:resume";
/// Session resume with debug-level detail.
pub const SESSION_RESUME_DEBUG: &str = "session:resume:debug";
/// Session resume with raw (full) detail.
pub const SESSION_RESUME_RAW: &str = "session:resume:raw";

// --- Prompt lifecycle ---

/// A prompt has been submitted for processing.
pub const PROMPT_SUBMIT: &str = "prompt:submit";
/// Prompt processing is complete.
pub const PROMPT_COMPLETE: &str = "prompt:complete";

// --- Planning (optional orchestration phases) ---

/// An orchestration planning phase has started.
pub const PLAN_START: &str = "plan:start";
/// An orchestration planning phase has ended.
pub const PLAN_END: &str = "plan:end";

// --- Provider calls (high-level LLM events) ---

/// A request has been sent to a provider.
pub const PROVIDER_REQUEST: &str = "provider:request";
/// A response has been received from a provider.
pub const PROVIDER_RESPONSE: &str = "provider:response";
pub const PROVIDER_RETRY: &str = "provider:retry";
/// A provider call resulted in an error.
pub const PROVIDER_ERROR: &str = "provider:error";

// --- LLM request/response (with debug tiers) ---

/// An LLM request has been issued.
pub const LLM_REQUEST: &str = "llm:request";
/// LLM request with debug-level detail.
pub const LLM_REQUEST_DEBUG: &str = "llm:request:debug";
/// LLM request with raw (full) detail.
pub const LLM_REQUEST_RAW: &str = "llm:request:raw";
/// An LLM response has been received.
pub const LLM_RESPONSE: &str = "llm:response";
/// LLM response with debug-level detail.
pub const LLM_RESPONSE_DEBUG: &str = "llm:response:debug";
/// LLM response with raw (full) detail.
pub const LLM_RESPONSE_RAW: &str = "llm:response:raw";

// --- Content block events (real-time streaming display) ---

/// A content block has started streaming.
pub const CONTENT_BLOCK_START: &str = "content_block:start";
/// A delta chunk within a content block.
pub const CONTENT_BLOCK_DELTA: &str = "content_block:delta";
/// A content block has finished streaming.
pub const CONTENT_BLOCK_END: &str = "content_block:end";

// --- Thinking events (model reasoning) ---

/// A delta chunk of model thinking/reasoning.
pub const THINKING_DELTA: &str = "thinking:delta";
/// Final model thinking/reasoning output.
pub const THINKING_FINAL: &str = "thinking:final";

// --- Tool invocations ---

/// A tool is about to be invoked (pre-hook).
pub const TOOL_PRE: &str = "tool:pre";
/// A tool has completed (post-hook).
pub const TOOL_POST: &str = "tool:post";
/// A tool invocation resulted in an error.
pub const TOOL_ERROR: &str = "tool:error";

// --- Context management ---

/// Context is about to be compacted (pre-hook).
pub const CONTEXT_PRE_COMPACT: &str = "context:pre_compact";
/// Context has been compacted (post-hook).
pub const CONTEXT_POST_COMPACT: &str = "context:post_compact";
/// A context compaction event.
pub const CONTEXT_COMPACTION: &str = "context:compaction";
/// Context has been included/added.
pub const CONTEXT_INCLUDE: &str = "context:include";

// --- Orchestrator lifecycle ---

/// The orchestrator has completed its run.
pub const ORCHESTRATOR_COMPLETE: &str = "orchestrator:complete";
/// Orchestrator execution begins.
pub const EXECUTION_START: &str = "execution:start";
/// Orchestrator execution completes.
pub const EXECUTION_END: &str = "execution:end";

// --- User notifications ---

/// A notification intended for the user.
pub const USER_NOTIFICATION: &str = "user:notification";

// --- Artifacts (files, diffs, external blobs) ---

/// An artifact has been written.
pub const ARTIFACT_WRITE: &str = "artifact:write";
/// An artifact has been read.
pub const ARTIFACT_READ: &str = "artifact:read";

// --- Policy / approvals ---

/// A policy violation was detected.
pub const POLICY_VIOLATION: &str = "policy:violation";
/// An approval gate has been triggered.
pub const APPROVAL_REQUIRED: &str = "approval:required";
/// An approval has been granted.
pub const APPROVAL_GRANTED: &str = "approval:granted";
/// An approval has been denied.
pub const APPROVAL_DENIED: &str = "approval:denied";

// --- Cancellation lifecycle ---

/// Cancellation has been requested (graceful or immediate).
pub const CANCEL_REQUESTED: &str = "cancel:requested";
/// Cancellation has been finalized, session stopping.
pub const CANCEL_COMPLETED: &str = "cancel:completed";

// --- Aggregate ---

/// All canonical event names, for iteration and validation.
///
/// This slice contains every event constant defined in this module,
/// matching the order used in the Python `ALL_EVENTS` list.
pub const ALL_EVENTS: &[&str] = &[
    SESSION_START,
    SESSION_START_DEBUG,
    SESSION_START_RAW,
    SESSION_END,
    SESSION_FORK,
    SESSION_FORK_DEBUG,
    SESSION_FORK_RAW,
    SESSION_RESUME,
    SESSION_RESUME_DEBUG,
    SESSION_RESUME_RAW,
    PROMPT_SUBMIT,
    PROMPT_COMPLETE,
    PLAN_START,
    PLAN_END,
    PROVIDER_REQUEST,
    PROVIDER_RESPONSE,
    PROVIDER_RETRY,
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
];

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Verify exact string values for every constant ----

    #[test]
    fn session_constants() {
        assert_eq!(SESSION_START, "session:start");
        assert_eq!(SESSION_START_DEBUG, "session:start:debug");
        assert_eq!(SESSION_START_RAW, "session:start:raw");
        assert_eq!(SESSION_END, "session:end");
        assert_eq!(SESSION_FORK, "session:fork");
        assert_eq!(SESSION_FORK_DEBUG, "session:fork:debug");
        assert_eq!(SESSION_FORK_RAW, "session:fork:raw");
        assert_eq!(SESSION_RESUME, "session:resume");
        assert_eq!(SESSION_RESUME_DEBUG, "session:resume:debug");
        assert_eq!(SESSION_RESUME_RAW, "session:resume:raw");
    }

    #[test]
    fn prompt_constants() {
        assert_eq!(PROMPT_SUBMIT, "prompt:submit");
        assert_eq!(PROMPT_COMPLETE, "prompt:complete");
    }

    #[test]
    fn plan_constants() {
        assert_eq!(PLAN_START, "plan:start");
        assert_eq!(PLAN_END, "plan:end");
    }

    #[test]
    fn provider_constants() {
        assert_eq!(PROVIDER_REQUEST, "provider:request");
        assert_eq!(PROVIDER_RESPONSE, "provider:response");
        assert_eq!(PROVIDER_RETRY, "provider:retry");
        assert_eq!(PROVIDER_ERROR, "provider:error");
    }

    #[test]
    fn llm_constants() {
        assert_eq!(LLM_REQUEST, "llm:request");
        assert_eq!(LLM_REQUEST_DEBUG, "llm:request:debug");
        assert_eq!(LLM_REQUEST_RAW, "llm:request:raw");
        assert_eq!(LLM_RESPONSE, "llm:response");
        assert_eq!(LLM_RESPONSE_DEBUG, "llm:response:debug");
        assert_eq!(LLM_RESPONSE_RAW, "llm:response:raw");
    }

    #[test]
    fn content_block_constants() {
        assert_eq!(CONTENT_BLOCK_START, "content_block:start");
        assert_eq!(CONTENT_BLOCK_DELTA, "content_block:delta");
        assert_eq!(CONTENT_BLOCK_END, "content_block:end");
    }

    #[test]
    fn thinking_constants() {
        assert_eq!(THINKING_DELTA, "thinking:delta");
        assert_eq!(THINKING_FINAL, "thinking:final");
    }

    #[test]
    fn tool_constants() {
        assert_eq!(TOOL_PRE, "tool:pre");
        assert_eq!(TOOL_POST, "tool:post");
        assert_eq!(TOOL_ERROR, "tool:error");
    }

    #[test]
    fn context_constants() {
        assert_eq!(CONTEXT_PRE_COMPACT, "context:pre_compact");
        assert_eq!(CONTEXT_POST_COMPACT, "context:post_compact");
        assert_eq!(CONTEXT_COMPACTION, "context:compaction");
        assert_eq!(CONTEXT_INCLUDE, "context:include");
    }

    #[test]
    fn orchestrator_and_execution_constants() {
        assert_eq!(ORCHESTRATOR_COMPLETE, "orchestrator:complete");
        assert_eq!(EXECUTION_START, "execution:start");
        assert_eq!(EXECUTION_END, "execution:end");
    }

    #[test]
    fn user_notification_constant() {
        assert_eq!(USER_NOTIFICATION, "user:notification");
    }

    #[test]
    fn artifact_constants() {
        assert_eq!(ARTIFACT_WRITE, "artifact:write");
        assert_eq!(ARTIFACT_READ, "artifact:read");
    }

    #[test]
    fn policy_and_approval_constants() {
        assert_eq!(POLICY_VIOLATION, "policy:violation");
        assert_eq!(APPROVAL_REQUIRED, "approval:required");
        assert_eq!(APPROVAL_GRANTED, "approval:granted");
        assert_eq!(APPROVAL_DENIED, "approval:denied");
    }

    #[test]
    fn cancellation_constants() {
        assert_eq!(CANCEL_REQUESTED, "cancel:requested");
        assert_eq!(CANCEL_COMPLETED, "cancel:completed");
    }

    // ---- ALL_EVENTS aggregate tests ----

    #[test]
    fn all_events_count() {
        assert_eq!(ALL_EVENTS.len(), 48, "Python source defines exactly 48 events");
    }

    #[test]
    fn all_events_contains_every_constant() {
        let expected: &[&str] = &[
            SESSION_START,
            SESSION_START_DEBUG,
            SESSION_START_RAW,
            SESSION_END,
            SESSION_FORK,
            SESSION_FORK_DEBUG,
            SESSION_FORK_RAW,
            SESSION_RESUME,
            SESSION_RESUME_DEBUG,
            SESSION_RESUME_RAW,
            PROMPT_SUBMIT,
            PROMPT_COMPLETE,
            PLAN_START,
            PLAN_END,
            PROVIDER_REQUEST,
            PROVIDER_RESPONSE,
            PROVIDER_RETRY,
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
        ];
        for event in expected {
            assert!(
                ALL_EVENTS.contains(event),
                "ALL_EVENTS missing: {event}"
            );
        }
    }

    #[test]
    fn all_events_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for event in ALL_EVENTS {
            assert!(seen.insert(event), "Duplicate in ALL_EVENTS: {event}");
        }
    }

    #[test]
    fn all_event_values_follow_namespace_pattern() {
        for event in ALL_EVENTS {
            assert!(
                event.contains(':'),
                "Event {event} does not follow namespace:action pattern"
            );
        }
    }
}
