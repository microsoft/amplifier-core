"""Proto-based structural validation for module results.

Validates dicts against constraints derived from the proto schema.
These functions check field presence, types, and allowed values
as defined by the proto source of truth.
"""

from __future__ import annotations

# Valid hook actions from proto HookAction enum
VALID_HOOK_ACTIONS = frozenset(
    {"continue", "deny", "modify", "inject_context", "ask_user"}
)

# Valid context injection roles from proto ContextInjectionRole enum
VALID_CONTEXT_INJECTION_ROLES = frozenset({"system", "user", "assistant"})

# Valid approval defaults from proto ApprovalDefault enum
VALID_APPROVAL_DEFAULTS = frozenset({"allow", "deny"})

# Valid user message levels from proto UserMessageLevel enum
VALID_USER_MESSAGE_LEVELS = frozenset({"info", "warning", "error"})


def validate_tool_result(data: dict) -> list[str]:
    """Validate a tool result dict against proto-derived constraints.

    Args:
        data: Dict to validate (typically from a tool execution).

    Returns:
        List of error strings. Empty list means valid.
    """
    errors: list[str] = []

    if "success" not in data:
        errors.append("Missing required field: 'success'")
    elif not isinstance(data["success"], bool):
        errors.append(
            f"Field 'success' must be bool, got {type(data['success']).__name__}"
        )

    return errors


def validate_hook_result(data: dict) -> list[str]:
    """Validate a hook result dict against proto-derived constraints.

    Args:
        data: Dict to validate (typically from a hook handler).

    Returns:
        List of error strings. Empty list means valid.
    """
    errors: list[str] = []

    action = data.get("action", "continue")  # proto default is CONTINUE
    if action not in VALID_HOOK_ACTIONS:
        errors.append(
            f"Invalid action '{action}': must be one of {sorted(VALID_HOOK_ACTIONS)}"
        )

    role = data.get("context_injection_role")
    if role is not None and role not in VALID_CONTEXT_INJECTION_ROLES:
        errors.append(
            f"Invalid context_injection_role '{role}': "
            f"must be one of {sorted(VALID_CONTEXT_INJECTION_ROLES)}"
        )

    level = data.get("user_message_level")
    if level is not None and level not in VALID_USER_MESSAGE_LEVELS:
        errors.append(
            f"Invalid user_message_level '{level}': "
            f"must be one of {sorted(VALID_USER_MESSAGE_LEVELS)}"
        )

    approval_default = data.get("approval_default")
    if approval_default is not None and approval_default not in VALID_APPROVAL_DEFAULTS:
        errors.append(
            f"Invalid approval_default '{approval_default}': "
            f"must be one of {sorted(VALID_APPROVAL_DEFAULTS)}"
        )

    return errors
