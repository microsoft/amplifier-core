# Migrating to amplifier-core 2.0

This release combines rich tool-result content, native session metadata, model
pricing and precise usage cost, canonical source-module imports, and ordered
hook context injections. Existing JSON/Python payloads can omit the new optional
fields. The major version reflects changes to public Rust construction and
conversion APIs and stricter module identity checks.

## Upgrade the CLI before Core

Use an amplifier-app-cli version containing the configured-source credential
preflight and import-free provider banner. Older CLI preflight can import a
provider from a different checkout before Core sees the configured source. Core
2.0 rejects that conflicting package identity rather than executing mixed code.

Install and validate the CLI companion first; then upgrade Core using a newly
built native wheel. Do not substitute only Python source while retaining an old
native extension. Restart workers after updating their environment so already
imported modules cannot outlive the installed version.

## Rust callers

- `ToolResult` has a new `content` field. Prefer `ToolResult::new(...)`, or add
  `content: None` to existing struct literals. Construct rich content through
  `ToolResult::normalize_content`; `ToolResultContent` keeps its vector private
  so invalid content cannot bypass validation.
- Conversions between the public Rust and generated protobuf `ToolResult` now
  implement `TryFrom`. Replace `.into()` with `.try_into()` and propagate or
  handle `ToolResultContentError`.
- Add `pricing: None` to existing `ModelInfo` literals, `cost_usd: None` to
  `Usage` literals, and `context_injections: Vec::new()` to `HookResult` literals.
  `HookResult` and `ToolResult` also support default-based construction.
- `Usage.cost_usd` is an optional decimal string. Preserve its precision instead
  of converting it through a floating-point number.

## Rich tool results

The optional `ToolResult.content` carries an ordered, non-empty list of text and
base64 image blocks. Only the supported canonical block shapes are accepted;
URL images and unrelated message-block variants are rejected. Legacy
`success`, `output`, and `error` fields remain available. Callers should handle
validation failures rather than dropping invalid content silently.

## Source-module identity

Source modules import under their canonical Python package names, including
relative and sibling imports. Do not preload the same package from a different
checkout, or mutate `sys.modules` to hide a conflict. Resolve and activate the
configured source before importing provider metadata. A worker restart is
required when changing the source of a package already imported in that worker.

## Ordered hook injections

`HookResult.context_injections` is the authoritative ordered list when present.
Each `ContextInjection` has its own content, role, lifetime, placement flag, and
registry-bound provenance. Consumers must process each item once; do not also
apply the scalar compatibility projection.

The native coordinator validates the batch, persists durable items in order,
and returns only ephemeral items for the orchestrator to apply to the current
request. An all-durable result is consumed and returns `continue`. Legacy scalar
hook results remain accepted. Custom loops that bypass the coordinator must
honor each item's lifetime and placement rather than flattening the whole batch.

## Release ordering

Land the backwards-compatible CLI companion first, validate the combined Core
source with current consumers, and then land the Core carrier. Publishing the
version tag and native wheels is a separate release action. A source merge
alone is not evidence that a running host or worker has upgraded.
