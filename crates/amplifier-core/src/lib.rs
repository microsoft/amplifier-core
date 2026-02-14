//! amplifier-core: Pure Rust kernel for modular AI agent orchestration.
//!
//! This crate contains the core coordination engine for the Amplifier
//! ecosystem. It has ZERO Python dependency — it can be consumed from
//! any language via bindings.
//!
//! # Crate Organization
//!
//! - `events` — Canonical event name constants
//! - `errors` — All error types (AmplifierError, ProviderError, etc.)
//! - `models` — Core data models (HookResult, ToolResult, ModelInfo, etc.)
//! - `messages` — Chat protocol models (ChatRequest, ChatResponse, Message, etc.)
//! - `traits` — Module contracts (Tool, Provider, Orchestrator, etc.)
//! - `cancellation` — CancellationToken state machine
//! - `hooks` — HookRegistry event dispatch pipeline
//! - `coordinator` — ModuleCoordinator mount points and capabilities
//! - `session` — AmplifierSession lifecycle management

pub mod events;
pub mod errors;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        assert!(true);
    }
}
