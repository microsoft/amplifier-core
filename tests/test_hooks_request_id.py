"""Tests for LLM call correlation (`request_id`) stamping in HookRegistry.emit().

`llm:request` and `llm:response` shared no field identifying the call they
belonged to, so consumers paired them positionally (FIFO). Measured on real
captures, any concurrently-issued call -- a background summarizer, a
session-naming hook -- crosses the pairing silently: both events parse and the
resulting cost attribution is simply wrong.

These tests pin the fix at both levels:

* the policy in `amplifier_core.correlation` (pure Python, exhaustive), and
* the emit path in `HookRegistry.emit()` (end-to-end, including the
  concurrency case FIFO gets wrong).
"""

import asyncio
import uuid

import pytest
from amplifier_core import correlation
from amplifier_core.correlation import REQUEST_ID_FIELD
from amplifier_core.correlation import current_request_id
from amplifier_core.correlation import new_request_id
from amplifier_core.correlation import resolve_request_id
from amplifier_core.hooks import HookRegistry
from amplifier_core.models import HookResult


@pytest.fixture(autouse=True)
def _clean_correlation_slot():
    """Each test starts with no call in flight."""
    correlation.reset_request_id()
    yield
    correlation.reset_request_id()


def _recorder(sink):
    async def handler(event, data):
        sink.append((event, dict(data)))
        return HookResult(action="continue")

    return handler


# ---------------------------------------------------------------------------
# Policy: amplifier_core.correlation
# ---------------------------------------------------------------------------


def test_field_name_is_request_id():
    """The field name is contract surface -- pinned here and in the Rust bridge."""
    assert REQUEST_ID_FIELD == "request_id"


def test_new_request_id_is_a_uuid4_string():
    value = new_request_id()
    assert isinstance(value, str)
    parsed = uuid.UUID(value)
    assert parsed.version == 4
    assert str(parsed) == value


def test_request_generates_and_response_echoes():
    request_id = resolve_request_id("llm:request")
    assert request_id
    assert resolve_request_id("llm:response") == request_id


def test_each_request_gets_a_distinct_id():
    first = resolve_request_id("llm:request")
    resolve_request_id("llm:response")
    second = resolve_request_id("llm:request")
    assert first != second


def test_explicit_request_id_wins_and_is_adopted():
    assert resolve_request_id("llm:request", "provider-supplied") == "provider-supplied"
    assert resolve_request_id("llm:response") == "provider-supplied"


def test_response_without_a_request_has_no_id():
    """Absent is always better than wrong -- never invent a pairing."""
    assert resolve_request_id("llm:response") is None


def test_closed_call_is_not_reused_by_a_later_event():
    """A stale id would silently mis-attribute; the slot closes on the terminal event."""
    request_id = resolve_request_id("llm:request")
    assert resolve_request_id("llm:response") == request_id
    assert resolve_request_id("llm:response") is None
    assert resolve_request_id("provider:throttle") is None


def test_error_path_echoes_the_request_id():
    """A call that times out has no response -- provider:error carries the id instead."""
    request_id = resolve_request_id("llm:request")
    assert resolve_request_id("provider:error") == request_id


def test_retry_and_throttle_echo_without_ending_the_call():
    request_id = resolve_request_id("llm:request")
    assert resolve_request_id("provider:retry") == request_id
    assert resolve_request_id("provider:throttle") == request_id
    assert resolve_request_id("llm:response") == request_id


def test_unrelated_events_carry_no_correlation_id():
    resolve_request_id("llm:request")
    for event in ("tool:pre", "session:start", "content_block:delta", "provider:resolve"):
        assert resolve_request_id(event) is None


def test_current_request_id_tracks_the_in_flight_call():
    assert current_request_id() is None
    request_id = resolve_request_id("llm:request")
    assert current_request_id() == request_id
    resolve_request_id("llm:response")
    assert current_request_id() is None


