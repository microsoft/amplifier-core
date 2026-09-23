#!/usr/bin/env python3
"""Record and verify provenance for one official amplifier-core native wheel."""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import importlib.metadata
import json
import platform
import subprocess
import sys
import sysconfig
import tempfile
import tomllib
import zipfile
from pathlib import Path
from typing import Any


REQUIRED_PYO3_FEATURES = {
    "generate-import-lib",
    "multiple-pymethods",
    "abi3-py311",
}


class QualificationError(RuntimeError):
    """A qualification assertion that must stop publishing."""


def sha256_bytes(data: bytes) -> str:
    """Return the SHA-256 digest for bytes."""
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest for a file without loading it all at once."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def version_at_least(version: str, minimum: tuple[int, int, int]) -> bool:
    """Compare the numeric prefix of a Cargo version without third-party packages."""
    parts = version.split(".")
    try:
        parsed = tuple(int(part) for part in parts[:3])
    except ValueError as error:
        raise QualificationError(f"invalid package version {version!r}") from error
    parsed = parsed + (0,) * (3 - len(parsed))
    return parsed >= minimum


def load_lock_packages(lock_path: Path) -> list[dict[str, Any]]:
    """Load the Cargo lockfile's package records."""
    try:
        packages = tomllib.loads(lock_path.read_text(encoding="utf-8"))["package"]
    except (KeyError, OSError, tomllib.TOMLDecodeError) as error:
        raise QualificationError(f"cannot read Cargo lockfile: {error}") from error
    if not isinstance(packages, list):
        raise QualificationError("Cargo lockfile package table is not a list")
    return packages


def qualified_pyo3_packages(packages: list[dict[str, Any]]) -> list[dict[str, str | None]]:
    """Reject vulnerable or incoherent PyO3 package sets and retain their checksums."""
    by_name: dict[str, list[dict[str, Any]]] = {}
    for package in packages:
        name = package.get("name")
        version = package.get("version")
        if isinstance(name, str) and isinstance(version, str):
            by_name.setdefault(name, []).append(package)

    for name, minimum in (
        ("pyo3", (0, 29, 0)),
        ("pyo3-async-runtimes", (0, 29, 0)),
        ("pyo3-log", (0, 13, 4)),
    ):
        matches = by_name.get(name, [])
        if not matches:
            raise QualificationError(f"Cargo.lock does not resolve required package {name}")
        if any(not version_at_least(match["version"], minimum) for match in matches):
            versions = ", ".join(match["version"] for match in matches)
            raise QualificationError(f"{name} has unpatched resolved version(s): {versions}")

    pyo3_family = [
        package
        for name, records in by_name.items()
        if name.startswith("pyo3")
        and name not in {"pyo3-async-runtimes", "pyo3-log"}
        for package in records
    ]
    if not pyo3_family:
        raise QualificationError("Cargo.lock has no PyO3 family packages")
    if any(not version_at_least(package["version"], (0, 29, 0)) for package in pyo3_family):
        versions = ", ".join(
            f'{package["name"]} {package["version"]}' for package in pyo3_family
        )
        raise QualificationError(f"PyO3 family is not coherently patched: {versions}")

    selected = [
        package
        for name, records in by_name.items()
        if name.startswith("pyo3")
        for package in records
    ]
    return [
        {
            "name": package["name"],
            "version": package["version"],
            "checksum": package.get("checksum"),
        }
        for package in sorted(selected, key=lambda item: (item["name"], item["version"]))
    ]


def resolved_packages(packages: list[dict[str, Any]]) -> list[dict[str, str | None]]:
    """Retain resolved versions and checksums without sources or filesystem paths."""
    records = [
        {
            "name": package["name"],
            "version": package["version"],
            "checksum": package.get("checksum"),
        }
        for package in packages
        if isinstance(package.get("name"), str) and isinstance(package.get("version"), str)
    ]
    return sorted(records, key=lambda item: (item["name"], item["version"]))


