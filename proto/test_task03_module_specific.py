"""TDD test for Task 03: Module-specific messages in amplifier_module.proto.

Tests proto compilation and verifies all required types are present.
"""
import subprocess
import re
import sys
import os
import tempfile

PROTO_PATH = os.path.join(os.path.dirname(__file__), "amplifier_module.proto")


def read_proto():
    with open(PROTO_PATH, "r") as f:
        return f.read()


def field_present(body, field_spec):
    """Check if a field like 'bool success' is present, tolerating extra whitespace."""
    # Split field_spec into parts and join with \s+ for flexible matching
    parts = field_spec.split()
    pattern = r'\s+'.join(re.escape(p) for p in parts)
    return bool(re.search(pattern, body))


def test_proto_compiles():
    """Proto must compile with exit code 0."""
    with tempfile.TemporaryDirectory() as tmpdir:
        result = subprocess.run(
            ["protoc", f"--proto_path={os.path.dirname(PROTO_PATH)}",
             f"--python_out={tmpdir}", PROTO_PATH],
            capture_output=True, text=True
        )
        assert result.returncode == 0, f"protoc failed:\n{result.stderr}"


# ---------------------------------------------------------------------------
# Enums
# ---------------------------------------------------------------------------

def test_hook_action_enum():
    """HookAction enum with 6 values: UNSPECIFIED through ASK_USER."""
    content = read_proto()
    assert "enum HookAction" in content, "Missing enum HookAction"
    # Extract enum body
    m = re.search(r'enum HookAction\s*\{([^}]+)\}', content)
    assert m, "Cannot parse HookAction enum body"
    body = m.group(1)
    expected = [
        "HOOK_ACTION_UNSPECIFIED",
        "HOOK_ACTION_CONTINUE",
        "HOOK_ACTION_MODIFY",
        "HOOK_ACTION_SKIP",
        "HOOK_ACTION_BLOCK",
        "HOOK_ACTION_ASK_USER",
    ]
    for val in expected:
        assert val in body, f"HookAction missing value: {val}"


def test_context_injection_role_enum():
    """ContextInjectionRole enum with 4 values."""
    content = read_proto()
    assert "enum ContextInjectionRole" in content, "Missing enum ContextInjectionRole"
    m = re.search(r'enum ContextInjectionRole\s*\{([^}]+)\}', content)
    assert m, "Cannot parse ContextInjectionRole enum body"
    body = m.group(1)
    expected = [
        "CONTEXT_INJECTION_ROLE_UNSPECIFIED",
        "CONTEXT_INJECTION_ROLE_SYSTEM",
        "CONTEXT_INJECTION_ROLE_USER",
        "CONTEXT_INJECTION_ROLE_ASSISTANT",
    ]
    for val in expected:
        assert val in body, f"ContextInjectionRole missing value: {val}"


def test_approval_default_enum():
    """ApprovalDefault enum with 3 values."""
    content = read_proto()
    assert "enum ApprovalDefault" in content, "Missing enum ApprovalDefault"
    m = re.search(r'enum ApprovalDefault\s*\{([^}]+)\}', content)
    assert m, "Cannot parse ApprovalDefault enum body"
    body = m.group(1)
    expected = [
        "APPROVAL_DEFAULT_UNSPECIFIED",
        "APPROVAL_DEFAULT_APPROVE",
        "APPROVAL_DEFAULT_DENY",
    ]
    for val in expected:
        assert val in body, f"ApprovalDefault missing value: {val}"


def test_user_message_level_enum():
    """UserMessageLevel enum with 4 values."""
    content = read_proto()
    assert "enum UserMessageLevel" in content, "Missing enum UserMessageLevel"
    m = re.search(r'enum UserMessageLevel\s*\{([^}]+)\}', content)
    assert m, "Cannot parse UserMessageLevel enum body"
    body = m.group(1)
    expected = [
        "USER_MESSAGE_LEVEL_UNSPECIFIED",
        "USER_MESSAGE_LEVEL_INFO",
        "USER_MESSAGE_LEVEL_WARNING",
        "USER_MESSAGE_LEVEL_ERROR",
    ]
    for val in expected:
        assert val in body, f"UserMessageLevel missing value: {val}"


# ---------------------------------------------------------------------------
# Messages
# ---------------------------------------------------------------------------

