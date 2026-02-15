"""Observability utilities for truncating and redacting data structures."""

from typing import Any

# Known sensitive key patterns (mechanism, not exhaustive policy)
SENSITIVE_KEYS = frozenset(
    {
        "api_key",
        "apikey",
        "api-key",
        "secret",
        "password",
        "token",
        "credential",
        "credentials",
        "private_key",
        "privatekey",
        "auth",
        "authorization",
    }
)


def truncate_values(obj: Any, max_length: int = 180) -> Any:
    """Recursively truncate string values in nested structures.

    Preserves structure, only truncates leaf string values longer than max_length.

    Args:
        obj: Any nested dict/list/value structure
        max_length: Maximum string length before truncation (default 180)

    Returns:
        Copy of structure with long strings truncated

    Examples:
        >>> truncate_values("short")
        'short'
        >>> truncate_values("x" * 200, max_length=10)
        'xxxxxxxxxx... (truncated 190 chars)'
        >>> truncate_values({"key": "x" * 200}, max_length=10)
        {'key': 'xxxxxxxxxx... (truncated 190 chars)'}
    """
    if isinstance(obj, dict):
        return {k: truncate_values(v, max_length) for k, v in obj.items()}
    elif isinstance(obj, list):
        return [truncate_values(item, max_length) for item in obj]
    elif isinstance(obj, str):
        if len(obj) > max_length:
            truncated_chars = len(obj) - max_length
            return f"{obj[:max_length]}... (truncated {truncated_chars} chars)"
        return obj
    else:
        # Pass through other types (int, bool, None, float, etc.)
        return obj


def redact_secrets(obj: Any, sensitive_keys: frozenset[str] = SENSITIVE_KEYS) -> Any:
    """Redact known sensitive keys from nested structures.

    This is a MECHANISM (always-on safety). Policy-level redaction
    (custom patterns) lives in hooks-redaction module.

    Args:
        obj: Any nested dict/list/value structure
        sensitive_keys: Set of lowercase key names to redact

    Returns:
        Copy of structure with sensitive values replaced by "[REDACTED]"

    Examples:
        >>> redact_secrets({"api_key": "secret123"})
        {'api_key': '[REDACTED]'}
        >>> redact_secrets({"user": "alice", "password": "hunter2"})
        {'user': 'alice', 'password': '[REDACTED]'}
        >>> redact_secrets([{"token": "abc"}])
        [{'token': '[REDACTED]'}]
    """
    if isinstance(obj, dict):
        result = {}
        for key, value in obj.items():
            if isinstance(key, str) and key.lower() in sensitive_keys:
                result[key] = "[REDACTED]"
            else:
                result[key] = redact_secrets(value, sensitive_keys)
        return result
    elif isinstance(obj, list):
        return [redact_secrets(item, sensitive_keys) for item in obj]
    else:
        # Pass through all other types unchanged
        return obj
