"""Cost serialization contract for Usage, SessionStatus, and ToolResult.

cost_usd is Decimal in memory (precision-preserving for monetary arithmetic)
and a JSON-safe string when serialized. Callers can dump and json.dumps the
result without specifying a mode or providing a default= encoder.

None ≠ Decimal("0"): unknown cost is distinct from known-zero cost, and both
must survive serialization without collapsing into one another.

Tool outputs containing Decimal values serialize safely too — the encoder
narrowly handles Decimal; other unsupported types still raise so real bugs
surface clearly.
"""

import json
from decimal import Decimal

import pytest

from amplifier_core.message_models import Usage
from amplifier_core.models import SessionStatus
from amplifier_core.models import ToolResult


# ---------------------------------------------------------------------------
# Usage.cost_usd
# ---------------------------------------------------------------------------


class TestUsageCostUsdSerialization:
    """Usage.cost_usd: Decimal in memory, string on the wire."""

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
        """Serialization yields JSON-safe values; callers never need a default= encoder."""
        u = Usage(
            input_tokens=10,
            output_tokens=20,
            total_tokens=30,
            cost_usd=Decimal("0.047"),
        )
        serialized = json.dumps(u.model_dump())
        assert '"cost_usd": "0.047"' in serialized

    def test_direct_attribute_access_still_decimal(self):
        """Attribute access preserves Decimal precision for in-memory arithmetic."""
        u = Usage(
            input_tokens=1,
            output_tokens=1,
            total_tokens=2,
            cost_usd=Decimal("0.047"),
        )
        assert isinstance(u.cost_usd, Decimal)
        assert u.cost_usd == Decimal("0.047")

    def test_float_validator_still_rejects(self):
        """Floats are rejected at validation — monetary precision requires Decimal."""
        with pytest.raises(Exception, match="must be Decimal"):
            Usage(
                input_tokens=1,
                output_tokens=1,
                total_tokens=2,
                cost_usd=0.047,
            )

    def test_extra_allow_still_works(self):
        """Provider-specific extra fields coexist with cost_usd serialization."""
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
        """None (unknown cost) survives serialization as None — distinct from Decimal('0')."""
        u = Usage(input_tokens=1, output_tokens=1, total_tokens=2, cost_usd=None)
        dumped = u.model_dump()
        assert dumped["cost_usd"] is None

    def test_zero_decimal_survives_as_string_zero(self):
        """Decimal('0') (known-zero cost) serializes to '0' — distinct from None (unknown)."""
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
        """A dumped value can be validated back into a model with Decimal precision intact."""
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
        """Serialization preserves precision from sub-cent through millions."""
        u = Usage(input_tokens=1, output_tokens=1, total_tokens=2, cost_usd=value)
        dumped = u.model_dump()
        assert dumped["cost_usd"] == str(value)

    def test_plain_mode_matches_json_mode(self):
        """All serialization modes produce identical JSON-safe output — no mode='json' required."""
        u = Usage(
            input_tokens=1,
            output_tokens=1,
            total_tokens=2,
            cost_usd=Decimal("0.047"),
        )
        assert u.model_dump() == u.model_dump(mode="json")

    def test_int_coerces_to_decimal_then_serializes(self):
        """Integer inputs are accepted (coerced to Decimal) and serialize as string."""
        u = Usage(input_tokens=1, output_tokens=1, total_tokens=2, cost_usd=5)
        assert isinstance(u.cost_usd, Decimal)
        assert u.model_dump()["cost_usd"] == "5"


# ---------------------------------------------------------------------------
# SessionStatus.cost_usd
# ---------------------------------------------------------------------------


class TestSessionStatusCostUsdSerialization:
    """SessionStatus.cost_usd: Decimal in memory, string on the wire."""

    def test_model_dump_returns_string(self):
        s = SessionStatus(session_id="abc", cost_usd=Decimal("1.23"))
        dumped = s.model_dump()
        assert isinstance(dumped["cost_usd"], str)
        assert dumped["cost_usd"] == "1.23"

    def test_json_dumps_does_not_crash(self):
        """cost_usd is JSON-safe regardless of other Pydantic-native fields like datetime."""
        s = SessionStatus(session_id="abc", cost_usd=Decimal("1.23"))
        # default=str here covers SessionStatus's datetime fields, not cost_usd —
        # cost_usd is already a string in the dump output.
        json.dumps(s.model_dump(), default=str)
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
# ToolResult Decimal handling
# ---------------------------------------------------------------------------


class TestToolResultDecimalSafety:
    """ToolResult tolerates Decimal in output without crashing JSON serialization."""

    def test_dict_output_with_decimal_serializes(self):
        """Tool output dict containing Decimal serializes — precision preserved as string."""
        r = ToolResult(success=True, output={"price": Decimal("9.99"), "qty": 3})
        result = r.get_serialized_output()
        assert '"price": "9.99"' in result
        assert '"qty": 3' in result

    def test_list_output_with_decimal_serializes(self):
        """Decimal values in list outputs serialize as strings."""
        r = ToolResult(success=True, output=[Decimal("1.50"), Decimal("2.50")])
        result = r.get_serialized_output()
        assert '"1.50"' in result
        assert '"2.50"' in result

    def test_nested_decimal_in_dict_serializes(self):
        """Decimal values nested inside compound structures serialize as strings."""
        r = ToolResult(
            success=True,
            output={"items": [{"cost": Decimal("0.05")}, {"cost": Decimal("0.10")}]},
        )
        result = r.get_serialized_output()
        assert '"0.05"' in result
        assert '"0.10"' in result

    def test_non_decimal_unhandled_type_still_raises(self):
        """The Decimal encoder is narrow — other unsupported types still raise so real bugs surface."""

        class CustomThing:
            pass

        r = ToolResult(success=True, output={"thing": CustomThing()})
        with pytest.raises(TypeError, match="not JSON serializable"):
            r.get_serialized_output()

    def test_string_output_unchanged(self):
        """Non-structured outputs pass through serialization unchanged."""
        r = ToolResult(success=True, output="plain string")
        assert r.get_serialized_output() == "plain string"

    def test_no_output_unchanged(self):
        """Empty output returns the success placeholder."""
        r = ToolResult(success=True, output=None)
        assert r.get_serialized_output() == "Success"
