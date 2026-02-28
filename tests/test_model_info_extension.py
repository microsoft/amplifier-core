"""Tests for ModelInfo extension with cost and metadata fields."""

from typing import Any

from amplifier_core.models import ModelInfo


def _make_model_info(**kwargs: Any) -> ModelInfo:
    """Helper to create a ModelInfo with required fields."""
    base_kwargs = {
        "id": "test-model",
        "display_name": "Test Model",
        "context_window": 128000,
        "max_output_tokens": 4096,
    }
    base_kwargs.update(kwargs)
    return ModelInfo(**base_kwargs)


def test_model_info_new_fields_have_defaults() -> None:
    """Existing construction works; new fields are None/empty dict."""
    info = _make_model_info()
    assert info.cost_per_input_token is None
    assert info.cost_per_output_token is None
    assert info.metadata == {}


def test_model_info_with_cost_data() -> None:
    """Cost fields can be populated."""
    info = _make_model_info(
        cost_per_input_token=3e-6,
        cost_per_output_token=15e-6,
    )
    assert info.cost_per_input_token == 3e-6
    assert info.cost_per_output_token == 15e-6


def test_model_info_with_metadata() -> None:
    """Metadata dict accepts cost_tier and model_class."""
    info = _make_model_info(
        metadata={"cost_tier": "premium", "model_class": "frontier"},
    )
    assert info.metadata["cost_tier"] == "premium"
    assert info.metadata["model_class"] == "frontier"


def test_model_info_serialization_round_trip() -> None:
    """model_dump + reconstruct preserves new fields."""
    original = _make_model_info(
        cost_per_input_token=3e-6,
        cost_per_output_token=15e-6,
        metadata={"cost_tier": "premium", "model_class": "frontier"},
    )
    data = original.model_dump()
    restored = ModelInfo(**data)
    assert restored.cost_per_input_token == original.cost_per_input_token
    assert restored.cost_per_output_token == original.cost_per_output_token
    assert restored.metadata == original.metadata
    assert restored == original
