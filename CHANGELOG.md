# Changelog

All notable changes to amplifier-core are documented here.

This file follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) conventions.
Version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [1.5.0] — core#70 (M0 Cost Management)

### Added

- `SessionStatus.cost_usd` (`str | None`) — accumulated session cost as a decimal string
  (e.g. `"0.004231"`), populated by the orchestrator from `llm:response` event totals.
  String representation avoids floating-point rounding; parse with `Decimal(status.cost_usd)`
  when arithmetic is needed.

### Breaking changes

- `SessionStatus.estimated_cost` (`float | None`) has been removed. This field was never
  populated by any provider, orchestrator, or module in the ecosystem. All known consumers
  (7 providers, amplifier-foundation, amplifier-app-cli, cost-viewer, all hooks, both
  orchestrators) have been audited and confirmed not to read or write it. External/third-party
  consumers accessing `session_status.estimated_cost` will receive `AttributeError` after
  this release. Use `session_status.cost_usd` (new in this release) for cost data.
  We accept this SemVer risk given the zero-population audit.

---

## [1.4.1] — core#67

### Fixed

- Pristine-import regression: `validation/structural/__init__.py` no longer eagerly imports
  test base classes, eliminating the `ModuleNotFoundError: No module named 'pytest'` crash
  on clean `pip install amplifier-core` installs.
- Added pristine-import preflight to `e2e-smoke-test.sh` (Step 1b) to catch this class of
  regression before tagging.

---

## [1.4.0] — core#63

### Added

- `on_session_ready(coordinator)` optional lifecycle hook — called after all modules across
  all phases have completed `mount()`. Enables cross-module wiring without dual-path patterns.

---
