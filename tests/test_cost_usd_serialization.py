"""Tests for Decimal-safe JSON serialization on Usage.cost_usd and SessionStatus.cost_usd.

Background: M2 cost-tracking introduced `cost_usd: Decimal | None` on Usage and
SessionStatus. Without serializer help, `usage.model_dump()` emits Decimal values
that crash `json.dumps()`, leaking through every downstream event-payload writer.

The model defends this at the source via `@field_serializer(when_used='always')`,
so plain `model_dump()` and `model_dump(mode='json')` produce identical JSON-safe
output. Decimal precision is preserved as a string ('0.047' not '0.047000000001').
Direct attribute access still returns Decimal for in-memory arithmetic.

Plus: ToolResult.get_serialized_output() uses a `default=` encoder so any Decimal
that flows through tool result payloads also serializes safely.

Revives the approach from amplifier-core PR #73 with one refinement:
when_used='always' (not the default mode='json'-only behavior), so plain
model_dump() called from orchestrators emits JSON-safe payloads automatically.
"""

import json
from decimal import Decimal

import pytest

from amplifier_core.message_models import Usage
from amplifier_core.models import SessionStatus
from amplifier_core.models import ToolResult


# ---------------------------------------------------------------------------
# Usage.cost_usd serialization
# ---------------------------------------------------------------------------


class TestUsageCostUsdSerialization:
    """Usage.cost_usd serializes Decimal -> str on every model_dump path."""

    def test_model_dump_returns_string(self):
        u = Usage(
            input_tokens=10,
            output_tokens=20,
            total_tokens=30,
            cost_usd=Decimal("0.047"),
        )
        dumped = u.model_dump()
        assert isinstance(dumped["cost_usd"], str)
        assert dumped["cost_usd"] == "0.047"

    def test_json_dumps_does_not_crash(self):
        u = Usage(
            input_tokens=10,
            output_tokens=20,
            total_tokens=30,
            cost_usd=Decimal("0.047"),
        )
        # The whole point: plain json.dumps must not crash.
        serialized = json.dumps(u.model_dump())
        assert '"cost_usd": "0.047"' in serialized

    def test_direct_attribute_access_still_decimal(self):
        u = Usage(
            input_tokens=1,
            output_tokens=1,
            total_tokens=2,
            cost_usd=Decimal("0.047"),
        )
        assert isinstance(u.cost_usd, Decimal)
        assert u.cost_usd == Decimal("0.047")

    def test_float_validator_still_rejects(self):
        with pytest.raises(Exception, match="must be Decimal"):
            Usage(
                input_tokens=1,
                output_tokens=1,
                total_tokens=2,
                cost_usd=0.047,  # float — must be rejected pre-serialization
            )

    def test_extra_allow_still_works(self):
        u = Usage(
            input_tokens=1,
            output_tokens=1,
            total_tokens=2,
            cost_usd=Decimal("0.001"),
            provider_specific="foo",
        )
        dumped = u.model_dump()
        assert dumped["provider_specific"] == "foo"
        assert dumped["cost_usd"] == "0.001"

    def test_none_survives_as_none(self):
        """None != Decimal('0') invariant: unknown cost stays None."""
        u = Usage(input_tokens=1, output_tokens=1, total_tokens=2, cost_usd=None)
        dumped = u.model_dump()
        assert dumped["cost_usd"] is None

    def test_zero_decimal_survives_as_string_zero(self):
        """Decimal('0') = confirmed free, distinct from None = unknown."""
        u = Usage(
            input_tokens=1,
            output_tokens=1,
            total_tokens=2,
            cost_usd=Decimal("0"),
        )
        dumped = u.model_dump()
        assert dumped["cost_usd"] == "0"
        assert dumped["cost_usd"] is not None

    def test_round_trip_preserves_decimal(self):
        u = Usage(
            input_tokens=10,
            output_tokens=20,
            total_tokens=30,
            cost_usd=Decimal("0.047"),
        )
        rebuilt = Usage.model_validate(u.model_dump())
        assert isinstance(rebuilt.cost_usd, Decimal)
        assert rebuilt.cost_usd == Decimal("0.047")

    @pytest.mark.parametrize(
        "value",
        [
            Decimal("0.0000123"),
            Decimal("12345.6789"),
            Decimal("0"),
            Decimal("1E-9"),
        ],
    )
    def test_precision_preserved_across_scales(self, value):
        u = Usage(input_tokens=1, output_tokens=1, total_tokens=2, cost_usd=value)
        dumped = u.model_dump()
        assert dumped["cost_usd"] == str(value)

    def test_plain_mode_matches_json_mode(self):
        """Critical: orchestrators emit via plain model_dump() — must match json mode."""
        u = Usage(
            input_tokens=1,
            output_tokens=1,
            total_tokens=2,
            cost_usd=Decimal("0.047"),
        )
        assert u.model_dump() == u.model_dump(mode="json")

    def test_int_coerces_to_decimal_then_serializes(self):
        """Pydantic coerces int -> Decimal; serializer still emits string."""
        u = Usage(input_tokens=1, output_tokens=1, total_tokens=2, cost_usd=5)
        assert isinstance(u.cost_usd, Decimal)
        assert u.model_dump()["cost_usd"] == "5"


