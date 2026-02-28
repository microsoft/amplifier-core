"""
Well-known model capabilities and cost tiers for Amplifier.
Stable surface for model selection and routing.
"""

# Tier 1: Core capabilities
TOOLS = "tools"
STREAMING = "streaming"
THINKING = "thinking"
VISION = "vision"
JSON_MODE = "json_mode"

# Tier 2: Extended capabilities
FAST = "fast"
CODE_EXECUTION = "code_execution"
WEB_SEARCH = "web_search"
DEEP_RESEARCH = "deep_research"
LOCAL = "local"
AUDIO = "audio"
IMAGE_GENERATION = "image_generation"
COMPUTER_USE = "computer_use"
EMBEDDINGS = "embeddings"
LONG_CONTEXT = "long_context"
BATCH = "batch"

# All well-known capabilities (frozenset for O(1) membership checks; duplicates structurally impossible)
ALL_WELL_KNOWN_CAPABILITIES: frozenset[str] = frozenset(
    {
        # Tier 1
        TOOLS,
        STREAMING,
        THINKING,
        VISION,
        JSON_MODE,
        # Tier 2
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
    }
)

# Cost tiers
COST_TIER_FREE = "free"
COST_TIER_LOW = "low"
COST_TIER_MEDIUM = "medium"
COST_TIER_HIGH = "high"
COST_TIER_EXTREME = "extreme"

# All cost tiers (frozenset for O(1) membership checks; duplicates structurally impossible)
ALL_COST_TIERS: frozenset[str] = frozenset(
    {
        COST_TIER_FREE,
        COST_TIER_LOW,
        COST_TIER_MEDIUM,
        COST_TIER_HIGH,
        COST_TIER_EXTREME,
    }
)
