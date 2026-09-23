# Native Artifact Qualification

## Release-status snapshot

As of this document's update, `2.0.1` is proposed and PyPI serves `1.6.1`.
This snapshot must be updated by the release owner when publishing. Until
then, no artifact is described as published or qualified and no alert is
described as resolved.

The proposed binding resolves PyO3 `0.29.2`, `pyo3-async-runtimes` `0.29.0`,
and `pyo3-log` `0.13.4`, preserving `abi3-py311`, `multiple-pymethods`, and
`generate-import-lib`. The latter is deprecated by PyO3 0.29 in favor of
`raw-dylib`, so Windows qualification is required before changing no linkage
behavior.

This is not lifecycle PR #114. Although both candidates propose `2.0.1`, the
security candidate is independently qualified; release ordering and a later
lifecycle version remain owner decisions.

## Security and support boundary

Review of Core source and inspected companion/macros found no direct
`nth`/`nth_back` iterator or `PyCFunction::new_closure` calls associated with
GHSA-36hh-v3qg-5jq4, GHSA-chgr-c6px-7xpp, RustSec-2026-0176, and
RustSec-2026-0177. That finding does not dismiss transitive exposure or
establish exploitability; the patched dependency graph and qualified artifacts
are the remediation evidence.

Qualification is configured for normal-GIL CPython 3.11–3.13. Free-threaded
CPython 3.13 (`3.13t`) is not qualified. The smoke check imports the installed
extension from a fresh virtual environment; verifies Rust/Python callbacks,
conversion, and cancellation; and records the observed interpreter libc.

## Evidence and provenance

The builder writes a `build.json` beside each wheel. It records source and
lockfile identity, project/version metadata, binding features, the patched
PyO3 package set, target/platform facts, and the wheel's filename, SHA-256,
tags, and native-member hashes.

`resolved_dependency_graph_sha256` is the canonical hash of the supplementary
`resolved_dependency_graph`. The graph is from `cargo metadata --locked` for
the workspace, **not** a target-specific binary SBOM. `source_sha` and
`cargo_lock_sha256` establish source provenance; they do not mean a compiled
native binary contains `Cargo.lock`.

Each verifier writes `qualification-report.json`. Its smoke evidence includes
the observed libc at
`smoke_checks.interpreter.libc`; the imported member and its observed hash are
`smoke_checks.engine.native_payload.relative_path` and `.sha256`.

## Configured qualification matrix

The workflow builds six families—Linux x64/ARM64, macOS x64/ARM64, and Windows
x64/ARM64—and configures all 18 normal-GIL CPython 3.11/3.12/3.13 verification
cells. These are required configured checks, **not validation results yet**.
Windows ARM64/Python 3.11 download availability remains unknown until its
green run; it is not a support or publication claim. Linux ARM64/Python 3.13
must be accepted only with its target and observed-libc report.

After all six builds and all 18 qualifiers pass, `release-evidence` validates
the complete set and creates `release-evidence-<source_sha>.zip` plus
`SHA256SUMS`. A branch or manual-dispatch run retains those as Actions
artifacts only. A tag run first creates a draft GitHub release and attaches
that evidence before PyPI publication; the PyPI job requires
`release-evidence`. The final `publish-release` job makes the GitHub release
public only after PyPI succeeds. This pipeline has not run for the proposed
release.

## Consumer installation and verification

1. Obtain the official release's `release-evidence-<source_sha>.zip` and
   `SHA256SUMS`, verify the archive checksum, then use its matching
   `build.json` and qualification report. Match wheel tags, CPU/platform,
   and—for Linux—the recorded target and observed libc to the deployment.
2. Check the downloaded wheel against `build.json` before installing:

   ```bash
   python -m pip install --only-binary=:all: amplifier-core==<qualified-release>
   ```

3. Keep the downloaded wheel and receipt together, then run the following
   standard-library verification with their paths substituted:

   ```python
   import hashlib
   import importlib
   import importlib.metadata
   import json
   import sys
   from pathlib import Path

   receipt = json.loads(Path("build.json").read_text())
   wheel_path = Path("amplifier_core-<qualified-release>-<tags>.whl")
   assert hashlib.sha256(wheel_path.read_bytes()).hexdigest() == receipt["wheel"]["sha256"]

   distribution = importlib.metadata.distribution(receipt["pyproject_name"])
   metadata_version = importlib.metadata.version(receipt["pyproject_name"])
   engine = importlib.import_module("amplifier_core._engine")
   native_path = Path(engine.__file__).resolve()
   distribution_root = Path(distribution.locate_file("")).resolve()
   member_key = native_path.relative_to(distribution_root).as_posix()
   loaded_sha256 = hashlib.sha256(native_path.read_bytes()).hexdigest()

   assert metadata_version == engine.__version__ == receipt["pyproject_version"]
   assert loaded_sha256 == receipt["wheel"]["native_payload_sha256"][member_key]
   assert native_path.is_relative_to(Path(sys.prefix).resolve())
   print("distribution:", metadata_version)
   print("native member:", member_key, loaded_sha256)
   print("proposed receipt source_sha:", receipt["source_sha"])
   ```

The loaded engine version proves package-version agreement, not a Git revision.
Use the receipt's `source_sha`, plus its wheel and native-member hash matches,
to map the installed binary to the proposed source record. Prefer qualified
wheels over independently rebuilding native Core for routine application
releases.