def binding_features(binding_manifest: Path) -> list[str]:
    """Read and validate the PyO3 compatibility features from the binding manifest."""
    try:
        dependency = tomllib.loads(binding_manifest.read_text(encoding="utf-8"))["dependencies"]["pyo3"]
        features = dependency["features"]
    except (KeyError, OSError, tomllib.TOMLDecodeError, TypeError) as error:
        raise QualificationError(f"cannot read PyO3 binding features: {error}") from error
    if not isinstance(features, list) or not all(isinstance(item, str) for item in features):
        raise QualificationError("PyO3 binding features are not a list of strings")
    missing = REQUIRED_PYO3_FEATURES - set(features)
    if missing:
        raise QualificationError(f"required PyO3 features are missing: {', '.join(sorted(missing))}")
    return sorted(features)


def wheel_members(wheel: Path) -> dict[str, Any]:
    """Extract only the immutable wheel facts needed for artifact verification."""
    try:
        with zipfile.ZipFile(wheel) as archive:
            names = archive.namelist()
            wheel_files = [name for name in names if name.endswith(".dist-info/WHEEL")]
            metadata_files = [name for name in names if name.endswith(".dist-info/METADATA")]
            native_files = [name for name in names if name.lower().endswith((".so", ".pyd"))]
            if len(wheel_files) != 1 or len(metadata_files) != 1:
                raise QualificationError("wheel must contain exactly one WHEEL and one METADATA file")
            if not native_files:
                raise QualificationError("wheel has no native .so or .pyd payload")
            wheel_headers = parse_rfc822(archive.read(wheel_files[0]).decode("utf-8"))
            metadata_headers = parse_rfc822(archive.read(metadata_files[0]).decode("utf-8"))
            name = metadata_headers.get("Name")
            version = metadata_headers.get("Version")
            tags = wheel_headers.get("Tag", [])
            if not isinstance(name, str) or not isinstance(version, str) or not tags:
                raise QualificationError("wheel WHEEL/METADATA fields are incomplete")
            payloads = {
                name: sha256_bytes(archive.read(name))
                for name in sorted(native_files)
            }
    except (OSError, UnicodeDecodeError, zipfile.BadZipFile) as error:
        raise QualificationError(f"cannot inspect wheel {wheel}: {error}") from error
    return {
        "filename": wheel.name,
        "sha256": sha256_file(wheel),
        "tags": tags,
        "metadata_name": name,
        "metadata_version": version,
        "native_payload_sha256": payloads,
    }


def project_metadata(pyproject_path: Path) -> tuple[str, str]:
    """Read the project identity that must agree with the built wheel."""
    try:
        project = tomllib.loads(pyproject_path.read_text(encoding="utf-8"))["project"]
        name = project["name"]
        version = project["version"]
    except (KeyError, OSError, tomllib.TOMLDecodeError, TypeError) as error:
        raise QualificationError(f"cannot read project metadata: {error}") from error
    if not isinstance(name, str) or not isinstance(version, str):
        raise QualificationError("project name or version is not a string")
    return name, version


def target_info(label: str, observed_system: str) -> dict[str, str]:
    """Normalize a CI target label to its expected OS and architecture."""
    normalized = label.lower().replace("_", "-")
    system = (
        "Linux" if "linux" in normalized or "ubuntu" in normalized
        else "Darwin" if "macos" in normalized or "darwin" in normalized
        else "Windows" if "windows" in normalized or normalized.startswith("win-")
        else observed_system
    )
    return {
        "label": label,
        "system": system,
        "architecture": architecture_label(label),
    }


def wheel_tags_match_target(tags: list[str], target: dict[str, str]) -> bool:
    """Require wheel tags to declare the platform and CPU the receipt claims."""
    combined = " ".join(tags).lower()
    platform_matches = {
        "Linux": ("linux" in combined or "manylinux" in combined),
        "Darwin": "macosx" in combined,
        "Windows": "win" in combined,
    }
    architecture_matches = {
        "x64": ("x86_64" in combined or "amd64" in combined),
        "arm64": ("aarch64" in combined or "arm64" in combined),
    }
    return platform_matches.get(target["system"], False) and architecture_matches[
        target["architecture"]
    ]


def parse_rfc822(text: str) -> dict[str, str | list[str]]:
    """Parse the small header subset used by wheel metadata."""
    headers: dict[str, str | list[str]] = {}
    for line in text.splitlines():
        if not line or line[0].isspace() or ":" not in line:
            continue
        key, value = line.split(":", 1)
        value = value.strip()
        existing = headers.get(key)
        if existing is None:
            headers[key] = value
        elif isinstance(existing, list):
            existing.append(value)
        else:
            headers[key] = [existing, value]
    tag = headers.get("Tag")
    if isinstance(tag, str):
        headers["Tag"] = [tag]
    return headers


