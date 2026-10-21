# Implementation Specification: `claude-fable-5` Kernel + Orchestrator Compatibility

**Status:** Ready for implementation
**Author:** foundation:zen-architect (ARCHITECT mode)
**Scope:** `amplifier-core`, `amplifier-module-loop-streaming`, `amplifier-module-provider-anthropic`, contract docs
**Principle:** Kernel provides MECHANISM only; policy lives in modules/config. Additive, no breaking changes. Smallest change that works.

---

## 0. Overview

Anthropic's `claude-fable-5` introduces three behaviours the current stack does not model:

1. **`stop_reason: "refusal"` on HTTP 200.** A safety classifier can refuse a request and return `200` (not `4xx`), with `stop_details.category` (e.g. `"reasoning_extraction"`). The turn has empty/near-empty content. Today this silently terminates the agent loop as status `"incomplete"` — no event, no diagnostics, `stop_details` dropped on the floor.
2. **Model-bound thinking signatures.** Adaptive thinking is always on. Thinking blocks may carry **empty `thinking` text plus a `signature`**, and must round-trip **byte-identical on the same model**. When a request is retried on a *different* model, `thinking`/`redacted_thinking` blocks from prior assistant turns **must be stripped** (signatures are model-bound; sending them cross-model is rejected).
3. **`output_config.effort` (`low|medium|high|xhigh|max`)** is the primary depth/cost control. The provider already maps this from `ChatRequest.reasoning_effort` — but nothing in the contract docs models it, and `reasoning_effort` is an undocumented `extra="allow"` field.

The enabling mechanism for #1 and #2 is missing at the kernel level:

- `ChatResponse.finish_reason` (`message_models.py:235`) is a free-form string that **nothing in the orchestrator reads**.
- `Message` (`message_models.py:117-130`) carries **no record of which model produced it**, so the model-switch thinking-strip rule is *unimplementable* today.

This spec delivers the mechanisms (a provenance field, a refusal event, a stop-details carrier, and a strip rule), wires the one live orchestrator path to read `finish_reason`, and leaves the *policy* (which fallback model, how many retries) to provider config / hooks — exactly where the existing Anthropic fallback machinery already lives.

### Ground truth verified in current sources

| Fact | Location (verified) |
|---|---|
| `ChatResponse.finish_reason: str \| None` exists, nothing reads it | `amplifier-core/amplifier_core/message_models.py:235` |
| `Message` has `role, content, name, tool_call_id, metadata`; no producer field | `amplifier-core/amplifier_core/message_models.py:117-130` |
| `ThinkingBlock` (`thinking`, `signature`) / `RedactedThinkingBlock` (`data`) | `message_models.py:37-56` |
| Both models are `ConfigDict(extra="allow")` | `message_models.py:30,124,229` |
| Canonical events; `PROVIDER_REQUEST/RESPONSE/ERROR`; no refusal event; `ALL_EVENTS` list | `amplifier-core/amplifier_core/events.py:20-22, 62-95` |
| **Live** orchestrator path is the non-streaming branch; `provider.complete()` | cache `amplifier-module-loop-streaming-b0b975ea6a1072dd/amplifier_module_loop_streaming/__init__.py:481-488` |
| Branch point on tool calls (the "line 385 equivalent") — `if not tool_calls:` | same file, `:573-575` |
| `finish_reason` is NOT read anywhere in the loop; empty content → break → status `"incomplete"` | same file, `:575-649`, `:189-204` |
| Dormant streaming path accumulates text only (drops thinking + tool calls) | same file, `_stream_from_provider`, `:~1018-1029` |
| Anthropic provider already passes `stop_reason` through as `finish_reason`; drops `stop_details` | cache `amplifier-module-provider-anthropic-5181591dcf06d076/.../__init__.py:3751-3759` |
| `complete()` already maps `reasoning_effort` → thinking + `output_config.effort` | same file, `:1711, :2169, :2374-2378` |
| Message→API conversion + thinking-block handling lives in provider | same file, `_convert_messages :3178`, thinking at `:3276-3283` |
| Provider already switches models internally via overload fallback (invisible to orchestrator) | same file, `_resolve_effective_model :1313-1335`, `_open_fallback_window :1374` |
| Fallback config keys already exist (`fallback_*`, `persist_fallback_state`) | same file, `:526-548, :739-802` |

> ★ **Insight:** the provider already switches models on you (overload fallback, `_resolve_effective_model:1313`) without telling the orchestrator. That single fact settles the "who strips thinking?" debate before it starts — only the component that owns the model switch can know a switch happened. The orchestrator is structurally blind to it.

---

## 1. Design decisions (decided, with justification)

### 1.1 Model provenance → **first-class `Message.producer_model` field** (NOT `metadata`)

**Decision:** Add `producer_model: str | None = None` as a first-class optional field on `Message`.

