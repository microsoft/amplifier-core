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

    def test_rust_tests_runs_cargo_check(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("cargo check" in r and "amplifier-core" in r for r in run_cmds)

    def test_rust_tests_runs_cargo_fmt_check(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("cargo fmt" in r and "--check" in r for r in run_cmds)

    def test_rust_tests_fmt_before_clippy(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        fmt_idx = next(i for i, r in enumerate(run_cmds) if "cargo fmt" in r)
        clippy_idx = next(i for i, r in enumerate(run_cmds) if "cargo clippy" in r)
        assert fmt_idx < clippy_idx, "cargo fmt --check must run before clippy"

    def test_rust_toolchain_includes_rustfmt(self):
        wf = self._load()
        steps = wf["jobs"]["rust-tests"]["steps"]
        toolchain_steps = [s for s in steps if "rust-toolchain" in s.get("uses", "")]
        assert len(toolchain_steps) == 1
        components = toolchain_steps[0]["with"]["components"]
        assert "rustfmt" in components

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

    def test_does_not_trigger_on_branch_push(self):
        """Wheel builds should only run for release tags, not every branch push.

        Branch-push triggers were removed in the CI optimisation pass to avoid
        burning minutes on the 40-minute Rust+wasmtime build on every commit.
        The test CI (rust-core-ci.yml) still runs on every branch push.
        """
        wf = self._load()
        push_config = wf["on"].get("push", {})
        # 'branches' key must be absent — only 'tags' is allowed under push
        assert "branches" not in push_config, (
            "Wheel workflow must NOT trigger on branch pushes — "
            "only on 'v*' tags and workflow_dispatch"
        )

    def test_triggers_on_tag(self):
        wf = self._load()
        push_tags = wf["on"]["push"]["tags"]
        assert any("v" in str(t) for t in push_tags)

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
        includes = matrix["include"]
        os_list = {entry["os"] for entry in includes}
        assert "ubuntu-24.04" in os_list
        assert "macos-15" in os_list
        assert "windows-2025" in os_list
        assert all("artifact" in entry and "target" in entry for entry in includes)

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

    # -- publish job --

    def test_has_publish_job(self):
        wf = self._load()
        assert "publish" in wf["jobs"]

    def test_release_evidence_needs_all_build_jobs_before_publish(self):
        wf = self._load()
        needs = wf["jobs"]["release-evidence"]["needs"]
        assert "build-wheels" in needs
        assert "build-linux-aarch64" in needs
        assert "build-macos-x86_64" in needs
        assert "build-windows-arm64" in needs
        assert "release-evidence" in wf["jobs"]["publish"]["needs"]

    def test_publish_only_on_tag(self):
        wf = self._load()
        condition = wf["jobs"]["publish"]["if"]
        assert "refs/tags/v" in condition

    def test_publish_uses_pypi_action(self):
        wf = self._load()
        steps = wf["jobs"]["publish"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("pypi-publish" in u for u in uses_list)

    def test_qualification_matrix_has_all_native_python_combinations(self):
        wf = self._load()
        includes = wf["jobs"]["qualify-wheels"]["strategy"]["matrix"]["include"]
        combinations = {(item["artifact"], str(item["python-version"])) for item in includes}
        artifacts = {
            "wheels-ubuntu-latest", "wheels-linux-aarch64", "wheels-macos-latest",
            "wheels-macos-x86_64", "wheels-windows-latest", "wheels-windows-arm64",
        }
        assert len(includes) == 18
        assert combinations == {(artifact, version) for artifact in artifacts for version in ("3.11", "3.12", "3.13")}

    def test_release_evidence_preserves_unmerged_six_and_eighteen_artifacts(self):
        wf = self._load()
        job = wf["jobs"]["release-evidence"]
        assert "qualify-wheels" in job["needs"]
        downloads = [step for step in job["steps"] if "download-artifact" in step.get("uses", "")]
        assert [step["with"]["pattern"] for step in downloads] == ["wheels-*", "qualification-*"]
        assert all("merge-multiple" not in step["with"] for step in downloads)
        run = next(step["run"] for step in job["steps"] if step.get("name") == "Validate evidence and create archive")
        assert "six build targets" in run
        assert "18 required cells" in run
        assert 'receipt.get("source_sha") != source_sha' in run
        assert 'receipt.get("wheel", {}).get("sha256") != wheel_sha' in run
        assert 'report.get("source_sha") != source_sha' in run
        assert 'report.get("passed") is not True' in run

    def test_tag_publication_is_gated_on_evidence_and_pypi(self):
        wf = self._load()
        assert "release-evidence" in wf["jobs"]["publish"]["needs"]
        assert "qualify-wheels" in wf["jobs"]["publish"]["needs"]
        assert "refs/tags/v" in wf["jobs"]["release-evidence"]["steps"][-1]["if"]
        final = wf["jobs"]["publish-release"]
        assert "publish" in final["needs"]
        assert "release-evidence" in final["needs"]
        assert "refs/tags/v" in final["if"]


class TestNodeBindingsCIWorkflow:
    """Node.js binding tests in CI workflow."""

    WORKFLOW_PATH = ROOT / ".github" / "workflows" / "rust-core-ci.yml"

    def _load(self) -> dict:
        return _normalize_on_key(yaml.safe_load(self.WORKFLOW_PATH.read_text()))

    def test_has_node_tests_job(self):
        wf = self._load()
        assert "node-tests" in wf["jobs"]

    def test_node_tests_uses_setup_node(self):
        wf = self._load()
        steps = wf["jobs"]["node-tests"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("setup-node" in u for u in uses_list)

    def test_node_tests_uses_rust_cache(self):
        wf = self._load()
        steps = wf["jobs"]["node-tests"]["steps"]
        uses_list = [s.get("uses", "") for s in steps]
        assert any("rust-cache" in u for u in uses_list)

    def test_node_tests_runs_npm_build(self):
        wf = self._load()
        steps = wf["jobs"]["node-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("npm" in r and "build" in r for r in run_cmds)

    def test_node_tests_runs_vitest(self):
        wf = self._load()
        steps = wf["jobs"]["node-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("vitest" in r for r in run_cmds)

    def test_node_tests_runs_clippy_for_node_binding(self):
        wf = self._load()
        steps = wf["jobs"]["node-tests"]["steps"]
        run_cmds = [s.get("run", "") for s in steps]
        assert any("cargo clippy" in r and "amplifier-core-node" in r for r in run_cmds)