# ---------------------------------------------------------------------------
# Emit path: HookRegistry.emit()
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_emit_stamps_request_id_on_llm_request():
    registry = HookRegistry()
    seen = []
    registry.register("llm:request", _recorder(seen), name="capture")

    await registry.emit("llm:request", {"provider": "anthropic", "model": "m"})

    assert len(seen) == 1
    request_id = seen[0][1].get(REQUEST_ID_FIELD)
    assert request_id, "llm:request must carry a non-null request_id"
    assert uuid.UUID(request_id).version == 4


@pytest.mark.asyncio
async def test_emit_pairs_request_and_response_by_id():
    registry = HookRegistry()
    seen = []
    registry.register("llm:request", _recorder(seen), name="req")
    registry.register("llm:response", _recorder(seen), name="resp")

    await registry.emit("llm:request", {"provider": "anthropic"})
    await registry.emit("llm:response", {"provider": "anthropic", "usage": {}})

    request_id = seen[0][1][REQUEST_ID_FIELD]
    assert request_id
    assert seen[1][1][REQUEST_ID_FIELD] == request_id


@pytest.mark.asyncio
async def test_emit_preserves_an_explicit_request_id():
    registry = HookRegistry()
    seen = []
    registry.register("llm:request", _recorder(seen), name="req")
    registry.register("llm:response", _recorder(seen), name="resp")

    await registry.emit("llm:request", {REQUEST_ID_FIELD: "upstream-42"})
    await registry.emit("llm:response", {})

    assert seen[0][1][REQUEST_ID_FIELD] == "upstream-42"
    assert seen[1][1][REQUEST_ID_FIELD] == "upstream-42"


@pytest.mark.asyncio
async def test_emit_leaves_unrelated_events_untouched():
    """Consumers of every other event see a byte-identical payload."""
    registry = HookRegistry()
    seen = []
    registry.register("tool:pre", _recorder(seen), name="tool")

    await registry.emit("tool:pre", {"tool": "read_file"})

    assert REQUEST_ID_FIELD not in seen[0][1]


@pytest.mark.asyncio
async def test_emit_omits_request_id_when_no_call_is_in_flight():
    """An event stream with no request still emits cleanly -- no invented id."""
    registry = HookRegistry()
    seen = []
    registry.register("llm:response", _recorder(seen), name="resp")

    await registry.emit("llm:response", {"provider": "anthropic"})

    assert REQUEST_ID_FIELD not in seen[0][1]
    assert seen[0][1]["provider"] == "anthropic"


@pytest.mark.asyncio
async def test_emit_carries_request_id_onto_the_error_path():
    """The timeout case: a request with no response must still be attributable."""
    registry = HookRegistry()
    seen = []
    registry.register("llm:request", _recorder(seen), name="req")
    registry.register("provider:error", _recorder(seen), name="err")

    await registry.emit("llm:request", {"provider": "anthropic"})
    await registry.emit("provider:error", {"error": "timeout after 10s"})

    assert seen[1][1][REQUEST_ID_FIELD] == seen[0][1][REQUEST_ID_FIELD]


