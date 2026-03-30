---
name: release-mandate
description: 'amplifier-core release protocol: version bump checklist, E2E smoke test, incident playbook. Required reading before tagging a release.'
version: 1.0.0
---
# Release Mandate: amplifier-core

## The Rule

**Every PR merged to `amplifier-core` main MUST be immediately followed by a version bump, release commit, `v{version}` tag, and tag push. No exceptions.**

This is not a suggestion. It is the enforcement mechanism for the backward compatibility guarantee.

---

## Why This Rule Exists (and Why It's Unique to This Repo)

`amplifier-core` occupies a unique position in the Amplifier ecosystem:

- **It is the only ecosystem repo published to PyPI.** Users install it with `pip install amplifier-core` or `uv tool install amplifier`. They get the version that was last tagged and pushed to PyPI.
- **Downstream modules (amplifier-module-*, providers, bundles) install from git** and track `main` directly. When a module is updated, it picks up whatever is on `main` immediately.

This creates a version skew window: from the moment a PR is merged until a release tag is pushed and PyPI publishes, **git HEAD and PyPI diverge**. Any module author who updates their module during that window — or any user who installs the new module against the current PyPI release — will hit a mismatch.

**The incident that created this rule:** Commit `580ecc0` ("eliminate Python RetryConfig") was merged to main on March 3, 2026, but no release was cut. `provider-anthropic` was updated to use the new API (`initial_delay` instead of `min_delay`). All users on the PyPI v1.0.7 release broke immediately. An emergency v1.0.8 hotfix was required.

---

## Scope: This Rule Is for amplifier-core Only

Most other ecosystem repos — `amplifier-module-*`, `amplifier-bundle-*`, `amplifier-app-*`, provider repos — use `git+https` references for Python. Their users and consumers pick up changes directly from git. **Individual repo authors choose their own release process** for those repos. This mandate does not apply to them.

This rule exists **specifically** because `amplifier-core` publishes to PyPI and the rest of the ecosystem depends on that published package.

---

## The Checklist (Every Merge)

1. Determine the new version (semver: PATCH for bug fixes, MINOR for additive API, MAJOR for breaking)
2. Run the atomic bump script:
   ```bash
   python scripts/bump_version.py X.Y.Z
   ```
   This updates all three version files in sync:
   - `pyproject.toml` (line 3)
   - `crates/amplifier-core/Cargo.toml` (line 3)
   - `bindings/python/Cargo.toml` (line 3)
3. Run the E2E smoke test (mandatory since v1.2.5):
   ```bash
   ./scripts/e2e-smoke-test.sh
   ```
   This builds a wheel from local source, installs it in an isolated Docker container alongside
   the real `amplifier` CLI, and runs a real LLM-powered session exercising tool dispatch,
   agent delegation, and recipe execution. It catches:
   - Import/attribute errors in the Rust↔Python bridge
   - Session startup crashes
   - Tool dispatch failures
   - Any Python exception during a real agent loop

   **Requirements:** Docker running, `ANTHROPIC_API_KEY` set (or in `~/.amplifier/keys.env`).
   Takes ~5 minutes. **Do not tag until this passes.**

4. Verify no `[tool.uv.sources]` git overrides for `amplifier-core` exist in downstream repos:
   ```bash
   for repo in amplifier amplifier-app-cli amplifier-foundation; do
     echo "=== $repo ==="
     gh api repos/microsoft/$repo/contents/pyproject.toml --jq '.content' | base64 -d | grep -A2 'amplifier-core.*git' && echo "WARNING: git override found!" || echo "OK"
   done
   ```
   If any repo has a git source override for amplifier-core on main, the PyPI publish will not reach users correctly.

5. Commit, tag, and push:
   ```bash
   git commit -am "chore: bump version to X.Y.Z"
   git tag vX.Y.Z
   git push origin main --tags
   ```
5. The `v*` tag triggers `rust-core-wheels.yml` → builds wheels for all platforms → publishes to PyPI.

Full process details: `docs/CORE_DEVELOPMENT_PRINCIPLES.md` §10 — The Release Gate.

---

## Incident Playbook: When a Broken Version Reaches PyPI

*Added after the v1.2.3/v1.2.4 incidents (March 2026).*

### The Problem

Once a version is published to PyPI, `uv tool install amplifier` and `pip install amplifier-core`
serve it immediately. There is no "rollback" button. For `uv tool install` users specifically,
there is no fast local rollback — users must wait for a fix.

### The Playbook

1. **Yank the broken version on PyPI** (immediately, ~30 seconds):
   - Go to https://pypi.org/manage/project/amplifier-core/release/X.Y.Z/
   - Click "Options" → "Yank release"
   - Add reason: "Broken: [brief description]"

   Yanking tells pip/uv to skip this version for new installs. New `amplifier update`
   invocations will resolve to the last non-yanked version.

2. **Fix forward** — do NOT try to reuse the yanked version number:
   - Fix the bug on `main`
   - Run the E2E smoke test (`./scripts/e2e-smoke-test.sh`)
   - Bump to the next PATCH version
   - Tag + push as normal

3. **Post-mortem**: Add the incident to the history below.

### Incident History

| Version | Date | Root Cause | Impact | Resolution |
|---------|------|-----------|--------|------------|
| v1.0.7→v1.0.8 | 2026-03-03 | RetryConfig break for provider-anthropic | Provider users broken | Emergency hotfix |
| v1.2.3 | 2026-03-16 | `session_state` crash — missing dict field on RustCoordinator | CLI startup crashed | Yanked |
| v1.2.4 | 2026-03-16 | `_tool_dispatch_context` crash — RustCoordinator lacked `__dict__` | All tool dispatch crashed | Yanked |
| v1.2.4 | 2026-03-16 | Version files not bumped before tagging | PyPI publish rejected (400) | Re-tagged |