def only_wheel(directory: Path) -> Path:
    """Require the build artifact directory to contain exactly one wheel."""
    wheels = sorted(directory.glob("*.whl"))
    if len(wheels) != 1:
        raise QualificationError(
            f"expected exactly one wheel in {directory}, found {len(wheels)}"
        )
    return wheels[0]


def validate_download_artifact_dir(artifact_dir: Path, repo_root: Path) -> None:
    """Accept checkout-local input only at the workflow's isolated artifacts path."""
    if relative_to(artifact_dir, repo_root) and artifact_dir != repo_root / "artifacts":
        raise QualificationError("checkout-local artifact directory must be exactly ./artifacts")
    if not artifact_dir.is_dir():
        raise QualificationError("artifact directory does not exist")
    unexpected = [
        path.name
        for path in artifact_dir.iterdir()
        if path.name != "build.json" and not (path.is_file() and path.suffix == ".whl")
    ]
    if unexpected:
        raise QualificationError(
            "artifact directory contains unexpected files: " + ", ".join(sorted(unexpected))
        )


def run_text(command: list[str], cwd: Path) -> str:
    """Run a provenance command and return its stdout."""
    return subprocess.run(
        command, cwd=cwd, check=True, text=True, capture_output=True
    ).stdout


def current_head(repo_root: Path) -> str:
    """Read the checkout revision from Git rather than trusting caller input."""
    return run_text(["git", "rev-parse", "HEAD"], repo_root).strip()


def allowed_generated_path(path: str) -> bool:
    """Allow only root build output and Python bytecode outside tracked source."""
    normalized = path.rstrip("/")
    if normalized in {"dist", "target"} or normalized.startswith(("dist/", "target/")):
        return True
    parts = normalized.split("/")
    return (
        len(parts) >= 2
        and parts[-2] == "__pycache__"
        and parts[-1].endswith(".pyc")
    )


def require_clean_build_inputs(repo_root: Path) -> None:
    """Reject tracked or source-like untracked/ignored inputs from a build receipt."""
    command = ["git", "diff", "--quiet", "HEAD"]
    result = subprocess.run(command, cwd=repo_root, check=False)
    if result.returncode == 1:
        raise QualificationError("tracked source differs from HEAD")
    if result.returncode != 0:
        raise QualificationError(f"could not check source status: {' '.join(command)}")
    status = run_text(
        [
            "git",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
        ],
        repo_root,
    )
    unexpected = [
        line[3:]
        for line in status.splitlines()
        if line[:2] in {"??", "!!"} and not allowed_generated_path(line[3:])
    ]
    if unexpected:
        raise QualificationError(
            "unexpected untracked or ignored source input: " + ", ".join(unexpected)
        )


def sanitized_cargo_graph(metadata: dict[str, Any]) -> dict[str, list[Any]]:
    """Keep package names, versions, and dependency edges; discard metadata paths."""
    packages = metadata.get("packages")
    resolve = metadata.get("resolve")
    if not isinstance(packages, list) or not isinstance(resolve, dict):
        raise QualificationError("cargo metadata is missing packages or resolve")
    identifiers: dict[str, str] = {}
    nodes = []
    for package in packages:
        if not all(isinstance(package.get(key), str) for key in ("id", "name", "version")):
            raise QualificationError("cargo metadata package has incomplete identity")
        identity = f'{package["name"]}@{package["version"]}'
        identifiers[package["id"]] = identity
        nodes.append(identity)
    edges: set[tuple[str, str]] = set()
    for node in resolve.get("nodes", []):
        node_id = node.get("id")
        if node_id not in identifiers:
            raise QualificationError("cargo metadata resolve node is unknown")
        for dependency in node.get("deps", []):
            package_id = dependency.get("pkg")
            if package_id not in identifiers:
                raise QualificationError("cargo metadata dependency is unknown")
            edges.add((identifiers[node_id], identifiers[package_id]))
    return {
        "packages": sorted(set(nodes)),
        "dependency_edges": [list(edge) for edge in sorted(edges)],
    }


def canonical_json_sha256(value: Any) -> str:
    """Hash canonical JSON for a supplementary, portable receipt structure."""
    return sha256_bytes(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    )