**Why not `metadata`:** The ecosystem convention is that `metadata` is the property bag for *provider-specific* state (its own inline comment: "Provider-specific state (e.g., OpenAI reasoning items)", `message_models.py:130`). Model provenance is the opposite of provider-specific — it is a **universal, cross-provider correctness fact** that a *consumer* (the strip rule) must read deterministically. Burying it in a free-form dict means the stripper must guess a provider-specific key, it is invisible in the JSON schema / IDE hover / `help()`, and it collides with whatever else a provider stuffs into `metadata`. A load-bearing field read for a correctness decision earns a typed, greppable, schema-visible home.

**Why first-class is kernel-legal:** It is purely additive (`Optional`, default `None`), so every existing module and serialized transcript keeps working (kernel "don't break userspace" rule). It is a small, stable, text-first schema addition — exactly what the kernel is for (mechanism: "a message records which model produced it"). The kernel does not decide *what to do* with it; that is the module's strip policy.

**Rejected alternative considered:** two-implementation rule pushback ("only Anthropic needs it"). Overruled because the field is not Anthropic-shaped — OpenAI reasoning items are equally response/model-bound — and because deferring it leaves the strip rule unimplementable, which is the whole point of the change.

### 1.2 Thinking-strip on model switch → **provider, at request-build time** (NOT orchestrator)

**Decision:** The Anthropic provider strips `thinking` / `redacted_thinking` blocks from prior assistant turns during message conversion, when the block's `producer_model` differs from the model this request will actually be sent to (`effective_model`).

**Why the provider, not the orchestrator:**
- The provider **owns model selection**, including the internal overload-fallback that swaps models *without the orchestrator's knowledge* (`_resolve_effective_model:1313`). The orchestrator cannot strip correctly because it cannot see the switch.
- Signatures being model-bound is **vendor knowledge** (Anthropic-specific). Per kernel philosophy, vendor specifics live at the edges (the provider adapter), not in the policy-level orchestrator.
- The provider already special-cases thinking blocks at exactly this seam (`_convert_messages:3276-3283`), so the strip is a local, low-risk addition — not a new traversal.

**The kernel's only job here** is to carry the provenance (`producer_model`) from response → stored message → next request so the provider *can* compare. Mechanism (kernel) + policy (provider) cleanly separated.

### 1.3 `stop_details` carrier → **`ChatResponse.metadata["stop_details"]`** (NOT a new field)

**Decision:** The provider writes `stop_details` (the raw Anthropic dict, e.g. `{"category": "reasoning_extraction"}`) into `ChatResponse.metadata` under the key `"stop_details"`. `finish_reason` continues to carry the raw `stop_reason` string (already wired at provider `:3755`).

**Why:** `stop_details` is genuinely provider-specific structured detail — the textbook use of the `metadata` property bag. `finish_reason` (the *category* of stop) is the cross-provider fact and stays first-class. No new `ChatResponse` field is needed; this is the smallest additive change.

### 1.4 Refusal handling → **new `provider:refusal` event + distinct turn status; retry is policy**

**Decision:** Add canonical event `PROVIDER_REFUSAL = "provider:refusal"`. The orchestrator, when `finish_reason == "refusal"`, emits it with the `stop_details` payload and **terminates the turn with a distinct status `"refused"`** (not `"incomplete"`). It does **not** append a corrupt empty assistant turn.

**Why retry-on-another-model is NOT in this change:** *Which* model to retry on and *how many times* is policy (kernel philosophy §2.2). The mechanism this spec ships (provenance + strip rule + refusal event + distinct status) is exactly what a policy layer needs to implement retry later — via provider config or a `provider:refusal` hook — without any further kernel change. Shipping the mechanism now and the policy later is the additive, decoupled path.

