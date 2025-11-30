# Documentation Linking Guidelines

**Purpose**: Guide AI tools on maintaining the YAML frontmatter linking system in amplifier-core documentation.

---

## Overview

The `docs/contracts/` directory uses YAML frontmatter to create machine-readable links between documentation and source code. This enables AI tools to navigate the codebase and understand relationships.

**Key principle**: Contract files are **hubs** that reference (not duplicate) source files. The frontmatter creates a discoverable graph for AI navigation.

---

## YAML Frontmatter Pattern

### Contract Files (`docs/contracts/*.md`)

Each contract file has YAML frontmatter at the top:

```yaml
---
contract_type: module_specification
module_type: provider | tool | hook | orchestrator | context
contract_version: 1.0.0
last_modified: YYYY-MM-DD
related_files:
  - path: amplifier_core/interfaces.py#ProtocolName
    relationship: protocol_definition
    lines: START-END
  - path: amplifier_core/models.py#ModelName
    relationship: data_models
  - path: amplifier_core/content_models.py
    relationship: event_content_types
  - path: ../specs/SPECIFICATION_NAME.md
    relationship: detailed_spec
  - path: ../specs/CONTRIBUTION_CHANNELS.md
    relationship: observability
  - path: amplifier_core/testing.py#TestUtilityName
    relationship: test_utilities
canonical_example: https://github.com/microsoft/amplifier-module-TYPE-NAME
---
```

### Relationship Types

| Relationship | Use When |
|-------------|----------|
| `protocol_definition` | Links to Protocol class in interfaces.py |
| `data_models` | Links to Pydantic/dataclass models |
| `request_response_models` | Links to message_models.py for request/response envelopes |
| `event_content_types` | Links to content_models.py for event/streaming types |
| `detailed_spec` | Links to specification docs with implementation details |
| `configuration` | Links to mount plan or config specs |
| `observability` | Links to contribution channels or events docs |
| `test_utilities` | Links to testing.py utilities |

---

## When to Update Frontmatter

### Adding a New Source File

When creating a new source file that relates to a module contract:

1. Identify which contract(s) it relates to
2. Add a `related_files` entry in the contract's frontmatter
3. Use appropriate `relationship` type
4. Include `lines:` if referencing specific code sections

**Example**: Adding `content_models.py`
```yaml
related_files:
  # ... existing entries ...
  - path: amplifier_core/content_models.py
    relationship: event_content_types
```

### Moving or Renaming Files

1. Update all `path:` entries in frontmatter that reference the old path
2. Update `lines:` if line numbers changed significantly
3. Run link checker to verify (when available)

### Adding New Protocols/Models

When adding new protocols to `interfaces.py` or models to `models.py`:

1. Update the `lines:` range in relevant contract frontmatter
2. Add new `related_files` entry if it's a distinct concept

### Creating New Contract Files

When adding a new module type contract:

1. Copy frontmatter structure from existing contract
2. Update `module_type` field
3. Add all relevant `related_files` entries
4. Set `last_modified` to current date
5. Add entry to `docs/contracts/README.md` index

---

## Source of Truth Pattern

The `docs/contracts/README.md` lists authoritative source locations:

```markdown
## Source of Truth

**Protocols are in code**, not docs:

- **Protocol definitions**: `amplifier_core/interfaces.py`
- **Data models**: `amplifier_core/models.py`
- **Message models**: `amplifier_core/message_models.py` (Pydantic for envelopes)
- **Content models**: `amplifier_core/content_models.py` (dataclasses for events)
```

**When to update this list**:
- Adding new model files (e.g., a new `*_models.py`)
- Splitting or consolidating model files
- Changing the canonical location for a concept

---

## File Distinction Guidelines

### message_models.py vs content_models.py

These serve different purposes:

| File | Purpose | Type System |
|------|---------|-------------|
| `message_models.py` | Request/response envelopes | Pydantic models |
| `content_models.py` | Event emission, streaming UI | Dataclasses |

**When referencing in frontmatter**:
- Use `request_response_models` for message_models.py
- Use `event_content_types` for content_models.py

---

## Validation (Future)

A CI check will verify:
1. All `path:` entries point to existing files
2. `lines:` ranges are within file bounds
3. Referenced anchors (`#ClassName`) exist in the file

Until the CI check is implemented, manually verify links when updating.

---

## Quick Reference: Common Updates

### New dataclass/model added
```yaml
# Add to relevant contract frontmatter
related_files:
  - path: amplifier_core/NEW_FILE.py
    relationship: appropriate_type
```

### Protocol signature changed
```yaml
# Update lines range
- path: amplifier_core/interfaces.py#ProtocolName
  relationship: protocol_definition
  lines: NEW_START-NEW_END  # Update these
```

### New canonical example module
```yaml
# Update canonical_example URL
canonical_example: https://github.com/microsoft/amplifier-module-NEW-NAME
```

### Documentation restructured
1. Update all `path:` entries in affected contracts
2. Update `docs/contracts/README.md` if file locations changed
3. Update `last_modified` date in affected contracts

---

## Design Reference

This linking system implements Phase 1 of the Module Developer Experience design:
- See `amplifier-dev/ai_working/designs/MODULE_DEVELOPER_EXPERIENCE.md` for full design rationale
- Key insight: AI-first optimization through discoverable links vs centralized docs
