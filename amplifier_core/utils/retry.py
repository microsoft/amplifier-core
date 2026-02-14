"""Shared retry utilities for LLM provider operations.

Provides:
- RetryConfig: Configuration dataclass for retry behavior.
- retry_with_backoff: Async retry loop with exponential backoff.
- classify_error_message: Heuristic error classifier for provider error strings.

These are mechanism, not policy. Providers and modules decide when
and how to use them.
"""

from __future__ import annotations

import asyncio
import random
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import TypeVar

from amplifier_core.llm_errors import (
    AccessDeniedError,
    AuthenticationError,
    ContentFilterError,
    ContextLengthError,
    InvalidRequestError,
    LLMError,
    NotFoundError,
    ProviderUnavailableError,
    RateLimitError,
)

T = TypeVar("T")


@dataclass
class RetryConfig:
    """Configuration for retry behavior.

    Follows exponential backoff with jitter. Respects
    ``RateLimitError.retry_after`` when ``honor_retry_after`` is True.
    Only retries errors where ``LLMError.retryable`` is True.
    """

    max_retries: int = 3
    """Maximum retry attempts. 0 means no retries (single attempt). Total calls = max_retries + 1."""

    min_delay: float = 1.0
    """Initial delay in seconds before the first retry."""

    max_delay: float = 60.0
    """Maximum delay between retries in seconds."""

    jitter: float = 0.2
    """Jitter factor (0.0-1.0). Applied as +/- jitter * delay."""

    backoff_multiplier: float = 2.0
    """Exponential backoff factor. Delay = min_delay * (multiplier ^ attempt)."""

    honor_retry_after: bool = True
    """If True, use max(calculated_delay, retry_after) for RateLimitError."""


async def retry_with_backoff(
    operation: Callable[..., Awaitable[T]],
    config: RetryConfig | None = None,
    *,
    on_retry: Callable[[int, float, LLMError], Awaitable[None]] | None = None,
) -> T:
    """Execute an async operation with retry on retryable LLMErrors.

    Args:
        operation: Async callable to execute (no args -- use functools.partial
            or lambda to bind arguments).
        config: Retry configuration. Uses defaults if None.
        on_retry: Optional async callback called before each retry sleep with
            (attempt, delay, error). Use for event emission, logging, etc.

    Returns:
        The result of a successful operation call.

    Raises:
        LLMError: The final error after all retries exhausted, or a
            non-retryable error immediately.
        Exception: Any non-LLMError exception from the operation (no retry).
    """
    if config is None:
        config = RetryConfig()

    last_error: LLMError | None = None

    for attempt in range(config.max_retries + 1):
        try:
            return await operation()
        except LLMError as e:
            last_error = e

            # Non-retryable: raise immediately
            if not e.retryable:
                raise

            # Out of retries: raise
            if attempt >= config.max_retries:
                raise

            # Calculate delay: min_delay * multiplier^attempt, capped at max_delay
            delay = config.min_delay * (config.backoff_multiplier**attempt)
            delay = min(delay, config.max_delay)

            # Respect retry_after from RateLimitError
            if (
                config.honor_retry_after
                and isinstance(e, RateLimitError)
                and e.retry_after is not None
            ):
                delay = max(delay, e.retry_after)

            # Apply jitter: delay * (1 +/- jitter)
            if config.jitter > 0:
                jitter_range = delay * config.jitter
                delay += random.uniform(-jitter_range, jitter_range)  # noqa: S311
                delay = max(0.0, delay)  # Never negative

            # Notify callback (attempt is 0-indexed, report as 1-indexed)
            if on_retry is not None:
                await on_retry(attempt + 1, delay, e)

            await asyncio.sleep(delay)

    # Unreachable, but satisfies type checker
    assert last_error is not None  # noqa: S101
    raise last_error


def classify_error_message(
    message: str,
    *,
    status_code: int | None = None,
    provider: str | None = None,
) -> type[LLMError]:
    """Classify an error message string into the most specific LLMError subclass.

    This centralizes the string-matching heuristics that all providers duplicate.
    Providers can use this as a fallback when they can't determine the error type
    from the SDK's native exception type.

    Status code takes priority when available (except 400, which is ambiguous
    and falls through to message-based classification).

    Args:
        message: The error message to classify.
        status_code: HTTP status code, if available.
        provider: Provider name for context (unused in classification, reserved).

    Returns:
        The most specific LLMError subclass matching the error.
    """
    # Status code takes priority for unambiguous codes
    if status_code is not None:
        if status_code == 401:
            return AuthenticationError
        if status_code == 403:
            return AccessDeniedError
        if status_code == 404:
            return NotFoundError
        if status_code == 413:
            return ContextLengthError
        if status_code == 429:
            return RateLimitError
        if status_code >= 500:
            return ProviderUnavailableError
        # 400/422 are ambiguous -- fall through to message classification

    # Message-based classification (lowercased)
    msg = message.lower()

    # Order matters: more specific patterns first
    if "context length" in msg or "too many tokens" in msg or "maximum context" in msg:
        return ContextLengthError

    if "rate limit" in msg or "too many requests" in msg:
        return RateLimitError

    if "authentication" in msg or "api key" in msg or "unauthorized" in msg:
        return AuthenticationError

    if "not found" in msg:
        return NotFoundError

    if "content filter" in msg or "safety" in msg or "blocked" in msg:
        return ContentFilterError

    # 400/422 with no specific message match -> InvalidRequestError
    if status_code is not None and status_code in (400, 422):
        return InvalidRequestError

    return LLMError
