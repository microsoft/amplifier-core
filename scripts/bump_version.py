#!/usr/bin/env python3
"""Atomic version bump script for amplifier-core.

Updates all three version files in sync and prints the git commands to
complete the release. Run this immediately after merging a PR to main.

This script exists because amplifier-core is published to PyPI and the
three version files MUST stay in sync. Manual edits to individual files
caused drift in the past. Use this script exclusively.

See: docs/CORE_DEVELOPMENT_PRINCIPLES.md §10 — The Release Gate

Usage:
    python scripts/bump_version.py X.Y.Z
    python scripts/bump_version.py vX.Y.Z   # leading 'v' is stripped
"""

import re
import sys
from pathlib import Path
from typing import NoReturn

# Repo root is one level up from scripts/
REPO_ROOT = Path(__file__).parent.parent

# All three version files that must be bumped in lockstep
VERSION_FILES = [
    # (relative path, regex to find version line, expected line number — sanity only)
    ("pyproject.toml", 3),
    ("crates/amplifier-core/Cargo.toml", 3),
    ("bindings/python/Cargo.toml", 3),
]

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+$")
VERSION_LINE_RE = re.compile(r'^(version\s*=\s*")([^"]+)(")', re.MULTILINE)


def die(msg: str) -> NoReturn:
    print(f"ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def validate_version(version: str) -> str:
    """Strip leading 'v' and validate semver format. Returns clean version string."""
    if version.startswith("v") or version.startswith("V"):
        version = version[1:]
    if not SEMVER_RE.match(version):
        die(f"Version '{version}' is not valid semver (expected X.Y.Z, e.g. 1.2.3)")
    return version


def find_version_in_file(content: str, filepath: str) -> str:
    """Extract the current version string from a file."""
    match = VERSION_LINE_RE.search(content)
    if not match:
        die(f"Could not find version line in {filepath}")
    return match.group(2)


def replace_version_in_file(content: str, new_version: str) -> str:
    """Replace the version string in file content."""
    return VERSION_LINE_RE.sub(rf"\g<1>{new_version}\g<3>", content, count=1)


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__)
        sys.exit(0 if sys.argv[1:] == ["--help"] else 1)

    new_version = validate_version(sys.argv[1])

    print(f"Bumping amplifier-core to v{new_version}")
    print()

    # Read all files first, detect existing versions and out-of-sync state
    file_data: list[tuple[Path, str, str]] = []  # (path, content, old_version)
    old_versions: set[str] = set()

    for rel_path, _ in VERSION_FILES:
        abs_path = REPO_ROOT / rel_path
        if not abs_path.exists():
            die(
                f"Version file not found: {rel_path}\n"
                f"  Expected at: {abs_path}\n"
                f"  Are you running from the repo root?"
            )
        content = abs_path.read_text()
        old_version = find_version_in_file(content, rel_path)
        old_versions.add(old_version)
        file_data.append((abs_path, content, old_version))

    # Warn if files are already out of sync (canary for prior manual edits)
    if len(old_versions) > 1:
        print("WARNING: Version files were already out of sync before this bump!")
        print("         This may indicate prior manual edits. Versions found:")
        for (abs_path, _, old_ver), (rel_path, _) in zip(file_data, VERSION_FILES):
            print(f"           {rel_path}: {old_ver}")
        print()

    # Validate we're not bumping to the same version
    if new_version in old_versions:
        die(
            f"New version {new_version} is the same as the current version. "
            f"Did you mean to bump to a higher version?"
        )

    # Apply updates atomically (read all first, then write all or none)
    new_contents: list[tuple[Path, str]] = []
    for abs_path, content, _ in file_data:
        new_content = replace_version_in_file(content, new_version)
        if new_content == content:
            die(
                f"Version replacement had no effect in {abs_path.relative_to(REPO_ROOT)}"
            )
        new_contents.append((abs_path, new_content))

    for abs_path, new_content in new_contents:
        abs_path.write_text(new_content)

    # Report what was done
    canonical_old = sorted(old_versions)[0]  # pick one if they were in sync
    print("Updated version files:")
    for rel_path, _ in VERSION_FILES:
        print(f"  {rel_path}: {canonical_old} → {new_version}")

    print()
    print("Next steps — run these git commands to complete the release:")
    print()
    print(f'  git commit -am "chore: bump version to {new_version}"')
    print(f"  git tag v{new_version}")
    print("  git push origin main --tags")
    print()
    print(f"The v{new_version} tag triggers rust-core-wheels.yml → PyPI publish.")
    print("CI will build wheels for all platforms and publish automatically.")
    print()
    print(f"Done. amplifier-core v{new_version} is ready to release.")


if __name__ == "__main__":
    main()