def validate_supplementary_graph(
    graph: Any, resolved: list[dict[str, str | None]]
) -> None:
    """Ensure a build-time graph references only recorded lockfile package identities."""
    if not isinstance(graph, dict):
        raise QualificationError("supplementary dependency graph is missing")
    nodes = graph.get("packages")
    edges = graph.get("dependency_edges")
    if not isinstance(nodes, list) or not isinstance(edges, list):
        raise QualificationError("supplementary dependency graph is incomplete")
    if not all(isinstance(node, str) for node in nodes) or len(nodes) != len(set(nodes)):
        raise QualificationError("supplementary dependency graph has invalid package nodes")
    recorded = {f'{item["name"]}@{item["version"]}' for item in resolved}
    node_set = set(nodes)
    if not node_set or not node_set.issubset(recorded):
        raise QualificationError("supplementary dependency graph references unknown packages")
    edge_tuples: list[tuple[str, str]] = []
    for edge in edges:
        if (
            not isinstance(edge, list)
            or len(edge) != 2
            or not all(isinstance(item, str) for item in edge)
            or not set(edge).issubset(node_set)
        ):
            raise QualificationError("supplementary dependency graph has an unknown edge")
        edge_tuples.append((edge[0], edge[1]))
    if len(edge_tuples) != len(set(edge_tuples)):
        raise QualificationError("supplementary dependency graph has duplicate edges")


def architecture_label(value: str) -> str:
    """Normalize matrix labels, Rust triples, and platform.machine() output."""
    normalized = value.lower().replace("-", "_")
    if "aarch64" in normalized or "arm64" in normalized:
        return "arm64"
    if (
        "x86_64" in normalized
        or "amd64" in normalized
        or normalized == "x64"
        or normalized.endswith("_x64")
    ):
        return "x64"
    raise QualificationError(f"cannot determine CPU architecture from {value!r}")