def test_tool_result_message():
    """ToolResult message with 3 fields: success, output_json, error_json."""
    content = read_proto()
    assert "message ToolResult" in content, "Missing message ToolResult"
    m = re.search(r'message ToolResult\s*\{([^}]+)\}', content)
    assert m, "Cannot parse ToolResult body"
    body = m.group(1)
    assert field_present(body, "bool success"), "ToolResult missing field: success"
    assert field_present(body, "string output_json"), "ToolResult missing field: output_json"
    assert field_present(body, "string error_json"), "ToolResult missing field: error_json"


def test_hook_result_message_15_fields():
    """HookResult must have all 15 fields."""
    content = read_proto()
    assert "message HookResult" in content, "Missing message HookResult"
    m = re.search(r'message HookResult\s*\{([^}]+)\}', content)
    assert m, "Cannot parse HookResult body"
    body = m.group(1)
    expected_fields = [
        "HookAction action",
        "string data_json",
        "string reason",
        "string context_injection",
        "ContextInjectionRole context_injection_role",
        "bool ephemeral",
        "string approval_prompt",
        "repeated string approval_options",
        "double approval_timeout",
        "ApprovalDefault approval_default",
        "bool suppress_output",
        "string user_message",
        "UserMessageLevel user_message_level",
        "string user_message_source",
        "bool append_to_last_tool_result",
    ]
    for field in expected_fields:
        assert field_present(body, field), f"HookResult missing field: {field}"
    # Verify approval_timeout default is 300.0
    # proto3 doesn't support default values natively; check for a comment
    assert "300" in body, "HookResult: approval_timeout should reference default 300"


def test_model_info_message():
    """ModelInfo message with 6 fields."""
    content = read_proto()
    assert "message ModelInfo" in content, "Missing message ModelInfo"
    m = re.search(r'message ModelInfo\s*\{([^}]+)\}', content)
    assert m, "Cannot parse ModelInfo body"
    body = m.group(1)
    expected_fields = [
        "string id",
        "string display_name",
        "int32 context_window",
        "int32 max_output_tokens",
        "repeated string capabilities",
        "string defaults_json",
    ]
    for field in expected_fields:
        assert field_present(body, field), f"ModelInfo missing field: {field}"


def test_provider_info_message():
    """ProviderInfo message with 6 fields including config_fields."""
    content = read_proto()
    assert "message ProviderInfo" in content, "Missing message ProviderInfo"
    m = re.search(r'message ProviderInfo\s*\{([^}]+)\}', content)
    assert m, "Cannot parse ProviderInfo body"
    body = m.group(1)
    assert "config_fields" in body, "ProviderInfo missing field: config_fields"
    # Count fields (lines with field numbers)
    field_numbers = re.findall(r'=\s*\d+', body)
    assert len(field_numbers) >= 6, f"ProviderInfo should have >= 6 fields, found {len(field_numbers)}"


def test_approval_request_message():
    """ApprovalRequest with 5 fields: tool_name, action, details_json, risk_level, timeout."""
    content = read_proto()
    assert "message ApprovalRequest" in content, "Missing message ApprovalRequest"
    m = re.search(r'message ApprovalRequest\s*\{([^}]+)\}', content)
    assert m, "Cannot parse ApprovalRequest body"
    body = m.group(1)
    expected_fields = [
        "string tool_name",
        "string action",
        "string details_json",
        "string risk_level",
        "double timeout",
    ]
    for field in expected_fields:
        assert field_present(body, field), f"ApprovalRequest missing field: {field}"


def test_approval_response_message():
    """ApprovalResponse with 3 fields: approved, reason, remember."""
    content = read_proto()
    assert "message ApprovalResponse" in content, "Missing message ApprovalResponse"
    m = re.search(r'message ApprovalResponse\s*\{([^}]+)\}', content)
    assert m, "Cannot parse ApprovalResponse body"
    body = m.group(1)
    expected_fields = [
        "bool approved",
        "string reason",
        "bool remember",
    ]
    for field in expected_fields:
        assert field_present(body, field), f"ApprovalResponse missing field: {field}"


if __name__ == "__main__":
    # Run all test functions
    failed = []
    passed = []
    for name, obj in sorted(globals().items()):
        if name.startswith("test_") and callable(obj):
            try:
                obj()
                passed.append(name)
                print(f"  PASS: {name}")
            except AssertionError as e:
                failed.append((name, str(e)))
                print(f"  FAIL: {name} -> {e}")
    print(f"\n{len(passed)} passed, {len(failed)} failed")
    sys.exit(1 if failed else 0)
