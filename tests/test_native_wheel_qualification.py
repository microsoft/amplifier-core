"""Unit tests for the dependency-free native wheel qualification helpers."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import tempfile
import types
import unittest
import zipfile
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts" / "qualify_native_wheel.py"
SPEC = importlib.util.spec_from_file_location("qualify_native_wheel", SCRIPT)
assert SPEC and SPEC.loader
qualify = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(qualify)


def cargo_lock(pyo3_version: str = "0.29.2") -> str:
    """Return a minimal coherent Cargo.lock package table."""
    return f"""
version = 4

[[package]]
name = "pyo3"
version = "{pyo3_version}"
checksum = "pyo3"

[[package]]
name = "pyo3-build-config"
version = "0.29.2"
checksum = "build"

[[package]]
name = "pyo3-ffi"
version = "0.29.2"
checksum = "ffi"

[[package]]
name = "pyo3-async-runtimes"
version = "0.29.0"
checksum = "async"

[[package]]
name = "pyo3-log"
version = "0.13.4"
checksum = "log"
"""


def write_wheel(path: Path, native: bytes = b"native-one") -> None:
    """Create the smallest wheel archive the inspector accepts."""
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(
            "amplifier_core-2.0.1.dist-info/WHEEL",
            "Wheel-Version: 1.0\nTag: cp311-abi3-manylinux_2_28_x86_64\n",
        )
        archive.writestr(
            "amplifier_core-2.0.1.dist-info/METADATA",
            "Name: amplifier-core\nVersion: 2.0.1\n",
        )
        archive.writestr("amplifier_core/_engine.so", native)


class PyO3LockTests(unittest.TestCase):
    def test_accepts_patched_coherent_pyo3_set(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lock = Path(directory) / "Cargo.lock"
            lock.write_text(cargo_lock())
            selected = qualify.qualified_pyo3_packages(qualify.load_lock_packages(lock))
        self.assertEqual(selected[0]["name"], "pyo3")
        self.assertEqual(selected[0]["version"], "0.29.2")

    def test_rejects_vulnerable_pyo3(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lock = Path(directory) / "Cargo.lock"
            lock.write_text(cargo_lock("0.28.2"))
            with self.assertRaisesRegex(qualify.QualificationError, "unpatched"):
                qualify.qualified_pyo3_packages(qualify.load_lock_packages(lock))

    def test_requires_compatibility_features(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "Cargo.toml"
            manifest.write_text('[dependencies]\npyo3 = { features = ["abi3-py311"] }\n')
            with self.assertRaisesRegex(qualify.QualificationError, "required PyO3 features"):
                qualify.binding_features(manifest)


class WheelReceiptTests(unittest.TestCase):
    def test_records_wheel_tags_metadata_and_native_payload(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            wheel = Path(directory) / "amplifier_core-2.0.1.whl"
            write_wheel(wheel)
            facts = qualify.wheel_members(wheel)
        self.assertEqual(facts["metadata_name"], "amplifier-core")
        self.assertEqual(facts["tags"], ["cp311-abi3-manylinux_2_28_x86_64"])
        self.assertEqual(
            facts["native_payload_sha256"]["amplifier_core/_engine.so"],
            hashlib.sha256(b"native-one").hexdigest(),
        )

    def test_rejects_zero_or_multiple_wheels(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact_dir = Path(directory)
            with self.assertRaisesRegex(qualify.QualificationError, "exactly one"):
                qualify.only_wheel(artifact_dir)
            write_wheel(artifact_dir / "first.whl")
            write_wheel(artifact_dir / "second.whl")
            with self.assertRaisesRegex(qualify.QualificationError, "exactly one"):
                qualify.only_wheel(artifact_dir)

    def test_verify_integrity_rejects_source_wheel_or_native_payload_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact_dir = root / "dist"
            artifact_dir.mkdir()
            wheel = artifact_dir / "amplifier_core-2.0.1.whl"
            write_wheel(wheel)
            lock = root / "Cargo.lock"
            lock.write_text(cargo_lock())
            binding = root / "bindings/python"
            binding.mkdir(parents=True)
            (binding / "Cargo.toml").write_text(
                '[dependencies]\npyo3 = { features = '
                '["generate-import-lib", "multiple-pymethods", "abi3-py311"] }\n'
            )
            (root / "pyproject.toml").write_text(
                '[project]\nname = "amplifier-core"\nversion = "2.0.1"\n'
            )
            receipt = {
                "source_sha": "expected-sha",
                "cargo_lock_sha256": qualify.sha256_file(lock),
                "pyo3_packages": qualify.qualified_pyo3_packages(
                    qualify.load_lock_packages(lock)
                ),
                "resolved_packages": qualify.resolved_packages(
                    qualify.load_lock_packages(lock)
                ),
                "binding_features": qualify.binding_features(binding / "Cargo.toml"),
                "pyproject_name": "amplifier-core",
                "pyproject_version": "2.0.1",
                "resolved_dependency_graph": {
                    "packages": ["pyo3@0.29.2"],
                    "dependency_edges": [],
                },
                "wheel": qualify.wheel_members(wheel),
            }
            receipt["resolved_dependency_graph_sha256"] = qualify.canonical_json_sha256(
                receipt["resolved_dependency_graph"]
            )
            original_head = qualify.current_head
            qualify.current_head = lambda _: "expected-sha"
            try:
                with self.assertRaisesRegex(qualify.QualificationError, "source SHA"):
                    qualify.verify_integrity(artifact_dir, receipt, "wrong-sha", root)
                receipt["resolved_dependency_graph"]["packages"].append("other@1.0.0")
                with self.assertRaisesRegex(qualify.QualificationError, "graph hash"):
                    qualify.verify_integrity(artifact_dir, receipt, "expected-sha", root)
                receipt["resolved_dependency_graph"]["packages"].pop()
                write_wheel(wheel, b"native-two")
                with self.assertRaisesRegex(qualify.QualificationError, "wheel sha256"):
                    qualify.verify_integrity(artifact_dir, receipt, "expected-sha", root)
                receipt["wheel"]["sha256"] = qualify.sha256_file(wheel)
                with self.assertRaisesRegex(qualify.QualificationError, "native_payload_sha256"):
                    qualify.verify_integrity(artifact_dir, receipt, "expected-sha", root)
            finally:
                qualify.current_head = original_head


class VerificationGuardTests(unittest.TestCase):
    def test_rejects_dirty_tracked_rust_source(self) -> None:
        original_run = qualify.subprocess.run
        qualify.subprocess.run = lambda *args, **kwargs: types.SimpleNamespace(returncode=1)
        try:
            with self.assertRaisesRegex(qualify.QualificationError, "tracked source"):
                qualify.require_clean_build_inputs(Path("/checkout"))
        finally:
            qualify.subprocess.run = original_run

    def test_rejects_untracked_source_and_cargo_configuration(self) -> None:
        original_run = qualify.subprocess.run

        def fake_run(command, **kwargs):
            if command[:3] == ["git", "diff", "--quiet"]:
                return types.SimpleNamespace(returncode=0)
            if command[:3] == ["git", "status", "--porcelain=v1"]:
                return types.SimpleNamespace(stdout="?? src/negative_untracked.py\n")
            raise AssertionError(command)

        qualify.subprocess.run = fake_run
        try:
            with self.assertRaisesRegex(qualify.QualificationError, "negative_untracked.py"):
                qualify.require_clean_build_inputs(Path("/checkout"))

            def cargo_config_run(command, **kwargs):
                if command[:3] == ["git", "diff", "--quiet"]:
                    return types.SimpleNamespace(returncode=0)
                return types.SimpleNamespace(stdout="!! .cargo/config.toml\n")

            qualify.subprocess.run = cargo_config_run
            with self.assertRaisesRegex(qualify.QualificationError, ".cargo/config.toml"):
                qualify.require_clean_build_inputs(Path("/checkout"))
        finally:
            qualify.subprocess.run = original_run

    def test_allows_only_generated_untracked_build_output(self) -> None:
        original_run = qualify.subprocess.run

        def fake_run(command, **kwargs):
            if command[:3] == ["git", "diff", "--quiet"]:
                return types.SimpleNamespace(returncode=0)
            return types.SimpleNamespace(
                stdout="?? dist/wheel.whl\n!! target/debug/cache\n?? python/pkg/__pycache__/module.pyc\n"
            )

        qualify.subprocess.run = fake_run
        try:
            qualify.require_clean_build_inputs(Path("/checkout"))
        finally:
            qualify.subprocess.run = original_run

    def test_record_removes_stale_receipt_before_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            wheel_dir = Path(directory)
            output = wheel_dir / "build.json"
            output.write_text('{"passed": true}')
            original_clean = qualify.require_clean_build_inputs
            qualify.require_clean_build_inputs = lambda _: (_ for _ in ()).throw(
                qualify.QualificationError("dirty")
            )
            try:
                with self.assertRaisesRegex(qualify.QualificationError, "dirty"):
                    qualify.record(
                        argparse.Namespace(
                            wheel_dir=str(wheel_dir),
                            output=str(output),
                            source_sha="unused",
                            target=None,
                        )
                    )
            finally:
                qualify.require_clean_build_inputs = original_clean
            self.assertFalse(output.exists())

    def test_verify_removes_stale_report_before_invalid_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "qualification.json"
            output.write_text('{"passed": true}')
            with self.assertRaisesRegex(qualify.QualificationError, "cannot read build receipt"):
                qualify.verify(
                    argparse.Namespace(
                        artifact_dir=str(root / "missing"),
                        output=str(output),
                        source_sha="unused",
                    )
                )
            self.assertFalse(output.exists())

    def test_rejects_modified_imported_native_payload_and_engine_version(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = root / "amplifier_core" / "_engine.so"
            engine.parent.mkdir()
            engine.write_bytes(b"qualified-native")
            payloads = {
                "amplifier_core/_engine.so": hashlib.sha256(b"qualified-native").hexdigest()
            }
            observed = qualify.installed_native_payload(engine, root, payloads)
            self.assertEqual(observed["relative_path"], "amplifier_core/_engine.so")
            engine.write_bytes(b"modified-native")
            with self.assertRaisesRegex(qualify.QualificationError, "engine hash"):
                qualify.installed_native_payload(engine, root, payloads)
        with self.assertRaisesRegex(qualify.QualificationError, "engine version"):
            qualify.verify_installed_metadata(
                "amplifier-core", "2.0.1", "2.0.1", "2.0.0", "amplifier-core", "2.0.1"
            )

    def test_rejects_venv_inside_checkout_and_understands_handler_entry_shapes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            checkout = Path(directory) / "checkout"
            venv = checkout / ".venv"
            engine = venv / "site-packages/amplifier_core/_engine.so"
            package = venv / "site-packages/amplifier_core/__init__.py"
            engine.parent.mkdir(parents=True)
            engine.write_bytes(b"native")
            package.write_text("")
            with self.assertRaisesRegex(qualify.QualificationError, "checkout"):
                qualify.validate_install_locations(engine, package, venv, checkout)
            with self.assertRaisesRegex(qualify.QualificationError, "exactly ./artifacts"):
                qualify.validate_download_artifact_dir(checkout / "dist", checkout)
        self.assertIn(
            "handler",
            qualify.registered_handler_names({"event": [{"name": "handler"}]}),
        )
        self.assertNotIn(
            "handler",
            qualify.registered_handler_names({"event": [{"name": "other"}]}),
        )

    def test_download_directory_rejects_extra_source_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory) / "artifacts"
            artifacts.mkdir()
            (artifacts / "build.json").write_text("{}")
            (artifacts / "unexpected.py").write_text("")
            with self.assertRaisesRegex(qualify.QualificationError, "unexpected.py"):
                qualify.validate_download_artifact_dir(artifacts, Path(directory) / "checkout")


class CargoGraphTests(unittest.TestCase):
    def test_sanitizes_metadata_to_names_versions_and_edges(self) -> None:
        metadata = {
            "workspace_root": "/private/checkout",
            "packages": [
                {"id": "a", "name": "amplifier-core", "version": "2.0.1", "manifest_path": "/private/a"},
                {"id": "b", "name": "pyo3", "version": "0.29.2", "manifest_path": "/private/b"},
            ],
            "resolve": {"nodes": [{"id": "a", "deps": [{"pkg": "b"}]}, {"id": "b", "deps": []}]},
        }
        graph = qualify.sanitized_cargo_graph(metadata)
        self.assertEqual(graph["packages"], ["amplifier-core@2.0.1", "pyo3@0.29.2"])
        self.assertEqual(graph["dependency_edges"], [["amplifier-core@2.0.1", "pyo3@0.29.2"]])
        self.assertNotIn("/private", json.dumps(graph))

    def test_supplementary_graph_requires_valid_hash_and_recorded_nodes(self) -> None:
        resolved = [{"name": "pyo3", "version": "0.29.2", "checksum": "pyo3"}]
        graph = {"packages": ["pyo3@0.29.2"], "dependency_edges": []}
        qualify.validate_supplementary_graph(graph, resolved)
        self.assertEqual(
            qualify.canonical_json_sha256(graph),
            qualify.canonical_json_sha256({"dependency_edges": [], "packages": ["pyo3@0.29.2"]}),
        )
        with self.assertRaisesRegex(qualify.QualificationError, "unknown packages"):
            qualify.validate_supplementary_graph(
                {"packages": ["pyo3@0.28.2"], "dependency_edges": []}, resolved
            )
        with self.assertRaisesRegex(qualify.QualificationError, "unknown edge"):
            qualify.validate_supplementary_graph(
                {"packages": ["pyo3@0.29.2"], "dependency_edges": [["pyo3@0.29.2", "other@1"]]},
                resolved,
            )

    def test_rejects_unknown_architecture(self) -> None:
        with self.assertRaisesRegex(qualify.QualificationError, "CPU architecture"):
            qualify.architecture_label("mips64")


if __name__ == "__main__":
    unittest.main()