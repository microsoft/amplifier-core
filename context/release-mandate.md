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

## Pre-Merge Gate: Proof of Release Readiness

*Added after the v1.2.3/v1.2.4 incidents and the PR #63 round-3 review (April 2026).*

### The Rule

**For `amplifier-core` only, version bump and release verification land in the PR, not after it.** Merge is release. The PR must prove it is ready to publish to PyPI before the merge button is pushed.

### Why This Rule Exists

The existing "every merge triggers a release" mandate created a dangerous window between merge and tag-push where:

- Version files could be out of sync (the v1.2.4 incident)
- The Rust↔Python FFI boundary could be broken in ways unit tests didn't catch (the v1.2.3 incident)
- Intermediate-state debt accumulated in Python fallback shims "until next wheel build" (observed in PR #63)
- Post-merge CI failures required yanking from PyPI instead of preventing the bad publish

Elevating the bar to pre-merge means the PR itself proves end-to-end readiness. No follow-up, no window, no yanks.

### Scope

This rule applies **only to `amplifier-core`** — the sole ecosystem repo published to PyPI. Downstream repos (modules, bundles, foundation, apps) install from git and don't face this constraint.

### What the PR Must Include

Every PR to `amplifier-core` that changes code shipped in the wheel must include:

1. **Version bump via atomic script** — `python scripts/bump_version.py X.Y.Z` applied to all three version files (`pyproject.toml`, `crates/amplifier-core/Cargo.toml`, `bindings/python/Cargo.toml`)

2. **Rust/Python symmetry for kernel primitives** — any new event constant, capability name, or protocol identifier must be defined in both Rust (`events.rs`, etc.) AND Python (`events.py`, etc.), with matching membership in both `ALL_EVENTS` lists. No "Python fallback shim until next wheel build" — the wheel build is part of the PR.

3. **Freshly built wheel** — the PR must include any regenerated binding artifacts (via `maturin develop` locally; CI verifies via `maturin build`). The Python side imports from the Rust binding via `amplifier_core._engine`, not via Python literals.

4. **E2E smoke test result** — `./scripts/e2e-smoke-test.sh` run locally on the branch, with the output posted in the PR. CI should run it automatically where environment permits.

5. **No `[tool.uv.sources]` git overrides for `amplifier-core`** in downstream repos (the existing step 4 from the post-merge checklist, elevated here).

### Who Merges

**The core owner merges.** Not the PR author, not a delegate, not a reviewer. The core owner verifies the PR is green on all pre-merge gates and pushes the merge button, which triggers the existing `rust-core-wheels.yml` workflow to build wheels and publish to PyPI.

This creates a single accountable decision point: the core owner's merge click is the release commit. There is no intermediate state.

### Relationship to the Existing Post-Merge Checklist

The existing checklist (§"The Checklist (Every Merge)") still applies — the version bump, E2E smoke test, and tag push remain required. The change is their **timing**: these gates move from "immediately post-merge" to "proven in the PR, automatic on merge."

### Relationship to the Incident Playbook

Yanking from PyPI remains the recovery path if something does reach the registry broken. The pre-merge gate is the prevention layer; the yank playbook is the recovery layer. Both stay.

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
