"""Tests for CI/CD workflow files (Milestone 8).

Validates that GitHub Actions workflow YAML files:
- Exist at the expected paths
- Are valid YAML
- Contain the required jobs, steps, and configuration
"""

from __future__ import annotations

import pathlib

import yaml

# Root of the amplifier-core submodule
ROOT = pathlib.Path(__file__).resolve().parent.parent


def _normalize_on_key(data: dict) -> dict:
    """PyYAML parses the bare keyword ``on`` as boolean True.

    GitHub Actions uses ``on:`` as a trigger key, so we normalise
    ``True`` → ``"on"`` after loading to keep tests readable.
    """
    if True in data and "on" not in data:
        data["on"] = data.pop(True)
    return data


class TestRustCoreCIWorkflow:
    """Task 8.1: Rust + Python CI workflow."""

    WORKFLOW_PATH = ROOT / ".github" / "workflows" / "rust-core-ci.yml"

    def test_workflow_file_exists(self):
        assert self.WORKFLOW_PATH.exists(), (
            f"CI workflow not found at {self.WORKFLOW_PATH}"
        )

    def _load(self) -> dict:
        return _normalize_on_key(yaml.safe_load(self.WORKFLOW_PATH.read_text()))

    # -- trigger configuration --

    def test_triggers_on_push_to_rust_core(self):
        wf = self._load()
        push_branches = wf["on"]["push"]["branches"]
        assert "rust-core" in push_branches

    def test_triggers_on_pr_to_rust_core_and_main(self):
        wf = self._load()
        pr_branches = wf["on"]["pull_request"]["branches"]
        assert "rust-core" in pr_branches
        assert "main" in pr_branches

    # -- rust-tests job --

    def test_has_rust_tests_job(self):
        wf = self._load()
        assert "rust-tests" in wf["jobs"]

    def test_rust_tests_uses_rust_cache(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("rust-cache" in u for u in uses_list), (
            "rust-tests job must use Swatinem/rust-cache"
        )

    def test_rust_tests_runs_cargo_test(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("cargo test" in r for r in run_cmds)

    def test_rust_tests_runs_cargo_check_workspace(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("cargo check" in r and "--workspace" in r for r in run_cmds)

    def test_rust_tests_runs_clippy_deny_warnings(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("cargo clippy" in r and "-D warnings" in r for r in run_cmds)

    # -- python-tests job --

    def test_has_python_tests_job(self):
        wf = self._load()
        assert "python-tests" in wf["jobs"]

    def test_python_matrix_covers_required_versions(self):
        wf = self._load()
        matrix = wf["jobs"]["python-tests"]["strategy"]["matrix"]
        versions = matrix["python-version"]
        for v in ["3.11", "3.12", "3.13"]:
            assert v in [str(x) for x in versions], f"Python {v} missing from matrix"

    def test_python_tests_uses_rust_cache(self):
        wf = self._load()
        steps = wf["jobs"]["python-tests"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("rust-cache" in u for u in uses_list)

    def test_python_tests_builds_with_maturin(self):
        wf = self._load()
        steps = wf["jobs"]["python-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("maturin" in r for r in run_cmds)

    def test_python_tests_runs_original_tests(self):
        wf = self._load()
        steps = wf["jobs"]["python-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("pytest tests/" in r or "pytest tests" in r for r in run_cmds)

    def test_python_tests_runs_bridge_tests(self):
        wf = self._load()
        steps = wf["jobs"]["python-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("bindings/python/tests" in r for r in run_cmds)


class TestBuildWheelsWorkflow:
    """Task 8.2: Cross-platform wheel build workflow."""

    WORKFLOW_PATH = ROOT / ".github" / "workflows" / "rust-core-wheels.yml"

    def test_workflow_file_exists(self):
        assert self.WORKFLOW_PATH.exists(), (
            f"Wheel workflow not found at {self.WORKFLOW_PATH}"
        )

    def _load(self) -> dict:
        return _normalize_on_key(yaml.safe_load(self.WORKFLOW_PATH.read_text()))

    # -- trigger configuration --

    def test_triggers_on_push_to_rust_core(self):
        wf = self._load()
        push_branches = wf["on"]["push"]["branches"]
        assert "rust-core" in push_branches

    def test_triggers_on_tag(self):
        wf = self._load()
        push_tags = wf["on"]["push"]["tags"]
        assert any("rust-core-v" in str(t) for t in push_tags)

    def test_has_workflow_dispatch(self):
        wf = self._load()
        assert "workflow_dispatch" in wf["on"]

    # -- build jobs --

    def test_has_build_wheels_job(self):
        wf = self._load()
        assert "build-wheels" in wf["jobs"]

    def test_build_wheels_matrix_covers_all_os(self):
        wf = self._load()
        matrix = wf["jobs"]["build-wheels"]["strategy"]["matrix"]
        os_list = matrix["os"]
        assert "ubuntu-latest" in os_list
        assert "macos-latest" in os_list
        assert "windows-latest" in os_list

    def test_build_wheels_uses_maturin_action(self):
        wf = self._load()
        steps = wf["jobs"]["build-wheels"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("maturin-action" in u for u in uses_list)

    def test_build_wheels_uploads_artifacts(self):
        wf = self._load()
        steps = wf["jobs"]["build-wheels"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("upload-artifact" in u for u in uses_list)

    def test_has_linux_aarch64_job(self):
        wf = self._load()
        assert "build-linux-aarch64" in wf["jobs"]

    def test_linux_aarch64_targets_aarch64(self):
        wf = self._load()
        steps = wf["jobs"]["build-linux-aarch64"]["steps"]
        maturin_steps = [s for s in steps if "maturin-action" in s.get("uses", "")]
        assert len(maturin_steps) == 1
        assert maturin_steps[0]["with"]["target"] == "aarch64-unknown-linux-gnu"

    def test_linux_aarch64_uploads_artifacts(self):
        wf = self._load()
        steps = wf["jobs"]["build-linux-aarch64"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("upload-artifact" in u for u in uses_list)
