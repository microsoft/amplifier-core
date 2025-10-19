# KERNEL_PHILOSOPHY.md
_Last updated: 2025-10-07T16:58:45Z_

> **📌 SOURCE OF TRUTH**
> This document is the **authoritative source** for Amplifier's kernel philosophy and design principles.
> All kernel design decisions should align with these principles.
>
> **Companion docs:** `IMPLEMENTATION_PHILOSOPHY.md`, `MODULAR_DESIGN_PHILOSOPHY.md`, `AMPLIFIER_AS_LINUX_KERNEL.md`
> **Last reviewed:** 2025-10-13
> **Review triggers:** Kernel API changes, new module protocols, philosophical shifts

> **Amplifier's north star:** a tiny, stable kernel that provides only mechanisms; all policies and features live at the edges as replaceable modules. The center stays still so the edges can move fast.

## 0) Purpose of this document
This file complements `IMPLEMENTATION_PHILOSOPHY.md` and `MODULAR_DESIGN_PHILOSOPHY.md`. It captures **Unix & Linux-kernel–inspired guidance** for how we design, evolve, and govern Amplifier’s “kernel” (the ultra‑thin core) **independently of any specific code or architecture choices**. Treat it as the background radiation that informs every design and PR. When trade‑offs are unclear, default to the principles here.

---

## 1) Core tenets (kernel mindset)

1. **Mechanism, not policy.**  
   The kernel exposes _capabilities_ and _stable contracts_. _Decisions about behavior_ belong outside the kernel. If something can plausibly be a policy, it should live in a module, not in core.

2. **Small, stable, and boring.**  
   The kernel is intentionally minimal and changes rarely. It is easy to reason about by a single maintainer. Favor deletion over accretion. Prefer saying “no” to keep the center still.

3. **Don’t break modules (the “don’t break userspace” rule).**  
   Backward compatibility in kernel interfaces is sacred. Additive evolution, clear deprecation, and long sunsets are the norm. Breaking changes to core contracts are an absolute last resort.

4. **Separation of concerns via explicit boundaries.**  
   Design **narrow, well‑documented interfaces**. No hidden backchannels, no implicit globals. If two parts need to talk, define a stable boundary and document the data that crosses it.

5. **Extensibility through composition, not configuration.**  
   New behavior comes from plugging in a different module, not from toggling a giant matrix of flags. Compose building blocks instead of growing conditional logic in core.

6. **Policy lives at the edges.**  
   Scheduling strategies, orchestration styles, provider choices, safety policies, formatting preferences, and logging policies belong in modules. The kernel provides only the hook points and contracts.

7. **Text-first, inspectable surfaces.**  
   Favor human-readable, deterministic, and versionable representations (plain text/JSON) for inputs, outputs, and contracts. If it can be inspected and diffed, it’s friendlier to humans and tools.

8. **Determinism before parallelism.**  
   Prefer simple, deterministic flows over clever concurrency. Optimize for predictability and debuggability first; parallelism can be an _alternative module_ later.

9. **Observability as a built-in mechanism.**  
   The kernel provides the **mechanism** (events/hooks) to observe what happens. _Policies_ for what to record, where to ship it, and how to visualize it live in modules.

10. **Security by construction.**  
    Least privilege, deny-by-default, and non-interference are invariants. The kernel enforces safety **mechanisms** (capability boundaries, approvals, resource limits). Security _policies_ plug in at the edges.

11. **Complexity budgets.**  
    Treat complexity like a scarce resource. Every non-trivial concept in core must “pay rent” with clearly articulated system-wide value. If it doesn’t pull its weight, it doesn’t belong in kernel.

12. **Rough consensus & running code—then abstraction.**  
    Prove ideas with small, working modules first. Only extract or expand kernel contracts after _multiple_ concrete implementations justify them.

---

## 2) What belongs in the kernel (and what does not)

### 2.1 Kernel responsibilities (mechanisms)
- **Stable contracts**: definition of a few core interfaces and invariants.  
- **Lifecycle & coordination**: loading/unloading modules, dispatching events, mediating calls at boundaries.  
- **Capability enforcement**: resource limits, permission checks, graceful degradation/fail-closed behavior.  
- **Minimal context plumbing**: passing identifiers and basic state necessary to make boundaries work.  
- **Observability hooks**: an eventing surface for modules to observe (not decide).

### 2.2 Explicit non-goals (policies)
- Orchestration strategies, heuristics, or plans.  
- Provider/model selection logic.  
- Tool behavior or domain rules.  
- Formatting, UX, or logs-as-policy (destinations, sampling, redaction rules).  
- Business or product defaults.  
- Any feature that can be swapped without rewriting the kernel.

> **Litmus test:** If two reasonable teams could want different behavior, it’s a **policy** → keep it out of kernel.

---

## 3) Invariants (must always hold)

