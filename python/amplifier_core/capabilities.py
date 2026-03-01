"""Well-known model capabilities for Amplifier.

All constants are defined in the Rust kernel and re-exported here
for backward compatibility with ``from amplifier_core.capabilities import TOOLS``.
"""

from amplifier_core._engine import (
    # Well-known capabilities
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
    ALL_WELL_KNOWN_CAPABILITIES,
)

__all__ = [
    "TOOLS",
    "STREAMING",
    "THINKING",
    "VISION",
    "JSON_MODE",
    "FAST",
    "CODE_EXECUTION",
    "WEB_SEARCH",
    "DEEP_RESEARCH",
    "LOCAL",
    "AUDIO",
    "IMAGE_GENERATION",
    "COMPUTER_USE",
    "EMBEDDINGS",
    "LONG_CONTEXT",
    "BATCH",
    "ALL_WELL_KNOWN_CAPABILITIES",
]
