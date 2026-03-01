"""Tests for ModuleLifecycle service definition in amplifier_module.proto.

Validates that ModuleLifecycle is defined with 4 RPCs: Mount, Cleanup,
HealthCheck, GetModuleInfo. Every gRPC module implements this service.
"""

import re
import subprocess
import sys
from pathlib import Path

import pytest

PROTO_PATH = Path(__file__).parent.parent / "proto" / "amplifier_module.proto"


@pytest.fixture(scope="module")
def proto_text() -> str:
    """Read proto file once per test module."""
    return PROTO_PATH.read_text()


@pytest.fixture(scope="module")
def lifecycle_service_body(proto_text: str) -> str:
    """Extract the ModuleLifecycle block body for scoped RPC matching."""
    match = re.search(r"service ModuleLifecycle\s*\{(.*?)\}", proto_text, re.DOTALL)
    if not match:
        raise ValueError("ModuleLifecycle block not found")
    return match.group(1)


def _compile_proto() -> subprocess.CompletedProcess[str]:
    """Compile the proto file using protoc and return the result."""
    proto_dir = PROTO_PATH.parent
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            f"--proto_path={proto_dir}",
            f"--python_out={proto_dir}",
            f"--grpc_python_out={proto_dir}",
            str(PROTO_PATH.name),
        ],
        capture_output=True,
        text=True,
        cwd=str(proto_dir),
    )
    return result


class TestProtoStillCompiles:
    def test_proto_compiles_with_exit_code_0(self):
        result = _compile_proto()
        assert result.returncode == 0, (
            f"Proto compilation failed (exit {result.returncode}):\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )


class TestModuleLifecycleExists:
    """ModuleLifecycle must be defined after KernelService."""

    def test_module_lifecycle_service_exists(self, proto_text: str):
        assert "service ModuleLifecycle" in proto_text

    def test_module_lifecycle_has_4_rpcs(self, lifecycle_service_body: str):
        rpcs = re.findall(r"rpc\s+\w+", lifecycle_service_body)
        assert len(rpcs) == 4, f"Expected 4 RPCs, found {len(rpcs)}: {rpcs}"

    def test_module_lifecycle_after_kernel_service(self, proto_text: str):
        kernel_pos = proto_text.find("service KernelService")
        lifecycle_pos = proto_text.find("service ModuleLifecycle")
        assert kernel_pos >= 0, "KernelService not found"
        assert lifecycle_pos >= 0, "ModuleLifecycle not found"
        assert lifecycle_pos > kernel_pos, (
            "ModuleLifecycle should appear after KernelService"
        )


class TestModuleLifecycleRPCs:
    """Each of the 4 RPCs with correct signatures."""

    def test_mount_rpc(self, lifecycle_service_body: str):
        assert re.search(
            r"rpc\s+Mount\s*\(\s*MountRequest\s*\)\s+returns\s*\(\s*MountResponse\s*\)",
            lifecycle_service_body,
        )

    def test_cleanup_rpc(self, lifecycle_service_body: str):
        assert re.search(
            r"rpc\s+Cleanup\s*\(\s*Empty\s*\)\s+returns\s*\(\s*Empty\s*\)",
            lifecycle_service_body,
        )

    def test_health_check_rpc(self, lifecycle_service_body: str):
        assert re.search(
            r"rpc\s+HealthCheck\s*\(\s*Empty\s*\)\s+returns\s*\(\s*HealthCheckResponse\s*\)",
            lifecycle_service_body,
        )

    def test_get_module_info_rpc(self, lifecycle_service_body: str):
        assert re.search(
            r"rpc\s+GetModuleInfo\s*\(\s*Empty\s*\)\s+returns\s*\(\s*ModuleInfo\s*\)",
            lifecycle_service_body,
        )


class TestAllEightServicesPresent:
    """All 8 services must be present in the proto file."""

    @pytest.mark.parametrize(
        "service_name",
        [
            "ToolService",
            "ProviderService",
            "OrchestratorService",
            "ContextService",
            "HookService",
            "ApprovalService",
            "KernelService",
            "ModuleLifecycle",
        ],
    )
    def test_service_exists(self, proto_text: str, service_name: str):
        assert f"service {service_name}" in proto_text, (
            f"Service {service_name} not found in proto"
        )

    def test_exactly_8_services(self, proto_text: str):
        services = re.findall(r"service\s+(\w+)\s*\{", proto_text)
        assert len(services) == 8, (
            f"Expected 8 services, found {len(services)}: {services}"
        )


class TestExistingServicesUnchanged:
    """Existing services must remain intact after adding ModuleLifecycle."""

    def test_tool_service_still_exists(self, proto_text: str):
        assert "service ToolService" in proto_text

    def test_provider_service_still_exists(self, proto_text: str):
        assert "service ProviderService" in proto_text

    def test_orchestrator_service_still_exists(self, proto_text: str):
        assert "service OrchestratorService" in proto_text

    def test_context_service_still_exists(self, proto_text: str):
        assert "service ContextService" in proto_text

    def test_hook_service_still_exists(self, proto_text: str):
        assert "service HookService" in proto_text

    def test_approval_service_still_exists(self, proto_text: str):
        assert "service ApprovalService" in proto_text

    def test_kernel_service_still_exists(self, proto_text: str):
        assert "service KernelService" in proto_text