- **Backward compatibility:** Existing modules continue to work across kernel updates, barring explicit, versioned deprecations with migration notes.
- **Non-interference:** A faulty module cannot crash or corrupt the kernel; errors are contained and reported via mechanisms.
- **Bounded side-effects:** Kernel code does not make irreversible external changes as part of coordination.
- **Deterministic semantics:** Given the same inputs at its boundaries, kernel behavior is predictable.
- **Minimal dependencies:** Kernel avoids heavy third-party dependencies and avoids transitive sprawl.
- **Textual introspection:** Every kernel decision that affects modules is observable via a stable, text-first surface (events, status, or errors).

---

## 4) Evolution rules (how the kernel changes)

1. **Additive first.** Extend contracts without breaking them. Prefer optional capabilities and feature negotiation over replacement.  
2. **Two-implementation rule.** Do not promote a new concept into kernel until at least two independent modules have converged on the need.  
3. **Deprecation discipline.** When removal is unavoidable: announce, document migration, support a dual path for a deprecation window, then remove.  
4. **Spec before code.** Kernel changes begin with a short spec: purpose, alternatives, impact on invariants, test strategy, roll-back plan.  
5. **No policy leaks.** If a change drifts toward policy, move it outward into a module or hook.  
6. **Complexity ledger.** Each kernel change updates a running “complexity budget” (additions must justify themselves and retire equivalent complexity elsewhere).

---

## 5) Interface guidance

- **Small & sharp.** Prefer a handful of precise operations to broad, do-everything calls.  
- **Stable schemas.** Version any data shapes crossing kernel boundaries; add fields, don’t repurpose them.  
- **Explicit errors.** Fail closed with actionable, text-first diagnostics. No silent fallbacks that hide bugs.  
- **Capability-scoped.** Pass only the minimum capability a module needs (principle of least authority).  
- **Negotiated features.** Support opt-in capability flags rather than guessing intent.  
- **Text over magic.** Favor explicit parameters and plain formats over implicit global context.

---

## 6) Governance & maintainership

- **Single-throat-to-choke for the kernel.** One lead (or tiny core team) owns acceptance and release of kernel changes.  
- **High bar, low velocity.** Releases are small and boring; large or risky ideas must prove themselves at the edges first.  
- **PR minimums for kernel:** tiny diff, invariant review, failure modes, rollback plan, tests, and docs.  
- **Fast lanes at the edges.** Modules may iterate rapidly; the kernel does not chase them—modules adapt to kernel, not vice versa.  
- **No surprise upgrades.** Semantic versioning and clear release notes; never ship breaking changes under a patch/minor version.

---

## 7) Security & safety posture

- **Deny by default.** Kernel offers no ambient authority; modules must request capabilities explicitly.  
- **Sandbox boundaries.** All calls across boundaries are validated, attributed, and observable.  
- **Non-interference by design.** Failures in modules are isolated; recovery paths are documented; the kernel stays up.  
- **Privacy-first mechanisms.** Provide hooks for redaction/approval, but never decide policy in kernel.

---

## 8) Observability principles (mechanism only)

- **Event-first.** The kernel emits lifecycle and boundary events that modules can observe.  
- **Single source of truth.** Strive for one canonical, structured feed; derived views live outside kernel.  
- **Causality IDs.** Provide session/request/span identifiers so edges can correlate activity end-to-end.  
- **Never block primary flow.** Observability must not jeopardize system progress; on failure, degrade gracefully.

---

## 9) Performance & reliability

- **Predictability over peak throughput.** Favor constant-time/constant-risk paths in kernel.  
- **Graceful degradation.** On resource pressure, shed optional work at the edges; preserve kernel invariants.  
- **Tight feedback loops.** Kernel errors surface immediately with actionable messages; no silent retries.  
- **Measure before tuning.** Evidence-driven optimization; avoid speculative complexity in core.

---

## 10) Contribution checklist (kernel-targeting PRs)

Before proposing a kernel change, ensure you can answer **yes** to all:
- Does it implement a **mechanism** that multiple policies could use?  
- Is there evidence from **≥2 independent modules** that need it?  
- Does it **preserve invariants** (non-interference, backward compatibility, minimal deps)?  
- Is the interface **small, explicit, and text-first** with a versioned schema?  
- Are **tests and docs** included, plus a rollback plan?  
- Did you **retire equivalent complexity** elsewhere (complexity budget neutral or better)?

If any answer is no, prototype it **as a module** first.

---

## 11) Red flags & anti-patterns (what to resist)

- “Let’s add a flag in kernel to cover this product use case.”  
- “We can just pass the whole context through ‘for flexibility’.”  
- “We’ll break the API now; adoption is small.”  
- “We’ll add it to kernel now and figure out policy later.”  
- “This needs to run in parallel inside kernel for speed.”  
- “It’s only one more dependency.”  

When you hear these, pause and route the work to the edges.

---

## 12) North star outcomes

- **Unshakeable center:** a kernel so small and stable that it can be maintained by one person and audited in an afternoon.  
- **Explosive edges:** a flourishing ecosystem of modules that can compete, evolve, and be swapped without touching the center.  
- **Forever-upgradeable:** we ship improvements weekly at the edges while kernel updates remain rare, safe, and boring.

---

### TL;DR
Keep the center tiny and timeless. Export **mechanisms** and **stable contracts**. Push everything else—the **policies**, the **variation**, the **innovation**—to the edges.