> 🔪 MJ Lens ─────────────────────────────────
> - **Bricks** — four independent bricks: (1) provenance field on `Message`, (2) refusal event + status in the loop, (3) `stop_details` in `metadata`, (4) provider strip rule. Each snaps in alone.
> - **How solid is it?** — logically forced, not guesswork: the audit proves `finish_reason` is unread and provenance is absent, so refusal-surfacing and cross-model stripping are *literally* impossible without (1) and (2). Evidence-backed by file:line.
> - **Adversarial / circular** — the one seam to watch: does the provider actually populate `producer_model` on the way *out*, and does the orchestrator persist it on the way *in*? If either half is missing the strip rule silently no-ops. Contract test §7.2 pins both halves.
> - **Grit** — medium. Contained additive edits at known seams; no reshaping of the loop or the envelope.
> - **Subtraction** — resisted a new `ChatResponse.stop_details` field (metadata already carries it) and resisted auto-retry (that's policy). What survives deletion: the provenance field and the refusal event. Everything else hangs off those two.
> - **Buildable now** — first increment: §2 (provenance field) + §4.1 (read `finish_reason`). Prove it by asserting `producer_model` survives one response→request round-trip.
> `─────────────────────────────────────────────`

---

## 2. Change: `amplifier-core` — `Message.producer_model` (provenance mechanism)

**File:** `amplifier-core/amplifier_core/message_models.py`
**Location:** `Message` class, `:117-130`

### 2.1 Exact edit

Add one field after `metadata` (line 130):

```python
class Message(BaseModel):
    """Single message in conversation history.

    Messages contain role and content which can be either a string or
    a list of ContentBlocks for multimodal/structured content.
    """

    model_config = ConfigDict(extra="allow")

    role: Literal["system", "developer", "user", "assistant", "function", "tool"]
    content: Union[str, list[ContentBlockUnion]]
    name: str | None = None
    tool_call_id: str | None = None
    metadata: dict[str, Any] | None = None  # Provider-specific state (e.g., OpenAI reasoning items)
    producer_model: str | None = Field(
        default=None,
        description=(
            "Model identifier that produced this message (assistant turns only). "
            "Provenance mechanism: consumers compare this against the target model "
            "to decide whether model-bound content (e.g. Anthropic thinking-block "
            "signatures) must be stripped before re-sending. None for user/tool/"
            "system messages and for messages produced before provenance stamping."
        ),
    )
```

`Field` is already imported (`message_models.py:24`).

### 2.2 Back-compat notes

- Purely additive, `Optional`, default `None`. Existing constructors (`Message(**msg)` at orchestrator `:414`) are unaffected.
- Serialized transcripts without `producer_model` deserialize fine (default applies).
- `extra="allow"` means even if a stored dict already carried a stray `producer_model`, no conflict is introduced.

### 2.3 Success criteria

- `Message(role="assistant", content="hi").producer_model is None` ✓
- `Message(role="assistant", content="hi", producer_model="claude-fable-5").producer_model == "claude-fable-5"` ✓
- `python_check` clean on `message_models.py`.
- Existing `amplifier-core` test suite passes unchanged.

---

## 3. Change: `amplifier-core` — `provider:refusal` event vocabulary (mechanism)

**File:** `amplifier-core/amplifier_core/events.py`

### 3.1 Exact edits

Add the constant in the "Provider calls" block (after `:22`):

```python
# Provider calls (LLMs)
PROVIDER_REQUEST = "provider:request"
PROVIDER_RESPONSE = "provider:response"
PROVIDER_ERROR = "provider:error"
PROVIDER_REFUSAL = "provider:refusal"
```

Add to `ALL_EVENTS` (after `PROVIDER_ERROR`, `:73`):

```python
    PROVIDER_REQUEST,
    PROVIDER_RESPONSE,
    PROVIDER_ERROR,
    PROVIDER_REFUSAL,
```

### 3.2 Semantics (normative — belongs in the event's docstring block / provider spec)

`provider:refusal` is emitted by the orchestrator when a provider returns a **successful** HTTP response whose `finish_reason` indicates a safety refusal (Anthropic `stop_reason == "refusal"`), i.e. the model declined to answer rather than erroring. It is distinct from `provider:error` (transport/HTTP failure) and from a normal empty completion. Payload spec in §4.2.

### 3.3 Back-compat notes

- Additive to a stable list. Hooks that iterate `ALL_EVENTS` gain one entry; none break.
- No existing emitter uses the name, so there is no collision (per the "zero consumers → no compat burden" lesson, we add cleanly, no alias).

### 3.4 Success criteria

- `from amplifier_core.events import PROVIDER_REFUSAL` resolves to `"provider:refusal"`.
- `PROVIDER_REFUSAL in ALL_EVENTS` is `True`.

---

## 4. Change: `amplifier-module-loop-streaming` — read `finish_reason`, surface refusal

> ⚠️ **Spec against the CACHE copy**, not the ~6-month-stale `~/dev` checkout.
> **File:** `~/.amplifier/cache/amplifier-module-loop-streaming-b0b975ea6a1072dd/amplifier_module_loop_streaming/__init__.py`
> The upstream repo to PR is `microsoft/amplifier-module-loop-streaming`. The builder must first reconcile: confirm the live non-streaming branch (`:481-649`) and the `execute()` status logic (`:189-204`) match what ships on `main`, then apply the edits below to the current `main`.

The **only live path** is the non-streaming branch (`else:` at `:481` — providers without a real `stream` method). The refusal check goes there, immediately after `parse_tool_calls`, before the existing `if not tool_calls:` branch at `:575`.

### 4.1 Add refusal detection in `_execute_stream` (non-streaming branch)

**Insert after `tool_calls = provider.parse_tool_calls(response)` (`:573`), before `if not tool_calls:` (`:575`):**

```python
                # Parse tool calls
                tool_calls = provider.parse_tool_calls(response)

                # --- Refusal handling (fable-5+): a safety refusal arrives as a
                # successful response with finish_reason == "refusal" and (near-)
                # empty content. Surface it explicitly instead of silently ending
                # the turn as "incomplete". Retrying on another model is POLICY
                # (provider config / a provider:refusal hook), not handled here.
                finish_reason = getattr(response, "finish_reason", None)
                if finish_reason == "refusal":
                    stop_details = None
                    resp_meta = getattr(response, "metadata", None)
                    if isinstance(resp_meta, dict):
                        stop_details = resp_meta.get("stop_details")
                    await hooks.emit(
                        PROVIDER_REFUSAL,
                        {
                            "provider": provider_name,
                            "iteration": iteration,
                            "finish_reason": finish_reason,
                            "stop_details": stop_details,  # e.g. {"category": "reasoning_extraction"}
                            "metadata": None,
                        },
                    )
                    # Do NOT append an empty/near-empty assistant turn — that would
                    # corrupt context (orphaned/blank assistant message). End the
                    # turn cleanly and signal refusal to execute().
                    self._refused = True
                    self._refusal_stop_details = stop_details
                    self._steering_queue.clear()
                    return
```

**Import** at the top of the file (with the other event imports, `:20-30`):

```python
from amplifier_core.events import PROVIDER_REFUSAL
```

**Instance state** — initialise in `__init__` (near `:86`, alongside `_cancel_requested_emitted`):

```python
        self._refused: bool = False
        self._refusal_stop_details: Any | None = None
```

**Reset** at the top of `execute()` (near `:171`, alongside `self._cancel_requested_emitted = False`):

```python
        self._refused = False
        self._refusal_stop_details = None
```

> Note on the empty-content edge (finding #2): even if a future model returns `finish_reason == "refusal"` *with* tool calls or text, this check runs before the `if not tool_calls` split and returns immediately, so refusal always wins and never falls through into the normal tool/again branch.

### 4.2 Emit a distinct terminal status in `execute()`

**File location:** `execute()` status computation, `:189-204`.

**Current:**
```python
        if error:
            status = "error"
        elif coordinator and coordinator.cancellation.is_cancelled:
            status = "cancelled"
        else:
            status = "success" if full_response else "incomplete"
```

**Change to:**
```python
        if error:
            status = "error"
        elif coordinator and coordinator.cancellation.is_cancelled:
            status = "cancelled"
        elif self._refused:
            status = "refused"
        else:
            status = "success" if full_response else "incomplete"
```

`ORCHESTRATOR_COMPLETE` (`:197-204`) then carries `status: "refused"`, giving observers a stable, distinct signal (previously indistinguishable from `"incomplete"`).

### 4.3 Return value on refusal

`execute()` returns `full_response` (empty on refusal). Keep that — callers already handle empty strings, and the `provider:refusal` event + `"refused"` status are the machine-readable channel. Do **not** synthesize a fake assistant answer.

**`provider:refusal` payload spec (normative):**

| Key | Type | Meaning |
|---|---|---|
| `provider` | `str` | Provider name that refused (e.g. `"anthropic"`). |
| `iteration` | `int` | 1-based loop iteration at refusal. |
| `finish_reason` | `str` | Always `"refusal"` for this event. |
| `stop_details` | `dict \| None` | Raw provider detail, e.g. `{"category": "reasoning_extraction"}`. `None` if the provider did not supply it. |
| `metadata` | `None` | Reserved extensibility slot (ecosystem convention). |

### 4.4 Register the event for observability

In `mount()`, the `observability.events` contributor list (`:47-55`) is the loop's own events. `provider:refusal` is a provider-domain event the orchestrator emits on the provider's behalf; register it so `hooks-logging` auto-discovers it:

```python
    coordinator.register_contributor(
        "observability.events",
        "loop-streaming",
        lambda: [
            "execution:start",
            "execution:end",
            "orchestrator:steering_injected",
            "provider:refusal",  # emitted when a provider returns a safety refusal (HTTP 200)
        ],
    )
```

### 4.5 Success criteria

- With a stub provider returning `ChatResponse(content=[], finish_reason="refusal", metadata={"stop_details": {"category": "reasoning_extraction"}})`:
  - a `provider:refusal` event fires exactly once with the payload above;
  - `execute()` returns `""`;
  - the `orchestrator:complete` event carries `status == "refused"`;
  - **no** assistant message is appended to context (assert `context.messages` gained none from this turn).
- With a normal response (`finish_reason="end_turn"`, non-empty content): behaviour is byte-for-byte unchanged (no refusal event, status `"success"`). Regression-guard this.
- `python_check` clean.

---

## 5. Change: `amplifier-module-provider-anthropic` — stamp provenance, carry `stop_details`, strip on switch

**File:** cache `amplifier-module-provider-anthropic-5181591dcf06d076/amplifier_module_provider_anthropic/__init__.py` (PR upstream `microsoft/amplifier-module-provider-anthropic`).

### 5.1 Carry `stop_details` on the response (`metadata`)

**Location:** response construction, `:3751-3759` (`return AnthropicChatResponse(...)`).

The raw Anthropic `response.stop_reason` is already mapped to `finish_reason` (`:3755`). Add `stop_details` capture into `metadata`:

```python
        # Capture stop_details (fable-5+ safety refusals populate this, e.g.
        # {"category": "reasoning_extraction"}). finish_reason already carries
        # the raw stop_reason string; stop_details is provider-specific detail,
        # so it rides in the metadata property bag rather than a new field.
        response_metadata: dict[str, Any] = {}
        raw_stop_details = getattr(response, "stop_details", None)
        if raw_stop_details is not None:
            response_metadata["stop_details"] = (
                raw_stop_details.model_dump()
                if hasattr(raw_stop_details, "model_dump")
                else dict(raw_stop_details)
                if isinstance(raw_stop_details, dict)
                else {"category": getattr(raw_stop_details, "category", str(raw_stop_details))}
            )

        return AnthropicChatResponse(
            content=content_blocks,
            tool_calls=tool_calls if tool_calls else None,
            usage=usage,
            finish_reason=response.stop_reason,
            metadata=response_metadata or None,
            content_blocks=event_blocks if event_blocks else None,
            text=combined_text or None,
            web_search_results=web_search_results if web_search_results else None,
        )
```

If `AnthropicChatResponse` does not already accept/expose `metadata`, add it (its base `ChatResponse` already declares `metadata: dict[str, Any] | None`, `message_models.py:236`, so this is a passthrough — confirm the subclass does not shadow it).

### 5.2 Stamp `producer_model` on assistant messages the orchestrator will store

The orchestrator builds the stored assistant message from `response.content` and `response.metadata` (`:609-643, :664-719`). To stamp provenance without touching the orchestrator's persistence logic, the provider exposes the model it actually used, and the orchestrator copies it into the message.

**Two-part change:**

**(a) Provider — expose the effective model on the response.** In the same `metadata` dict from §5.1, add:

```python
        response_metadata["producer_model"] = response.model  # model that actually served this response
```

`response.model` is the Anthropic SDK's echoed model id (already read at `:3733` for cost). This is the *effective* model, so it correctly reflects any internal overload fallback.

**(b) Orchestrator — copy it onto the stored message.** In `amplifier-module-loop-streaming`, where the assistant message is assembled (both the no-tool branch `:638-643` and the tool branch `:716-721`), after the existing `metadata` passthrough add:

```python
                    # Stamp model provenance (fable-5+): record which model produced
                    # this turn so the provider can strip model-bound thinking
                    # signatures if a later request targets a different model.
                    if hasattr(response, "metadata") and response.metadata:
                        producer = response.metadata.get("producer_model")
                        if producer:
                            assistant_msg["producer_model"] = producer
```

Because `Message` now has a first-class `producer_model` (§2) and `Message(**msg)` is used at `:414`, the value survives the dict→`Message` round-trip on the next request.

> Rationale for splitting (a)/(b): the provider is the only component that knows the effective model; the orchestrator is the only component that persists messages. Neither can do it alone. The kernel field (§2) is the shared contract that lets them cooperate without a backchannel.

### 5.3 Strip model-bound thinking on model switch (request-build)

**Location:** `_convert_messages` (`:3178`) and/or its caller in `complete()` where `effective_model` is known (`_resolve_effective_model:1313` returns it).

**Rule (normative):** For each prior **assistant** message being converted, if it carries `producer_model` and `producer_model != effective_model`, **drop** its `thinking` and `redacted_thinking` blocks (and any stored `thinking_block`) before building the Anthropic payload. Preserve `text` and `tool_use` blocks unchanged. Messages with no `producer_model` (legacy/unstamped) are left as-is — the pre-fable-5 behaviour — since we cannot prove a mismatch.

**Implementation sketch** (apply at the point where `effective_model` is resolved, passing it into conversion):

```python
    def _strip_cross_model_thinking(
        self, messages: list[dict[str, Any]], effective_model: str
    ) -> list[dict[str, Any]]:
        """Remove model-bound thinking/redacted_thinking from assistant turns
        produced by a DIFFERENT model. Signatures are model-bound; sending a
        prior model's thinking to a new model is rejected. Text and tool_use
        are preserved. Unstamped messages (no producer_model) are untouched."""
        out: list[dict[str, Any]] = []
        for msg in messages:
            if msg.get("role") != "assistant":
                out.append(msg)
                continue
            producer = msg.get("producer_model")
            if not producer or producer == effective_model:
                out.append(msg)
                continue
            cleaned = dict(msg)
            cleaned.pop("thinking_block", None)
            content = cleaned.get("content")
            if isinstance(content, list):
                cleaned["content"] = [
                    b for b in content
                    if not (isinstance(b, dict)
                            and b.get("type") in ("thinking", "redacted_thinking"))
                ]
            out.append(cleaned)
        return out
```

Call it just before `_convert_messages` runs, using the `effective_model` from `_resolve_effective_model`. This single seam covers **both** trigger paths: internal overload fallback (`effective_model` diverges from requested) and any future refusal-driven cross-model retry.

### 5.4 Empty-thinking round-trip (same-model) — verify, do not regress

The fact: a thinking block may have **empty `thinking` text + a `signature`** and must round-trip **byte-identical on the same model**. The strip rule (§5.3) only fires on *mismatch*, so same-model turns are untouched — correct. The one hazard is any "clean/normalize" step dropping an empty-`thinking` block as if it were vacuous.

**Action:** Audit `_clean_content_block` (used at `:3280`) and any thinking serialization to confirm a `ThinkingBlock` with `thinking == ""` and a non-null `signature` is preserved verbatim (both `thinking` and `signature` fields survive). Add a regression test (§7). No behavioural change expected — this is a guard against silently dropping the signature.

### 5.5 `reasoning_effort` / `output_config.effort` — no logic change, documentation only

The provider already: reads `request.reasoning_effort` (`:2169`), maps `low|medium|high|xhigh|max` to thinking type + budget (`:2242-2258`), and emits `output_config.effort` for output-config-capable models (`:2374-2378`). **No provider code change is required for fable-5 effort.** The gap is purely contractual (see §6.2). Optionally promote `reasoning_effort` to a first-class `ChatRequest` field — see §6.4 (recommended, additive).

### 5.6 Success criteria

- **stop_details:** a mocked Anthropic response with `stop_reason="refusal"`, `stop_details={"category":"reasoning_extraction"}` yields `ChatResponse.finish_reason == "refusal"` and `ChatResponse.metadata["stop_details"] == {"category":"reasoning_extraction"}`.
- **provenance:** a normal completion yields `ChatResponse.metadata["producer_model"] == <served model id>`.
- **strip:** given a message list where an assistant turn has `producer_model="claude-fable-5"` with a thinking block, and `effective_model="claude-sonnet-4-6"`, the converted Anthropic payload contains **no** `thinking`/`redacted_thinking` for that turn but **retains** its `text`/`tool_use`. Given `effective_model="claude-fable-5"` (match), the thinking block is retained byte-identical.
- **empty-thinking:** a same-model `ThinkingBlock(thinking="", signature="abc")` survives conversion with `signature` intact.
- `python_check` clean.

---

## 6. Contract-doc diffs (normative text to add)

### 6.1 `PROVIDER_SPECIFICATION.md` — stop-reason vocabulary + refusal

**File:** `amplifier-core/docs/specs/PROVIDER_SPECIFICATION.md`
**Insert a new section after "Content Preservation (Critical)" (`:79-87`):**

```markdown
### Stop Reasons & Refusals

Providers set `ChatResponse.finish_reason` to the vendor's stop reason. The
orchestrator reads it; the following values are load-bearing:

| finish_reason | Meaning | Orchestrator behaviour |
|---------------|---------|------------------------|
| `end_turn` / `stop` | Normal completion | Store assistant turn, end loop |
| `tool_use` / `tool_calls` | Model requested tools | Execute tools, continue loop |
| `max_tokens` / `length` | Truncated | Provider may auto-continue (see below) |
| `refusal` | **Safety refusal on HTTP 200.** Content is empty/near-empty. | Emit `provider:refusal`, end turn with status `refused`. No assistant turn is stored. |

A **refusal is not an error** — it is a successful HTTP response in which the
model declined. Providers MUST:

1. Set `finish_reason` to the raw stop reason (`"refusal"`).
2. Place any structured detail in `metadata["stop_details"]`
   (e.g. `{"category": "reasoning_extraction"}`).

Retrying a refusal on a different model is **policy** (provider config or a
`provider:refusal` hook), never hardcoded in the orchestrator.
```

**Update the Content Block Reference row for `thinking` (`:192`) and add a round-trip clause. Replace the "Content Preservation" thinking row (`:85`) and reference row (`:192`) guidance with:**

```markdown
| `ThinkingBlock` | Preserve `signature` EXACTLY. A thinking block may have an
EMPTY `thinking` string with a non-null `signature`; it MUST round-trip
byte-identical on the SAME model. On a model SWITCH, strip `thinking` and
`redacted_thinking` from prior assistant turns — signatures are model-bound and
are rejected cross-model. Use `Message.producer_model` to detect the switch. |
```

### 6.2 `PROVIDER_SPECIFICATION.md` — effort / output_config params

**Insert after the new Stop Reasons section:**

```markdown
### Reasoning Effort (depth/cost control)

`ChatRequest.reasoning_effort` is the portable depth/cost control:
`low | medium | high | xhigh | max`. Providers map it to their native surface:

- Anthropic (thinking-capable): enables extended thinking and sizes the budget;
  on output-config-capable models it is sent as `output_config.effort`.
- Providers that do not support graded effort SHOULD treat any non-null value as
  "enable reasoning" and ignore the specific tier.

`reasoning_effort` is the primary knob for `claude-fable-5`-class models
(adaptive thinking is always on; effort governs depth and cost). It is optional;
`None` means "provider default".
```

### 6.3 `PROVIDER_CONTRACT.md` — checklist additions

**File:** `amplifier-core/docs/contracts/PROVIDER_CONTRACT.md`
**Add to "Required" checklist (`:155-159`):**

```markdown
- [ ] Set `finish_reason` from the vendor stop reason (incl. `refusal`)
- [ ] Put `stop_details` in `metadata` on refusals
- [ ] Stamp `metadata["producer_model"]` with the model that served the response
- [ ] Strip model-bound `thinking`/`redacted_thinking` when the target model differs from a turn's `producer_model`
```

### 6.4 `message_models.py` docstrings + optional `ChatRequest.reasoning_effort` promotion

**Recommended (additive):** promote `reasoning_effort` from an undocumented `extra="allow"` field to a first-class optional field on `ChatRequest` (`:172-189`), so it is schema-visible and type-checked (same justification as §1.1 for `producer_model`):

```python
    reasoning_effort: Literal["low", "medium", "high", "xhigh", "max"] | None = Field(
        default=None,
        description="Portable reasoning depth/cost control. Providers map to their "
        "native surface (Anthropic: extended thinking budget / output_config.effort).",
    )
```

Back-compat: additive; the orchestrator already sets `reasoning_effort=...` on `ChatRequest` (`loop-streaming :431`) via `extra="allow"`, so this only *formalises* an existing field — no caller changes. `Literal` is safe here because the value set is provider-portable and small; providers ignore tiers they do not support (§6.2).

### 6.5 Stale token examples — `MOUNT_PLAN_SPECIFICATION.md` + `CONTEXT_CONTRACT.md`

**File:** `amplifier-core/docs/specs/MOUNT_PLAN_SPECIFICATION.md`

| Line | Current | Change to |
|---|---|---|
| `:161` | `"max_tokens": 200000,` | `"max_tokens": 1000000,  # 1M context (fable-5 / sonnet-4.5+)` |
| `:205` | `"max_tokens": 200000,` | `"max_tokens": 1000000,` |
| `:255` | `"max_tokens": 200000,` | `"max_tokens": 1000000,` |
| `:266` | `"max_tokens": 4096` | `"max_tokens": 64000  # provider output cap; see model's max output` |

> Note: `:266`'s `max_tokens` is under a *provider* config (output cap), distinct from the *context* `max_tokens` (window budget) at `:161/205/255`. Keep them semantically distinct — the comment makes that explicit so the two are not conflated again.

Also update the example model ids on `:150, :213, :264` (`"claude-sonnet-4-5"`) only if the doc is meant to showcase current models; otherwise leave (out of scope — not a token-staleness fix).

**File:** `amplifier-core/docs/contracts/CONTEXT_CONTRACT.md`, `:115`

| Current | Change to |
|---|---|
| `max_tokens=config.get("max_tokens", 100000),` | `max_tokens=config.get("max_tokens", 1000000),  # 1M context default for current models` |

### 6.6 Success criteria

- Each contract doc contains the new normative text; internal cross-references resolve.
- No stale `200000` / `4096` / `100000` token example remains in the four cited locations.
- A builder reading `PROVIDER_SPECIFICATION.md` can implement refusal + effort + thinking round-trip with no reference to this spec.

---

## 7. Testing strategy

Incremental, per the ISSUE_HANDLING incremental-testing pattern (unit → interaction → integration).

### 7.1 Unit (per change)
- `message_models`: `producer_model` default/round-trip (§2.3); `reasoning_effort` literal validation (§6.4).
- `events`: `PROVIDER_REFUSAL` present + in `ALL_EVENTS` (§3.4).
- provider: `stop_details` capture, `producer_model` stamping, strip on mismatch / retain on match, empty-thinking survival (§5.6).

### 7.2 Interaction (contract round-trip — the critical seam)
- End-to-end provenance: feed a mock Anthropic response (model `A`) → orchestrator stores assistant message → assert stored `Message.producer_model == "A"` → next request with `effective_model == "B"` → assert thinking stripped; with `effective_model == "A"` → assert retained byte-identical. This pins **both halves** of §5.2 (provider stamps, orchestrator persists) — the seam the MJ Lens flagged.

### 7.3 Integration (orchestrator refusal)
- Stub provider returning `finish_reason="refusal"` → assert single `provider:refusal` event with correct `stop_details`, `execute()` returns `""`, `orchestrator:complete.status == "refused"`, no assistant message appended (§4.5).
- Regression: normal completion path unchanged (no refusal event, `status == "success"`).

### 7.4 Evidence requirements (before "done")
- `python_check` clean on all four repos' touched files.
- Each repo's existing suite green (no regressions).
- The §7.2 round-trip test is the acceptance gate: if provenance does not survive one response→request cycle, the strip rule is inert and the change has failed regardless of unit greens.

---

## 8. Explicit non-goals

1. **Server-side fallback parameters.** No modeling of any Anthropic request-side "fallback models" API param. Cross-model retry stays client-side policy (existing overload-fallback machinery + future `provider:refusal` hook).
2. **Automatic refusal retry-on-another-model in this change.** The mechanism (provenance, strip, event, status) is delivered; the *retry policy* is a follow-up in provider config or a hook. Wiring it now would bake policy into the kernel/orchestrator (violates mechanism/policy split).
3. **Streaming-path rewrite (finding #4 — KNOWN ISSUE, documented not fixed).** `_stream_from_provider` (`loop-streaming :~1018-1029`) accumulates **text only** — it drops thinking blocks and tool calls, and never inspects `finish_reason`. For mandatory-adaptive-thinking models this is a latent landmine (thinking never persisted → provenance never stamped → strip rule cannot protect a streamed turn; a streamed refusal is invisible). **This change does not touch it.** The live path today is the non-streaming branch, so the fix above is complete for current behaviour. Track a separate issue: *"loop-streaming `_stream_from_provider` drops thinking/tool_calls and ignores finish_reason; must reach parity with the non-streaming branch before any provider ships a real `stream()` method for adaptive-thinking models."* Until then, providers used with this orchestrator MUST NOT expose a `stream` method that bypasses `complete()`, or they forfeit refusal handling and provenance.
4. **`finish_reason` as an enum.** Kept as free-form `str` — other providers pass arbitrary vendor values through it. Constraining to `Literal` would be a breaking change for those providers. Vocabulary is documented (§6.1), not enforced by type.
5. **Retrofitting provenance onto historical transcripts.** Old messages have `producer_model = None` and are left unstripped (§5.3) — we never fabricate provenance we did not observe.

---

## 9. Migration & rollout notes

- **Order (dependency-first):** (1) `amplifier-core` (`producer_model`, `PROVIDER_REFUSAL`, `reasoning_effort` field, doc diffs) → (2) `amplifier-module-provider-anthropic` (stop_details, producer_model, strip) → (3) `amplifier-module-loop-streaming` (refusal read, status, provenance copy). Core is additive and backward-compatible, so older modules keep working against new core during rollout.
- **No breaking changes:** every field is optional/defaulted; the new event is additive; the new status value is a new string an existing consumer simply hasn't seen before (observers that switch on status must have a default branch — verify `hooks-logging` does).
- **Bundle cache:** after merging the module PRs, a running Amplifier keeps the old module in memory. Users pick up changes via `amplifier reset --remove cache -y` then restart (per ISSUE_HANDLING "Bundle Cache and Module Loading").
- **Cross-repo tracking:** create the finding-#4 streaming-parity issue before closing this work, and reference all three module PRs from a tracking issue.
- **`~/dev/amplifier-module-loop-streaming` staleness:** do not edit the stale local checkout. Reconcile the §4 edits against upstream `main` first (the cache copy is the behavioural reference, not necessarily identical to `main`).

---

## 10. Change summary (one-screen index for the builder)

| # | Repo | File:loc | Change | Breaking? |
|---|---|---|---|---|
| 2 | amplifier-core | `message_models.py:130` | add `Message.producer_model: str \| None` | No (additive) |
| 3 | amplifier-core | `events.py:22, :73` | add `PROVIDER_REFUSAL` + `ALL_EVENTS` entry | No |
| 6.4 | amplifier-core | `message_models.py:186` | promote `ChatRequest.reasoning_effort` to first-class | No |
| 6.1–6.5 | amplifier-core | `docs/specs/*`, `docs/contracts/*` | normative text + stale-token fixes | Docs |
| 5.1 | provider-anthropic | `__init__.py:3751` | capture `stop_details` → `metadata` | No |
| 5.2a | provider-anthropic | `__init__.py:3751` | stamp `metadata["producer_model"]` | No |
| 5.3 | provider-anthropic | `__init__.py:3178`/`complete()` | strip cross-model thinking | No (behaviour gated on mismatch) |
| 5.4 | provider-anthropic | `_clean_content_block` | verify empty-thinking survives | No |
| 4.1 | loop-streaming | `__init__.py:573` | read `finish_reason`, emit `provider:refusal`, end turn | No |
| 4.2 | loop-streaming | `__init__.py:189` | add `"refused"` status | No |
| 5.2b | loop-streaming | `__init__.py:638,716` | copy `producer_model` onto stored message | No |
| 4.4 | loop-streaming | `__init__.py:47` | register `provider:refusal` for observability | No |

Every row is additive. The two load-bearing bricks are `Message.producer_model` (row 2) and the refusal read (row 4.1); everything else hangs off those.
