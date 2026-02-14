"""LLM provider error taxonomy.

Provides a shared vocabulary for LLM provider errors that enables
cross-provider error handling in hooks, orchestrators, and applications.

Providers translate their native SDK errors into these types so that
downstream code can catch "rate limit" or "auth failure" without
provider-specific knowledge.

Design principles:
- Mechanism, not policy: the kernel defines the vocabulary; modules
  decide what to do with it (retry, fallback, deny, log).
- Incremental adoption: providers that don't translate errors continue
  to raise native exceptions. Existing ``except Exception`` catches
  still work.
- Chain preservation: providers use ``raise X(...) from native_error``
  so the original exception is available via ``__cause__``.
"""

from __future__ import annotations


class LLMError(Exception):
    """Base for all LLM provider errors.

    Attributes:
        provider: Name of the provider that raised the error (e.g. "anthropic").
        status_code: HTTP status code from the provider, if available.
        retryable: Whether the caller should consider retrying the request.
    """

    def __init__(
        self,
        message: str,
        *,
        provider: str | None = None,
        status_code: int | None = None,
        retryable: bool = False,
    ) -> None:
        super().__init__(message)
        self.provider = provider
        self.status_code = status_code
        self.retryable = retryable

    def __repr__(self) -> str:
        parts = [repr(str(self))]
        if self.provider is not None:
            parts.append(f"provider={self.provider!r}")
        if self.status_code is not None:
            parts.append(f"status_code={self.status_code!r}")
        if self.retryable:
            parts.append("retryable=True")
        return f"{type(self).__name__}({', '.join(parts)})"


class RateLimitError(LLMError):
    """Provider rate limit exceeded (HTTP 429 or equivalent).

    Attributes:
        retry_after: Seconds to wait before retrying, parsed from the
            provider's ``Retry-After`` header when available.
    """

    def __init__(
        self,
        message: str,
        *,
        retry_after: float | None = None,
        provider: str | None = None,
        status_code: int | None = None,
        retryable: bool = True,
    ) -> None:
        super().__init__(
            message,
            provider=provider,
            status_code=status_code,
            retryable=retryable,
        )
        self.retry_after = retry_after


class AuthenticationError(LLMError):
    """Invalid or missing API credentials (HTTP 401/403)."""

    pass


class ContextLengthError(LLMError):
    """Request exceeds the model's context window (HTTP 413 or provider-specific)."""

    pass


class ContentFilterError(LLMError):
    """Content blocked by the provider's safety filter."""

    pass


class InvalidRequestError(LLMError):
    """Malformed request rejected by the provider (HTTP 400/422)."""

    pass


class ProviderUnavailableError(LLMError):
    """Provider service unavailable (HTTP 5xx, network error, DNS failure).

    Retryable by default — the provider may recover.
    """

    def __init__(
        self,
        message: str,
        *,
        provider: str | None = None,
        status_code: int | None = None,
        retryable: bool = True,
    ) -> None:
        super().__init__(
            message,
            provider=provider,
            status_code=status_code,
            retryable=retryable,
        )


class LLMTimeoutError(LLMError):
    """Request timed out before the provider responded.

    Retryable by default — timeouts are often transient.
    """

    def __init__(
        self,
        message: str,
        *,
        provider: str | None = None,
        status_code: int | None = None,
        retryable: bool = True,
    ) -> None:
        super().__init__(
            message,
            provider=provider,
            status_code=status_code,
            retryable=retryable,
        )
