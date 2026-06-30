"""Serialization contract for ModelInfo.pricing and the Pricing model.

pricing is optional on ModelInfo: None when a provider has no rate data
(e.g., local providers like ollama, self-hosted backends like vllm), and a
populated Pricing object when a provider can supply rates. Both states must
round-trip cleanly through model_dump() / model_validate() so HTTP bridges
(e.g., amplifier-app-opencode) can rely on the field without special-casing
either branch.
"""

from amplifier_core.models import ModelInfo
from amplifier_core.models import Pricing


class TestPricingRoundTrip:
    """Pricing model dumps and reconstructs without loss."""

    def test_round_trip_full(self):
        pricing = Pricing(
            input_per_million=3.0,
            output_per_million=15.0,
            cache_read_per_million=0.3,
            cache_write_per_million=3.75,
            currency="USD",
        )
        dumped = pricing.model_dump()
        rebuilt = Pricing.model_validate(dumped)

        assert rebuilt == pricing
        assert rebuilt.input_per_million == 3.0
        assert rebuilt.output_per_million == 15.0
        assert rebuilt.cache_read_per_million == 0.3
        assert rebuilt.cache_write_per_million == 3.75
        assert rebuilt.currency == "USD"
        assert rebuilt.as_of is None

    def test_optional_fields_default_to_none(self):
        pricing = Pricing(input_per_million=1.0, output_per_million=5.0)

        assert pricing.cache_read_per_million is None
        assert pricing.cache_write_per_million is None
        assert pricing.currency == "USD"
        assert pricing.as_of is None


class TestModelInfoPricingField:
    """ModelInfo.pricing: optional, backwards-compatible, round-trips both states."""

    def test_pricing_none_serializes_cleanly(self):
        """Local/self-hosted providers (no rate data) omit pricing without error."""
        model = ModelInfo(
            id="local-model",
            display_name="Local Model",
            context_window=8192,
            max_output_tokens=4096,
        )

        dumped = model.model_dump()

        assert dumped["pricing"] is None
        rebuilt = ModelInfo.model_validate(dumped)
        assert rebuilt.pricing is None

    def test_pricing_populated_round_trips(self):
        pricing = Pricing(
            input_per_million=3.0,
            output_per_million=15.0,
            cache_read_per_million=0.3,
            cache_write_per_million=3.75,
        )
        model = ModelInfo(
            id="claude-sonnet-4-5",
            display_name="Claude Sonnet 4.5",
            context_window=200_000,
            max_output_tokens=64_000,
            pricing=pricing,
        )

        dumped = model.model_dump()
        rebuilt = ModelInfo.model_validate(dumped)

        assert rebuilt.pricing is not None
        assert rebuilt.pricing == pricing
        assert dumped["pricing"]["input_per_million"] == 3.0
        assert dumped["pricing"]["output_per_million"] == 15.0