@pytest.mark.asyncio
async def test_concurrent_calls_get_distinct_ids_and_pair_correctly():
    """The case FIFO gets wrong.

    Interleaving is forced to reproduce the measured trace (agent request,
    summarizer request, summarizer response, agent response) where positional
    pairing charges each response to the other caller.
    """
    registry = HookRegistry()
    seen = []
    for event in ("llm:request", "llm:response"):
        registry.register(event, _recorder(seen), name=f"cap-{event}")

    summarizer_requested = asyncio.Event()
    summarizer_responded = asyncio.Event()

    async def agent_call():
        await registry.emit("llm:request", {"caller": "agent"})
        await summarizer_responded.wait()
        await registry.emit("llm:response", {"caller": "agent"})

    async def summarizer_call():
        await summarizer_requested.wait()
        await registry.emit("llm:request", {"caller": "summarizer"})
        await registry.emit("llm:response", {"caller": "summarizer"})
        summarizer_responded.set()

    async def run():
        task_agent = asyncio.create_task(agent_call())
        task_summarizer = asyncio.create_task(summarizer_call())
        await asyncio.sleep(0)
        summarizer_requested.set()
        await asyncio.gather(task_agent, task_summarizer)

    await run()

    order = [(event, payload["caller"]) for event, payload in seen]
    assert order == [
        ("llm:request", "agent"),
        ("llm:request", "summarizer"),
        ("llm:response", "summarizer"),
        ("llm:response", "agent"),
    ], "expected the interleaving that defeats positional pairing"

    by_caller = {}
    for event, payload in seen:
        by_caller.setdefault(payload["caller"], {})[event] = payload[REQUEST_ID_FIELD]

    agent = by_caller["agent"]
    summarizer = by_caller["summarizer"]

    assert agent["llm:request"] == agent["llm:response"]
    assert summarizer["llm:request"] == summarizer["llm:response"]
    assert agent["llm:request"] != summarizer["llm:request"]

    # And the positional pairing this replaces would have crossed them:
    # FIFO joins the first request to the first response, which here belong
    # to different callers.
    requests = [p[REQUEST_ID_FIELD] for e, p in seen if e == "llm:request"]
    responses = [p[REQUEST_ID_FIELD] for e, p in seen if e == "llm:response"]
    assert requests[0] != responses[0], "expected FIFO to mis-pair this trace"


@pytest.mark.asyncio
async def test_many_concurrent_calls_all_pair_correctly():
    registry = HookRegistry()
    seen = []
    for event in ("llm:request", "llm:response"):
        registry.register(event, _recorder(seen), name=f"cap-{event}")

    async def one_call(index: int):
        await registry.emit("llm:request", {"caller": index})
        # Yield control so every call is genuinely in flight at once.
        await asyncio.sleep(0.01)
        await registry.emit("llm:response", {"caller": index})

    await asyncio.gather(*(one_call(i) for i in range(20)))

    ids = {}
    for event, payload in seen:
        ids.setdefault(payload["caller"], {})[event] = payload[REQUEST_ID_FIELD]

    assert len(ids) == 20
    for caller, pair in ids.items():
        assert pair["llm:request"] == pair["llm:response"], f"caller {caller} mis-paired"
    assert len({pair["llm:request"] for pair in ids.values()}) == 20


# ---------------------------------------------------------------------------
# Backward compatibility
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_id_ignoring_consumer_sees_an_otherwise_identical_payload():
    """The change is additive: nothing existing is renamed, moved, or dropped."""
    registry = HookRegistry()
    seen = []
    registry.register("llm:response", _recorder(seen), name="resp")

    payload = {"provider": "anthropic", "model": "m", "usage": {"input_tokens": 7}}
    await registry.emit("llm:request", {"provider": "anthropic"})
    await registry.emit("llm:response", dict(payload))

    observed = seen[0][1]
    for key, value in payload.items():
        assert observed[key] == value
    assert set(observed) - set(payload) <= {REQUEST_ID_FIELD, "timestamp"}


def test_historical_capture_without_request_id_still_parses():
    """Every capture already on disk has no request_id -- it must stay readable.

    Consumers treat the field as optional and fall back to their prior
    heuristic; the kernel never rewrites history.
    """
    historical = [
        {"event": "llm:request", "data": {"provider": "openai", "model": "gpt-5"}},
        {"event": "llm:response", "data": {"provider": "openai", "usage": {}}},
    ]

    for record in historical:
        assert record["data"].get(REQUEST_ID_FIELD) is None

    paired = list(
        zip(
            [r for r in historical if r["event"] == "llm:request"],
            [r for r in historical if r["event"] == "llm:response"],
        )
    )
    assert len(paired) == 1
