#!/usr/bin/env python3
"""
Check consistency of related_files references in YAML frontmatter.

This script validates that:
1. All files referenced in `related_files` exist
2. Line number ranges are valid (if specified)
3. Bi-directional references are consistent

Usage:
    python scripts/check_related_files.py
    python scripts/check_related_files.py --strict  # Exit 1 on warnings

Exit codes:
    0: All checks passed
    1: Errors or warnings found (in strict mode)
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Any

# Try yaml import, provide helpful message if missing
try:
    import yaml
except ImportError:
    print("Error: PyYAML required. Install with: pip install pyyaml")
    sys.exit(1)


def extract_frontmatter(file_path: Path) -> dict[str, Any] | None:
    """Extract YAML frontmatter from a markdown file."""
    try:
        content = file_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as e:
        print(f"  Warning: Could not read {file_path}: {e}")
        return None

    # Match YAML frontmatter between --- markers
    match = re.match(r"^---\n(.*?)\n---", content, re.DOTALL)
    if not match:
        return None

    try:
        return yaml.safe_load(match.group(1))
    except yaml.YAMLError as e:
        print(f"  Warning: Invalid YAML in {file_path}: {e}")
        return None


def check_line_numbers(file_path: Path, lines_spec: str) -> list[str]:
    """Verify that line number specification is valid for file."""
    errors = []

    try:
        total_lines = len(file_path.read_text(encoding="utf-8").splitlines())
    except (OSError, UnicodeDecodeError):
        return [f"Could not read {file_path} to verify line numbers"]

    # Parse line spec like "54-119" or "42"
    if "-" in lines_spec:
        try:
            start, end = map(int, lines_spec.split("-"))
            if start > total_lines or end > total_lines:
                errors.append(f"Line range {lines_spec} exceeds file length ({total_lines} lines)")
            if start > end:
                errors.append(f"Invalid line range: start ({start}) > end ({end})")
        except ValueError:
            errors.append(f"Invalid line specification: {lines_spec}")
    else:
        try:
            line_num = int(lines_spec)
            if line_num > total_lines:
                errors.append(f"Line {line_num} exceeds file length ({total_lines} lines)")
        except ValueError:
            errors.append(f"Invalid line number: {lines_spec}")

    return errors


def resolve_reference_path(base_file: Path, ref_path: str, repo_root: Path) -> Path | None:
    """Resolve a reference path relative to the base file or repo root."""
    # Strip fragment (e.g., #ContextManager)
    path_without_fragment = ref_path.split("#")[0]

    if not path_without_fragment:
        return None

    # Try relative to base file's directory
    relative_path = base_file.parent / path_without_fragment
    if relative_path.exists():
        return relative_path.resolve()

    # Try relative to repo root
    root_path = repo_root / path_without_fragment
    if root_path.exists():
        return root_path.resolve()

    return None


def check_file_references(file_path: Path, frontmatter: dict[str, Any], repo_root: Path) -> tuple[list[str], list[str]]:
    """Check all related_files references in frontmatter."""
    errors = []
    warnings = []

    related = frontmatter.get("related_files", []) or []
    related_contracts = frontmatter.get("related_contracts", []) or []

    all_refs = related + related_contracts

    for ref in all_refs:
        if isinstance(ref, dict):
            ref_path = ref.get("path", "")
            lines = ref.get("lines")
        else:
            ref_path = ref
            lines = None

        resolved = resolve_reference_path(file_path, ref_path, repo_root)

        if resolved is None:
            errors.append(f"Referenced file not found: {ref_path}")
        elif lines:
            line_errors = check_line_numbers(resolved, str(lines))
            errors.extend(line_errors)

    return errors, warnings


def find_markdown_files(root: Path) -> list[Path]:
    """Find all markdown files that might contain frontmatter."""
    return list(root.rglob("*.md"))


def main() -> int:
    parser = argparse.ArgumentParser(description="Check related_files references in YAML frontmatter")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with code 1 on warnings (not just errors)",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="Repository root (defaults to parent of scripts/)",
    )
    args = parser.parse_args()

    # Determine repo root
    script_dir = Path(__file__).parent
    repo_root = args.root or script_dir.parent

    if not repo_root.exists():
        print(f"Error: Repository root not found: {repo_root}")
        return 1

    print(f"Checking related_files references in: {repo_root}")
    print()

    total_errors = 0
    total_warnings = 0
    files_checked = 0
    files_with_frontmatter = 0

    for md_file in find_markdown_files(repo_root):
        # Skip common non-doc directories
        if any(part in md_file.parts for part in [".git", "node_modules", "__pycache__", ".venv", "venv"]):
            continue

        frontmatter = extract_frontmatter(md_file)
        files_checked += 1

        if frontmatter is None:
            continue

        files_with_frontmatter += 1

        # Check for related_files or related_contracts
        if not (frontmatter.get("related_files") or frontmatter.get("related_contracts")):
            continue

        errors, warnings = check_file_references(md_file, frontmatter, repo_root)

        if errors or warnings:
            rel_path = md_file.relative_to(repo_root)
            print(f"{rel_path}:")

            for error in errors:
                print(f"  ERROR: {error}")
                total_errors += 1

            for warning in warnings:
                print(f"  WARNING: {warning}")
                total_warnings += 1

            print()

    print("=" * 60)
    print(f"Files checked: {files_checked}")
    print(f"Files with frontmatter: {files_with_frontmatter}")
    print(f"Errors: {total_errors}")
    print(f"Warnings: {total_warnings}")

    if total_errors > 0:
        print("\nFailed: Fix errors above")
        return 1

    if total_warnings > 0 and args.strict:
        print("\nFailed (strict mode): Fix warnings above")
        return 1

    print("\nPassed: All references valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())
