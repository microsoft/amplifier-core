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


# ---- New error types (Phase 3, purely additive) ----


class NotFoundError(LLMError):
    """Model or endpoint not found (HTTP 404).

    Non-retryable: the resource doesn't exist, retrying won't help.

    Examples:
        - Model ID doesn't exist: "gpt-99" is not a valid model
        - Endpoint not found: wrong base_url configuration
        - Deployment not found: Azure OpenAI deployment deleted
    """

    pass


class StreamError(LLMError):
    """Connection dropped or corrupted during streaming.

    Retryable by default: stream interruptions are often transient
    (network blip, load balancer timeout, server-side reset).

    Distinct from ProviderUnavailableError because the initial connection
    succeeded -- the failure happened mid-stream.
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


class AbortError(LLMError):
    """Caller-initiated cancellation of an LLM request.

    Non-retryable by default: the caller explicitly requested cancellation.
    This is not a failure -- it's cooperative cancellation via CancellationToken
    or abort signal.
    """

    pass


class InvalidToolCallError(LLMError):
    """Model produced a malformed tool call.

    Non-retryable by default: the model generated invalid JSON arguments
    or referenced a tool that doesn't exist. Retrying the same prompt will
    likely produce the same malformed output.

    Attributes:
        tool_name: Name of the tool the model tried to call.
        raw_arguments: The raw argument string before parsing failed.
    """

    def __init__(
        self,
        message: str,
        *,
        tool_name: str | None = None,
        raw_arguments: str | None = None,
        provider: str | None = None,
        status_code: int | None = None,
        retryable: bool = False,
    ) -> None:
        super().__init__(
            message,
            provider=provider,
            status_code=status_code,
            retryable=retryable,
        )
        self.tool_name = tool_name
        self.raw_arguments = raw_arguments


class ConfigurationError(LLMError):
    """Misconfigured provider or SDK setup.

    Non-retryable: configuration problems require human intervention.

    Examples:
        - Missing API key
        - Invalid base_url
        - Unsupported model/provider combination
        - Missing required provider options
    """

    pass


class AccessDeniedError(AuthenticationError):
    """Permission denied (HTTP 403).

    Distinct from AuthenticationError (401) -- credentials are valid but
    lack sufficient permissions for the requested operation.

    Backward compatible: ``except AuthenticationError:`` still catches this.
    """

    pass


class NetworkError(ProviderUnavailableError):
    """Connection-level network failure.

    Retryable by default (inherits from ProviderUnavailableError).

    Distinct from ProviderUnavailableError (which covers HTTP 5xx responses)
    because no HTTP response was received at all -- the connection failed.

    Examples:
        - DNS resolution failure
        - TCP connection refused
        - TLS handshake failure
        - Connection reset by peer

    Backward compatible: ``except ProviderUnavailableError:`` still catches this.
    """

    pass


class QuotaExceededError(RateLimitError):
    """Billing or usage quota exhausted.

    Non-retryable by default (unlike parent RateLimitError which IS retryable).
    Quota exhaustion means the account has hit a hard spending or usage limit,
    not a transient rate limit that clears after a delay.

    Backward compatible: ``except RateLimitError:`` still catches this.
    """

    def __init__(
        self,
        message: str,
        *,
        retry_after: float | None = None,
        provider: str | None = None,
        status_code: int | None = None,
        retryable: bool = False,
    ) -> None:
        super().__init__(
            message,
            retry_after=retry_after,
            provider=provider,
            status_code=status_code,
            retryable=retryable,
        )
