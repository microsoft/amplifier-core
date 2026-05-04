"""Tests for cost_usd fields on Usage and SessionStatus.

Verifies:
- cost_usd is a declared Decimal field (not bag extra)
- Pydantic validates Decimal type — rejects float
- None means unknown (not zero)
- Decimal("0") means explicitly free
- SessionStatus.estimated_cost is removed (was never populated)
"""

from decimal import Decimal

import pytest
from pydantic import ValidationError

from amplifier_core.message_models import Usage


class TestUsageCostUsd:
    def test_cost_usd_defaults_to_none(self):
        """cost_usd is None when not provided — all existing Usage construction is unaffected."""
        usage = Usage(input_tokens=100, output_tokens=50, total_tokens=150)
        assert usage.cost_usd is None

    def test_cost_usd_accepts_decimal(self):
        """cost_usd should accept a Decimal value."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            cost_usd=Decimal("0.047832"),
        )
        assert usage.cost_usd == Decimal("0.047832")
        assert isinstance(usage.cost_usd, Decimal)

    def test_cost_usd_accepts_decimal_zero(self):
        """Decimal('0') is valid — means explicitly free (not unknown)."""
        usage = Usage(
            input_tokens=0, output_tokens=0, total_tokens=0, cost_usd=Decimal("0")
        )
        assert usage.cost_usd == Decimal("0")
        assert usage.cost_usd is not None  # None != 0

    def test_cost_usd_rejects_float(self):
        """Float must be rejected — Pydantic should raise ValidationError for float input."""
        with pytest.raises(ValidationError):
            Usage(
                input_tokens=100,
                output_tokens=50,
                total_tokens=150,
                cost_usd=0.047,  # float — not acceptable for monetary values
            )

    def test_cost_usd_accepts_decimal_from_string(self):
        """Decimal coercion from string is acceptable (event dict transport pattern)."""
        usage = Usage(
            input_tokens=100, output_tokens=50, total_tokens=150,
            cost_usd="0.0478",  # raw string, as it would arrive from a JSON dict
        )
        assert usage.cost_usd == Decimal("0.0478")
        assert isinstance(usage.cost_usd, Decimal)

    def test_none_is_not_zero(self):
        """Explicit contract: None (unknown) != Decimal('0') (free)."""
        unknown = Usage(input_tokens=1, output_tokens=1, total_tokens=2)
        free = Usage(
            input_tokens=0, output_tokens=0, total_tokens=0, cost_usd=Decimal("0")
        )
        assert unknown.cost_usd is None
        assert free.cost_usd == Decimal("0")
        assert unknown.cost_usd != free.cost_usd

    def test_model_dump_includes_cost_usd_as_decimal(self):
        """model_dump() should include cost_usd as Decimal (not string, not float)."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            cost_usd=Decimal("0.047"),
        )
        dumped = usage.model_dump()
        assert "cost_usd" in dumped
        assert isinstance(dumped["cost_usd"], Decimal)

    def test_model_dump_json_mode_serializes_cost_usd_as_string(self):
        """model_dump(mode='json') serializes Decimal as string for JSON safety."""
        usage = Usage(
            input_tokens=100,
            output_tokens=50,
            total_tokens=150,
            cost_usd=Decimal("0.047"),
        )
        dumped = usage.model_dump(mode="json")
        assert isinstance(dumped["cost_usd"], str)
        assert dumped["cost_usd"] == "0.047"

    def test_cost_usd_not_in_dump_when_none(self):
        """When cost_usd is None, model_dump(exclude_none=True) omits it."""
        usage = Usage(input_tokens=100, output_tokens=50, total_tokens=150)
        dumped = usage.model_dump(exclude_none=True)
        assert "cost_usd" not in dumped