def write_json(path: Path, data: dict[str, Any]) -> None:
    """Atomically write a JSON report only after all assertions have passed."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False
    ) as handle:
        json.dump(data, handle, indent=2, sort_keys=True)
        handle.write("\n")
        temporary = Path(handle.name)
    temporary.replace(path)


def record(args: argparse.Namespace) -> None:
    """Create a build receipt next to one wheel."""
    repo_root = Path.cwd().resolve()
    wheel_dir = Path(args.wheel_dir).resolve()
    output = Path(args.output).resolve()
    if output.parent != wheel_dir:
        raise QualificationError("--output must be alongside the wheel")
    if output.suffix == ".whl":
        raise QualificationError("--output must not overwrite a wheel artifact")
    output.unlink(missing_ok=True)
    require_clean_build_inputs(repo_root)
    observed_head = current_head(repo_root)
    if args.source_sha != observed_head:
        raise QualificationError("--source-sha does not match the current Git HEAD")

    wheel = only_wheel(wheel_dir)
    lock_path = repo_root / "Cargo.lock"
    pyproject_path = repo_root / "pyproject.toml"
    binding_manifest = repo_root / "bindings/python/Cargo.toml"
    packages = load_lock_packages(lock_path)
    metadata = json.loads(run_text(
        ["cargo", "metadata", "--locked", "--format-version", "1"], repo_root
    ))
    project_name, project_version = project_metadata(pyproject_path)
    target = args.target or platform.machine()
    target = target_info(target, platform.system())
    if target["system"] != platform.system():
        raise QualificationError("target OS does not match the wheel build runner OS")
    wheel_facts = wheel_members(wheel)
    if (
        wheel_facts["metadata_name"] != project_name
        or wheel_facts["metadata_version"] != project_version
    ):
        raise QualificationError("wheel metadata name or version does not match pyproject.toml")
    if not wheel_tags_match_target(wheel_facts["tags"], target):
        raise QualificationError("wheel tags do not match the declared target OS and CPU")
    receipt = {
        "schema_version": 1,
        "source_sha": observed_head,
        "cargo_lock_sha256": sha256_file(lock_path),
        "pyproject_name": project_name,
        "pyproject_version": project_version,
        "binding_features": binding_features(binding_manifest),
        "rustc_version": run_text(["rustc", "--version"], repo_root).strip(),
        "platform": {
            "system": platform.system(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "target": target,
        "pyo3_packages": qualified_pyo3_packages(packages),
        "resolved_packages": resolved_packages(packages),
        "resolved_dependency_graph": sanitized_cargo_graph(metadata),
        "resolved_dependency_graph_scope": (
            "cargo metadata --locked resolved workspace graph; "
            "not a target-specific binary SBOM"
        ),
        "wheel": wheel_facts,
    }
    receipt["resolved_dependency_graph_sha256"] = canonical_json_sha256(
        receipt["resolved_dependency_graph"]
    )
    # Raw Cargo metadata is an internal CI receipt. It is deliberately not uploaded.
    write_json(wheel_dir / "cargo-metadata.json", metadata)
    write_json(output, receipt)


def relative_to(path: Path, parent: Path) -> bool:
    """Return whether path is inside parent on supported Python versions."""
    try:
        path.resolve().relative_to(parent.resolve())
    except ValueError:
        return False
    return True


def verify_integrity(
    artifact_dir: Path, receipt: dict[str, Any], source_sha: str, repo_root: Path
) -> dict[str, Any]:
    """Verify immutable source, lockfile, wheel, and native-payload provenance."""
    if receipt.get("source_sha") != source_sha:
        raise QualificationError("build receipt source SHA does not match --source-sha")
    if current_head(repo_root) != source_sha:
        raise QualificationError("current checkout HEAD does not match --source-sha")
    lock_sha = sha256_file(repo_root / "Cargo.lock")
    if receipt.get("cargo_lock_sha256") != lock_sha:
        raise QualificationError("current Cargo.lock hash does not match build receipt")
    packages = load_lock_packages(repo_root / "Cargo.lock")
    if receipt.get("pyo3_packages") != qualified_pyo3_packages(packages):
        raise QualificationError("current resolved PyO3 packages do not match build receipt")
    if receipt.get("resolved_packages") != resolved_packages(packages):
        raise QualificationError("current resolved packages do not match build receipt")
    graph = receipt.get("resolved_dependency_graph")
    if receipt.get("resolved_dependency_graph_sha256") != canonical_json_sha256(graph):
        raise QualificationError("supplementary dependency graph hash does not match")
    validate_supplementary_graph(graph, receipt["resolved_packages"])
    if receipt.get("binding_features") != binding_features(
        repo_root / "bindings/python/Cargo.toml"
    ):
        raise QualificationError("current PyO3 binding features do not match build receipt")
    current_name, current_version = project_metadata(repo_root / "pyproject.toml")
    if (
        receipt.get("pyproject_name") != current_name
        or receipt.get("pyproject_version") != current_version
    ):
        raise QualificationError("current project metadata does not match build receipt")
    wheel = only_wheel(artifact_dir)
    actual = wheel_members(wheel)
    expected = receipt.get("wheel")
    if not isinstance(expected, dict):
        raise QualificationError("build receipt has no wheel record")
    for field in ("filename", "sha256", "metadata_name", "metadata_version", "tags", "native_payload_sha256"):
        if actual.get(field) != expected.get(field):
            raise QualificationError(f"wheel {field} does not match build receipt")
    return actual


def validate_install_locations(
    engine_path: Path, package_path: Path, venv_root: Path, repo_root: Path
) -> None:
    """Require the imported package and extension to come from a venv outside checkout."""
    if not relative_to(engine_path, venv_root) or not relative_to(package_path, venv_root):
        raise QualificationError("amplifier_core did not import from the current virtual environment")
    if relative_to(venv_root, repo_root) or relative_to(engine_path, repo_root) or relative_to(
        package_path, repo_root
    ):
        raise QualificationError("amplifier_core imported from the checkout rather than the installed wheel")


def verify_installed_metadata(
    distribution_name: str | None,
    distribution_version: str,
    package_version: str,
    engine_version: str,
    expected_name: str,
    expected_version: str,
) -> None:
    """Ensure all installed metadata surfaces name and version-identically."""
    if distribution_name != expected_name:
        raise QualificationError("installed distribution name does not match the qualified wheel")
    if {
        distribution_version,
        package_version,
        engine_version,
    } != {expected_version}:
        raise QualificationError("installed package or engine version does not match the qualified wheel")


def installed_native_payload(
    engine_path: Path, distribution_root: Path, expected_payloads: dict[str, Any]
) -> dict[str, str]:
    """Hash the imported extension and map it to its exact native wheel member."""
    try:
        relative_path = engine_path.resolve().relative_to(distribution_root.resolve()).as_posix()
    except ValueError as error:
        raise QualificationError("imported native engine is outside the installed distribution") from error
    expected_hash = expected_payloads.get(relative_path)
    if not isinstance(expected_hash, str):
        raise QualificationError("imported native engine is not an expected wheel payload member")
    observed_hash = sha256_file(engine_path)
    if observed_hash != expected_hash:
        raise QualificationError("imported native engine hash does not match the qualified wheel")
    return {"relative_path": relative_path, "sha256": observed_hash}


def registered_handler_names(value: Any) -> set[str]:
    """Extract handler names from the public registry's list response."""
    if isinstance(value, str):
        return {value}
    if isinstance(value, dict):
        names = {value["name"]} if isinstance(value.get("name"), str) else set()
        for item in value.values():
            names.update(registered_handler_names(item))
        return names
    if isinstance(value, (list, tuple, set)):
        return set().union(*(registered_handler_names(item) for item in value))
    return set()


