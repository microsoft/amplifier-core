"""
ID generation helpers for correlation.
"""

import uuid


def new_session_id() -> str:
    return f"s-{uuid.uuid4().hex[:12]}"


def new_request_id() -> str:
    return f"r-{uuid.uuid4().hex[:12]}"


def new_span_id() -> str:
    return f"sp-{uuid.uuid4().hex[:8]}"
