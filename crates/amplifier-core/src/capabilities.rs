//! Model capability constants.
//!
//! This module defines well-known capability strings that describe what a model
//! can do (e.g. tool use, streaming, vision).

// ---------------------------------------------------------------------------
// Capability constants — Tier 1 (core)
// ---------------------------------------------------------------------------

/// Model supports tool/function calling.
pub const TOOLS: &str = "tools";
/// Model supports streaming responses.
pub const STREAMING: &str = "streaming";
/// Model supports extended thinking / chain-of-thought.
pub const THINKING: &str = "thinking";
/// Model can process image inputs.
pub const VISION: &str = "vision";
/// Model can produce structured JSON output.
pub const JSON_MODE: &str = "json_mode";

// ---------------------------------------------------------------------------
// Capability constants — Tier 2 (extended)
// ---------------------------------------------------------------------------

/// Model is optimised for low-latency responses.
pub const FAST: &str = "fast";
/// Model can execute code in a sandbox.
pub const CODE_EXECUTION: &str = "code_execution";
/// Model can search the web.
pub const WEB_SEARCH: &str = "web_search";
/// Model can perform deep, multi-step research.
pub const DEEP_RESEARCH: &str = "deep_research";
/// Model runs locally (on-device).
pub const LOCAL: &str = "local";
/// Model can process audio inputs.
pub const AUDIO: &str = "audio";
/// Model can generate images.
pub const IMAGE_GENERATION: &str = "image_generation";
/// Model can operate a computer (mouse, keyboard, screen).
pub const COMPUTER_USE: &str = "computer_use";
/// Model produces embedding vectors.
pub const EMBEDDINGS: &str = "embeddings";
/// Model supports an unusually large context window.
pub const LONG_CONTEXT: &str = "long_context";
/// Model supports batch / offline processing.
pub const BATCH: &str = "batch";

// ---------------------------------------------------------------------------
// All well-known capabilities
// ---------------------------------------------------------------------------

/// Every well-known capability string, in declaration order.
pub const ALL_WELL_KNOWN_CAPABILITIES: &[&str] = &[
    TOOLS,
    STREAMING,
    THINKING,
    VISION,
    JSON_MODE,
    FAST,
    CODE_EXECUTION,
    WEB_SEARCH,
    DEEP_RESEARCH,
    LOCAL,
    AUDIO,
    IMAGE_GENERATION,
    COMPUTER_USE,
    EMBEDDINGS,
    LONG_CONTEXT,
    BATCH,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_constants_are_strings() {
        let capabilities: &[&str] = &[
            TOOLS,
            STREAMING,
            THINKING,
            VISION,
            JSON_MODE,
            FAST,
            CODE_EXECUTION,
            WEB_SEARCH,
            DEEP_RESEARCH,
            LOCAL,
            AUDIO,
            IMAGE_GENERATION,
            COMPUTER_USE,
            EMBEDDINGS,
            LONG_CONTEXT,
            BATCH,
        ];
        for cap in capabilities {
            assert!(!cap.is_empty(), "Capability constant must be non-empty");
        }
    }

    #[test]
    fn test_all_well_known_capabilities_count() {
        assert_eq!(
            ALL_WELL_KNOWN_CAPABILITIES.len(),
            16,
            "Expected exactly 16 well-known capabilities"
        );
    }

    #[test]
    fn test_all_well_known_capabilities_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for cap in ALL_WELL_KNOWN_CAPABILITIES {
            assert!(seen.insert(*cap), "Duplicate capability found: {cap}");
        }
    }
}