async def run_native_smoke(
    target: dict[str, str],
    expected_name: str,
    expected_version: str,
    expected_payloads: dict[str, Any],
    repo_root: Path,
) -> dict[str, Any]:
    """Exercise public Rust-backed callbacks and cancellation after wheel installation."""
    import amplifier_core
    import amplifier_core._engine as engine
    from amplifier_core._engine import RustCancellationToken, RustHookRegistry

    if engine.RUST_AVAILABLE is not True:
        raise QualificationError("native engine does not report RUST_AVAILABLE=True")
    distribution = importlib.metadata.distribution(expected_name)
    verify_installed_metadata(
        distribution.metadata.get("Name"),
        distribution.version,
        amplifier_core.__version__,
        engine.__version__,
        expected_name,
        expected_version,
    )
    engine_path = Path(engine.__file__).resolve()
    package_path = Path(amplifier_core.__file__).resolve()
    distribution_root = Path(distribution.locate_file("")).resolve()
    validate_install_locations(engine_path, package_path, Path(sys.prefix), repo_root)
    if sysconfig.get_config_var("Py_GIL_DISABLED") not in (None, 0, "0", False):
        raise QualificationError("qualification requires a normal-GIL Python interpreter")
    if platform.system() != target["system"]:
        raise QualificationError("running operating system does not match the wheel target")
    if architecture_label(platform.machine()) != target["architecture"]:
        raise QualificationError("running Python CPU does not match the wheel target")

    sync_called = False
    registry = RustHookRegistry()

    def sync_handler(event: str, data: dict[str, Any]) -> dict[str, Any]:
        nonlocal sync_called
        sync_called = event == "qualification:sync" and data["conversion"]["integer"] == 1
        return {"action": "continue", "data": {"mode": "sync"}}

    registry.register("qualification:sync", sync_handler, name="qualification-sync")
    if "qualification-sync" not in registered_handler_names(
        registry.list_handlers("qualification:sync")
    ):
        raise QualificationError("Rust hook callback was not registered")
    sync_result = await registry.emit("qualification:sync", {"conversion": {"integer": 1}})
    registry.unregister("qualification-sync")
    if not sync_called or sync_result.action != "continue":
        raise QualificationError("synchronous Rust hook callback did not return a continue result")
    if "qualification-sync" in registered_handler_names(
        registry.list_handlers("qualification:sync")
    ):
        raise QualificationError("Rust hook callback did not unregister")

    async_called = False

    async def async_handler(event: str, data: dict[str, Any]) -> dict[str, Any]:
        nonlocal async_called
        await asyncio.sleep(0)
        async_called = event == "qualification:async" and data.get("safe") is True
        return {"action": "continue", "data": {"mode": "async"}}

    registry.register("qualification:async", async_handler, name="qualification-async")
    if "qualification-async" not in registered_handler_names(
        registry.list_handlers("qualification:async")
    ):
        raise QualificationError("asynchronous Rust hook callback was not registered")
    async_result = await registry.emit("qualification:async", {"safe": True})
    registry.unregister("qualification-async")
    if not async_called or async_result.action != "continue":
        raise QualificationError("asynchronous Rust hook callback did not return a continue result")
    if "qualification-async" in registered_handler_names(
        registry.list_handlers("qualification:async")
    ):
        raise QualificationError("asynchronous Rust hook callback did not unregister")

    callback_called = False
    token = RustCancellationToken()

    async def cancellation_callback() -> None:
        nonlocal callback_called
        callback_called = True

    token.on_cancel(cancellation_callback)
    if not token.request_graceful() or token.state != "graceful" or not token.is_cancelled:
        raise QualificationError("Rust cancellation token did not enter graceful state")
    await token.trigger_callbacks()
    token.request_immediate()
    if not callback_called or token.state != "immediate":
        raise QualificationError("Rust cancellation callback or transition failed")
    token.reset()
    if token.state != "none" or token.is_cancelled:
        raise QualificationError("Rust cancellation token did not reset")
    return {
        "engine": {
            "version": engine.__version__,
            "native_payload": installed_native_payload(
                engine_path, distribution_root, expected_payloads
            ),
        },
        "interpreter": {
            "implementation": platform.python_implementation(),
            "version": platform.python_version(),
            "libc": list(platform.libc_ver()),
        },
        "checks": {
            "rust_available": True,
            "installed_from_venv": True,
            "normal_gil": True,
            "native_cpu_matches_target": True,
            "sync_callback_and_unregister": True,
            "async_callback_and_conversion": True,
            "cancellation_callback_transition_and_reset": True,
        },
    }


