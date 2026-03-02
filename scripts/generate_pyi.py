#!/usr/bin/env python3
"""Verify all key proto types exist in generated Python stubs.

This script ensures the generated Python gRPC stubs contain all
expected message types from the proto source of truth. Run after
regenerating stubs to confirm nothing was lost.
"""

import sys


def main() -> int:
    try:
        from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2
    except ImportError as e:
        print(f"FAIL: Cannot import generated stubs: {e}")
        return 1

    required_types = [
        "ChatRequest",
        "ChatResponse",
        "Message",
        "ContentBlock",
        "ToolResult",
        "HookResult",
        "ModelInfo",
        "ProviderInfo",
        "ApprovalRequest",
        "ApprovalResponse",
        "Usage",
        "ModuleInfo",
        "MountRequest",
        "MountResponse",
    ]

    missing = []
    for name in required_types:
        if not hasattr(pb2, name):
            missing.append(name)
            print(f"  MISSING: {name}")
        else:
            print(f"  OK: {name}")

    if missing:
        print(f"\nFAIL: {len(missing)} types missing from generated stubs")
        return 1

    print(f"\nPASS: All {len(required_types)} proto types verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
