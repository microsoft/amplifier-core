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
3. Commit, tag, and push:
   ```bash
   git commit -am "chore: bump version to X.Y.Z"
   git tag vX.Y.Z
   git push origin main --tags
   ```
4. The `v*` tag triggers `rust-core-wheels.yml` → builds wheels for all platforms → publishes to PyPI.

Full process details: `docs/CORE_DEVELOPMENT_PRINCIPLES.md` §10 — The Release Gate.