def verify(args: argparse.Namespace) -> None:
    """Verify an installed wheel and write a report only when every assertion succeeds."""
    artifact_dir = Path(args.artifact_dir).resolve()
    output = Path(args.output).resolve()
    if output.suffix == ".whl":
        raise QualificationError("--output must not overwrite a wheel artifact")
    output.unlink(missing_ok=True)
    receipt_path = artifact_dir / "build.json"
    try:
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise QualificationError(f"cannot read build receipt: {error}") from error
    repo_root = Path.cwd().resolve()
    validate_download_artifact_dir(artifact_dir, repo_root)
    wheel = verify_integrity(artifact_dir, receipt, args.source_sha, repo_root)
    target = receipt.get("target")
    if not isinstance(target, dict) or not all(
        isinstance(target.get(field), str) for field in ("system", "architecture")
    ):
        raise QualificationError("build receipt target is incomplete")
    expected_name = receipt.get("pyproject_name")
    expected_version = receipt.get("pyproject_version")
    expected_payloads = wheel.get("native_payload_sha256")
    if (
        not isinstance(expected_name, str)
        or not isinstance(expected_version, str)
        or not isinstance(expected_payloads, dict)
    ):
        raise QualificationError("build receipt project or native payload details are missing")
    if (
        wheel["metadata_name"] != expected_name
        or wheel["metadata_version"] != expected_version
        or not wheel_tags_match_target(wheel["tags"], target)
    ):
        raise QualificationError("wheel metadata or tags do not match build receipt project target")
    smoke_checks = asyncio.run(
        run_native_smoke(
            target, expected_name, expected_version, expected_payloads, repo_root
        )
    )
    report = {
        "schema_version": 1,
        "source_sha": args.source_sha,
        "cargo_lock_sha256": receipt["cargo_lock_sha256"],
        "wheel": wheel,
        "platform": {
            "system": platform.system(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "target": target,
        "smoke_checks": smoke_checks,
        "passed": True,
    }
    write_json(output, report)


def parser() -> argparse.ArgumentParser:
    """Build the small, dependency-free command-line interface."""
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    record_parser = commands.add_parser("record", help="write a wheel build receipt")
    record_parser.add_argument("--wheel-dir", required=True)
    record_parser.add_argument("--output", required=True)
    record_parser.add_argument("--source-sha", required=True)
    record_parser.add_argument("--target", help="matrix target label or Rust target triple")
    record_parser.set_defaults(handler=record)
    verify_parser = commands.add_parser("verify", help="verify an installed wheel")
    verify_parser.add_argument("--artifact-dir", required=True)
    verify_parser.add_argument("--source-sha", required=True)
    verify_parser.add_argument("--output", required=True)
    verify_parser.set_defaults(handler=verify)
    return result


def main() -> None:
    """Run the requested qualification phase."""
    argument_parser = parser()
    args = argument_parser.parse_args()
    try:
        args.handler(args)
    except QualificationError as error:
        argument_parser.exit(1, f"qualification failed: {error}\n")


if __name__ == "__main__":
    main()