# ---------------------------------------------------------------------------
# SessionStatus.cost_usd serialization (mirror of Usage)
# ---------------------------------------------------------------------------


class TestSessionStatusCostUsdSerialization:
    """SessionStatus carries the same cost_usd contract as Usage."""

    def test_model_dump_returns_string(self):
        s = SessionStatus(session_id="abc", cost_usd=Decimal("1.23"))
        dumped = s.model_dump()
        assert isinstance(dumped["cost_usd"], str)
        assert dumped["cost_usd"] == "1.23"

    def test_json_dumps_does_not_crash(self):
        s = SessionStatus(session_id="abc", cost_usd=Decimal("1.23"))
        # to_dict() already uses mode='json'; model_dump() must also be safe.
        json.dumps(s.model_dump(), default=str)  # default=str fallback for datetime
        # Real test: the cost_usd field specifically does not require default=
        dumped = s.model_dump()
        assert isinstance(dumped["cost_usd"], str)

    def test_direct_attribute_access_still_decimal(self):
        s = SessionStatus(session_id="abc", cost_usd=Decimal("1.23"))
        assert isinstance(s.cost_usd, Decimal)

    def test_float_validator_still_rejects(self):
        with pytest.raises(Exception, match="must be Decimal"):
            SessionStatus(session_id="abc", cost_usd=1.23)

    def test_none_survives_as_none(self):
        s = SessionStatus(session_id="abc", cost_usd=None)
        assert s.model_dump()["cost_usd"] is None


# ---------------------------------------------------------------------------
# ToolResult.get_serialized_output Decimal handling
# ---------------------------------------------------------------------------


class TestToolResultDecimalSafety:
    """ToolResult.get_serialized_output uses default=_json_default for Decimal."""

    def test_dict_output_with_decimal_serializes(self):
        """Tool output dict containing Decimal must not crash json.dumps."""
        r = ToolResult(success=True, output={"price": Decimal("9.99"), "qty": 3})
        result = r.get_serialized_output()
        # Decimal converted to string in JSON
        assert '"price": "9.99"' in result
        assert '"qty": 3' in result

    def test_list_output_with_decimal_serializes(self):
        r = ToolResult(success=True, output=[Decimal("1.50"), Decimal("2.50")])
        result = r.get_serialized_output()
        assert '"1.50"' in result
        assert '"2.50"' in result

    def test_nested_decimal_in_dict_serializes(self):
        r = ToolResult(
            success=True,
            output={"items": [{"cost": Decimal("0.05")}, {"cost": Decimal("0.10")}]},
        )
        result = r.get_serialized_output()
        assert '"0.05"' in result
        assert '"0.10"' in result

    def test_non_decimal_unhandled_type_still_raises(self):
        """_json_default should only handle Decimal — other unsupported types must raise."""

        class CustomThing:
            pass

        r = ToolResult(success=True, output={"thing": CustomThing()})
        with pytest.raises(TypeError, match="not JSON serializable"):
            r.get_serialized_output()

    def test_string_output_unchanged(self):
        """Non-dict/list output paths untouched by Decimal handling."""
        r = ToolResult(success=True, output="plain string")
        assert r.get_serialized_output() == "plain string"

    def test_no_output_unchanged(self):
        r = ToolResult(success=True, output=None)
        assert r.get_serialized_output() == "Success"
