# Amplifier Design Philosophy

**Purpose**: Complete design philosophy for Amplifier - principles, patterns, and decision frameworks that guide all development.

**Use**: When requirements are unclear, use this document and the codebase to make correct, aligned decisions.

---

## Core Principles

**1. Mechanism, Not Policy**
- Kernel provides capabilities and stable contracts
- Modules decide behavior (which provider, how to orchestrate, what to log)
- If two teams might want different behavior → module, not kernel

**2. Ruthless Simplicity**
- KISS taken to heart: as simple as possible, but no simpler
- Minimize abstractions - every layer must justify existence
- Start minimal, grow as needed (avoid future-proofing)
- Code you don't write has no bugs

**3. Small, Stable, Boring Kernel**
- Kernel changes rarely, maintains backward compatibility always
- Easy to reason about by single maintainer
- Favor deletion over accretion in kernel
- Innovation happens at edges (modules)

**4. Modular Design (Bricks & Studs)**
- Each module = self-contained "brick" with clear responsibility
- Interfaces = "studs" that allow independent regeneration
- Prefer regeneration over editing (rebuild from spec, don't line-edit)
- Stable contracts enable parallel development

**5. Event-First Observability**
- If it's important → emit a canonical event
- If it's not observable → it didn't happen
- One JSONL stream = single source of truth
- Hooks observe without blocking

**6. Text-First, Inspectable**
- Human-readable, diffable, versionable representations
- JSON schemas for validation
- No hidden state, no magic globals
- Explicit > implicit

---

## The Linux Kernel Decision Framework

Use Linux kernel as a metaphor when decisions are unclear.

### Metaphor Mapping

| Linux Concept | Amplifier Analog | Decision Guidance |
|---------------|------------------|-------------------|
| **Ring 0 kernel** | `amplifier-core` | Export mechanisms (mount, emit), never policy. Keep tiny & boring. |
| **Syscalls** | Session operations | Few and sharp: `create_session()`, `mount()`, `emit()`. Stable ABI. |
| **Loadable drivers** | Modules (providers, tools, hooks, orchestrators) | Compete at edges; comply with protocols; regeneratable. |
| **Signals/Netlink** | Event bus / hooks | Kernel emits lifecycle events; hooks observe; non-blocking. |
| **/proc & dmesg** | Unified JSONL log | One canonical stream; redaction before logging. |
| **Capabilities/LSM** | Approval & capability checks | Least privilege; deny-by-default; policy at edges. |
| **Scheduler** | Orchestrator modules | Swap strategies by replacing module, not changing kernel. |
| **VM/Memory** | Context manager | Deterministic compaction; emit `context:*` events. |

### Decision Playbook

When requirements are vague:

1. **Is this kernel work?**
   - If it selects, optimizes, formats, routes, plans → **module** (policy)
   - Kernel only adds mechanisms many policies could use
   - **Litmus test**: Could two teams want different behavior? → Module

2. **Do we have two implementations?**
   - Prototype at edges first
   - Extract to kernel only after ≥2 modules converge on the need

3. **Prefer regeneration**
   - Keep contracts stable
   - Regenerate modules to new spec (don't line-edit)

4. **Event-first**
   - Important actions → emit canonical event
   - Hooks observe without blocking

5. **Text-first**
   - All diagnostics → JSONL
   - External views derive from canonical stream

6. **Ruthless simplicity**
   - Fewer moving parts wins
   - Clearer failure modes wins

---

## Kernel vs Module Boundaries

### Kernel Responsibilities (Mechanisms)

**What kernel does**:
- Stable contracts (protocols, schemas)
- Module loading/unloading (mount/unmount)
- Event dispatch (emit lifecycle events)
- Capability checks (enforcement mechanism)
- Minimal context plumbing (session_id, request_id)

**What kernel NEVER does**:
- Select providers or models (policy)
- Decide orchestration strategy (policy)
- Choose tool behavior (policy)
- Format output or pick logging destination (policy)
- Make product decisions (policy)

### Module Responsibilities (Policies)

**Providers**: Which model, what parameters
**Tools**: How to execute capabilities
**Orchestrators**: Execution strategy (basic, streaming, planning)
**Hooks**: What to log, where to log, what to redact
**Context**: Compaction strategy, summarization approach
**Agents**: Configuration overlays for sub-sessions

### Evolution Without Breaking Modules

**Additive evolution**:
- Add optional capabilities (feature negotiation)
- Extend schemas with optional fields
- Provide deprecation windows for removals

**Two-implementation rule**:
- Need from ≥2 independent modules before adding to kernel
- Proves the mechanism is actually general

**Backward compatibility is sacred**:
- Kernel changes must not break existing modules
- Clear deprecation notices + dual-path support
- Long sunset periods

---

## Module Design: Bricks & Studs

### The LEGO Model

Think of software as LEGO bricks:

**Brick** = Self-contained module with clear responsibility
**Stud** = Interface/protocol where bricks connect
**Blueprint** = Specification (docs define target state)
**Builder** = AI generates code from spec

### Key Practices

1. **Start with the contract** (the "stud")
   - Define: purpose, inputs, outputs, side-effects, dependencies
   - Document in README or top-level docstring
   - Keep small enough to hold in one prompt

2. **Build in isolation**
   - Code, tests, fixtures inside module directory
   - Only expose contract via `__all__` or interface file
   - No other module imports internals

3. **Regenerate, don't patch**
   - When change needed inside brick → rewrite whole brick from spec
   - Contract change → locate consumers, regenerate them too
   - Prefer clean regeneration over scattered line edits

4. **Human as architect, AI as builder**
   - Human: Write spec, review behavior, make decisions
   - AI: Generate brick, run tests, report results
   - Human rarely reads code unless tests fail

### Benefits

- Each module independently regeneratable
- Parallel development (different bricks simultaneously)
- Multiple variants possible (try different approaches)
- AI-native workflow (specify → generate → test)

---

## Implementation Patterns

### Vertical Slices

- Implement complete end-to-end paths first
- Start with core user journeys
- Get data flowing through all layers early
- Add features horizontally only after core flows work

### Iterative Development

- 80/20 principle: high-value, low-effort first
- One working feature > multiple partial features
- Validate with real usage before enhancing
- Be willing to refactor early work as patterns emerge

### Testing Strategy

- Emphasis on integration tests (full flow)
- Manual testability as design goal
- Critical path testing initially
- Unit tests for complex logic and edge cases
- Testing pyramid: 60% unit, 30% integration, 10% end-to-end

**What to test**:
- ✅ Runtime invariants (catch real bugs)
- ✅ Edge cases (boundary conditions)
- ✅ Integration behavior (full flow)
- ❌ Things obvious from reading code (constant values)
- ❌ Redundant with code inspection

### Error Handling

- Handle common errors robustly
- Log detailed information for debugging
- Provide clear error messages to users
- Fail fast and visibly during development
- Never silent fallbacks that hide bugs

### Simplicity Guidelines

**Start minimal**:
- Begin with simplest implementation meeting current needs
- Don't build for hypothetical future requirements
- Add complexity only when requirements demand it

**Question everything**:
- Does this abstraction justify its existence?
- Can we solve this more directly?
- What's the maintenance cost?

**Areas to embrace complexity**:
- Security (never compromise fundamentals)
- Data integrity (consistency and reliability)
- Core user experience (smooth primary flows)
- Error visibility (make problems diagnosable)

**Areas to aggressively simplify**:
- Internal abstractions (minimize layers)
- Generic "future-proof" code (YAGNI)
- Edge case handling (common cases first)
- Framework usage (only what you need)
- State management (keep simple and explicit)

---

## Decision Framework

When faced with implementation decisions, ask:

1. **Necessity**: "Do we actually need this right now?"
2. **Simplicity**: "What's the simplest way to solve this?"
3. **Directness**: "Can we solve this more directly?"
4. **Value**: "Does the complexity add proportional value?"
5. **Maintenance**: "How easy will this be to understand later?"

### Library vs Custom Code

**Evolution pattern** (both valid):
- Start simple → Custom code for basic needs
- Growing complexity → Switch to library when requirements expand
- Hitting limits → Back to custom when outgrowing library

**Questions to ask**:
- How well does this library align with actual needs?
- Are we fighting the library or working with it?
- Is integration clean or requiring workarounds?
- Will future requirements stay within library's capabilities?

**Stay flexible**: Keep library integration minimal and isolated so you can switch approaches when needs change.

---

## Anti-Patterns (What to Resist)

### In Kernel Development

❌ Adding defaults or config resolution inside kernel
❌ File I/O or search paths in kernel (app layer responsibility)
❌ Provider selection, orchestration strategy, tool routing (policies)
❌ Logging to stdout or private files (use unified JSONL only)
❌ Breaking backward compatibility without migration path

### In Module Development

❌ Depending on kernel internals (use protocols only)
❌ Inventing ad-hoc event names (use canonical taxonomy)
❌ Private log files (write via `context.log` only)
❌ Failing to emit events for observable actions
❌ Crashing kernel on module failure (non-interference)

### In Design

❌ Over-general modules trying to do everything
❌ Copying patterns without understanding rationale
❌ Optimizing the wrong thing (looks over function)
❌ Over-engineering for hypothetical futures
❌ Clever code over clear code

### In Process

❌ "Let's add a flag in kernel for this use case" → Module instead
❌ "We'll break the API now; adoption is small" → Backward compat sacred
❌ "We'll add it to kernel and figure out policy later" → Policy first at edges
❌ "This needs to be in kernel for speed" → Prove with benchmarks first

---

## Governance & Evolution

### Kernel Changes

**High bar, low velocity**:
- Kernel PRs: tiny diff, invariant review, tests, docs, rollback plan
- Releases are small and boring
- Large ideas prove themselves at edges first

**Acceptance criteria**:
- ✅ Implements mechanism (not policy)
- ✅ Evidence from ≥2 modules needing it
- ✅ Preserves invariants (non-interference, backward compat)
- ✅ Interface small, explicit, text-first
- ✅ Tests and docs included
- ✅ Retires equivalent complexity elsewhere

### Module Changes

**Fast lanes at the edges**:
- Modules iterate rapidly
- Kernel doesn't chase module changes
- Modules adapt to kernel (not vice versa)
- Compete through better policies

---

## Quick Reference

### For Kernel Work

**Questions before adding to kernel**:
1. Is this a mechanism many policies could use?
2. Do ≥2 modules need this?
3. Does it preserve backward compatibility?
4. Is the interface minimal and stable?

**If any "no"** → Prototype as module first

### For Module Work

**Module author checklist**:
- [ ] Implements protocol only (no kernel internals)
- [ ] Emits canonical events where appropriate
- [ ] Uses `context.log` (no private logging)
- [ ] Handles own failures (non-interference)
- [ ] Tests include isolation verification

### For Design Decisions

**Use the Linux kernel lens**:
- Scheduling strategy? → Orchestrator module (userspace)
- Provider selection? → App layer policy
- Tool behavior? → Tool module
- Security policy? → Hook module
- Logging destination? → Hook module

**Remember**: If two teams might want different behavior → Module, not kernel.

---

## Summary

**Amplifier succeeds by**:
- Keeping kernel tiny, stable, boring
- Pushing innovation to competing modules
- Maintaining strong, text-first contracts
- Enabling observability without opinion
- Trusting emergence over central planning

**The center stays still so the edges can move fast.**

Build mechanisms in kernel. Build policies in modules. Use Linux kernel as your decision metaphor. Keep it simple, keep it observable, keep it regeneratable.

**When in doubt**: Could another team want different behavior? If yes → Module. If no → Maybe kernel, but prove with ≥2 implementations first.
