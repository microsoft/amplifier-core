"""Correlation identity for LLM call events (infrastructure-owned).

`llm:request` and `llm:response` historically shared **no** field that
identified the call they belonged to. Consumers were forced to pair them
*positionally* (FIFO over the event stream), which silently mis-attributes
every time a second LLM call is in flight concurrently -- a background
summarizer, a session-naming hook, a forked sub-agent. The mis-pairing is
invisible: both events parse, the counts look plausible, and the resulting
per-caller cost attribution is simply wrong.

This module defines the correlation policy the kernel applies on the emit
path so that **every** provider gets correct pairing without changing a line
of provider code:

* `llm:request` carries a generated ``request_id``.
* The terminal event of the same call (`llm:response`, or `provider:error`
  when the call fails or times out) echoes that exact value.
* `provider:retry` / `provider:throttle` echo it too, without ending the call.

Scoping is by :mod:`contextvars`, so the id follows the *async task* that
issued the call. Two concurrent calls live in two tasks (``asyncio.gather``,
``create_task``, a forked session, a thread) and therefore hold two
independent slots -- which is exactly the case positional pairing gets wrong.

Backward compatibility
----------------------
The field is **additive**. Consumers that ignore ``request_id`` see an
otherwise identical payload. Event streams captured before this change carry
no ``request_id`` at all, so consumers must treat it as *optional*
(``data.get("request_id")``) and keep whatever pairing heuristic they used
before as the fallback. A provider that supplies its own ``request_id``
always wins -- the kernel never overwrites an explicit value.
"""

from __future__ import annotations

import uuid
from contextvars import ContextVar

__all__ = [
    "REQUEST_ID_FIELD",
    "REQUEST_EVENTS",
    "TERMINAL_EVENTS",
    "INTERIM_EVENTS",
    "new_request_id",
    "current_request_id",
    "resolve_request_id",
    "reset_request_id",
]

#: Name of the correlation field stamped onto event data.
REQUEST_ID_FIELD = "request_id"

#: Events that *open* a call and generate the correlation id.
REQUEST_EVENTS = frozenset({"llm:request"})

#: Events that *close* a call. They echo the id, then end the call so a later
#: unrelated event in the same task cannot inherit a stale id.
TERMINAL_EVENTS = frozenset({"llm:response", "provider:error"})

#: Events that echo the id of an in-flight call without ending it.
INTERIM_EVENTS = frozenset({"provider:retry", "provider:throttle"})

# (request_id, in_flight). `in_flight` is True between the request event and
# the terminal event of the same call. A closed slot is never read again --
# absence of a correlation id is always preferable to a wrong one.
_CALL: ContextVar[tuple[str, bool] | None] = ContextVar(
    "amplifier_core_llm_call", default=None
)


def new_request_id() -> str:
    """Generate a fresh correlation id.

    Client-generated (uuid4) on purpose: a provider-assigned id only exists
    *after* the response comes back, which is far too late to stamp onto the
    request -- and is absent entirely when the call times out.
    """
    return str(uuid.uuid4())


def current_request_id() -> str | None:
    """Correlation id of the in-flight LLM call in this context, if any.

    Returns ``None`` when no call is in flight (including after the call's
    terminal event). Useful for module authors who want to tag their own
    logs or custom events with the enclosing call.
    """
    call = _CALL.get()
    if call is None or not call[1]:
        return None
    return call[0]


def reset_request_id() -> None:
    """Clear the correlation slot for this context (test/teardown helper)."""
    _CALL.set(None)


def resolve_request_id(event: str, explicit: str | None = None) -> str | None:
    """Return the correlation id to stamp on ``event``, or ``None``.

    This is the whole policy, and it is deliberately the *only* place that
    decides. The kernel emit path calls it for every event in the ``llm:``
    and ``provider:`` families; this function is authoritative about which
    of those actually carry a correlation id.

    Args:
        event: Event name being emitted.
        explicit: A ``request_id`` the caller already put in the event data.
            An explicit value always wins and is adopted as the id of the
            call in flight.

    Returns:
        The id to stamp, or ``None`` when this event carries no correlation
        id (unknown event, or a response with no matching request in this
        context).
    """
    if event in REQUEST_EVENTS:
        if explicit:
            _CALL.set((explicit, True))
            return explicit
        request_id = new_request_id()
        _CALL.set((request_id, True))
        return request_id

    if event in TERMINAL_EVENTS:
        if explicit:
            _CALL.set((explicit, False))
            return explicit
        request_id = current_request_id()
        if request_id is None:
            return None
        _CALL.set((request_id, False))
        return request_id

    if event in INTERIM_EVENTS:
        if explicit:
            _CALL.set((explicit, True))
            return explicit
        return current_request_id()

    return None
