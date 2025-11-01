# Amplifier as Linux Kernel

> **Purpose.** Use the **Linux kernel** as a metaphor to think and decide correctly when details are unclear. Treat Amplifier as “our kernel” and the module ecosystem as “drivers & userspace.” This document stands alone and, with the codebase, should be **sufficient** to take on new work on‑philosophy.

---

## 1) The metaphor at a glance

| Linux concept         | Amplifier analog                                                              | What this means when you’re building                                                    |
| --------------------- | ----------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| **Ring 0 kernel**     | `amplifier-core` (tiny coordinator + events)                                  | Export **mechanisms** (mount, dispatch, emit), never policy. Keep it small & boring.    |
| **Syscalls**          | Kernel **operations/entry points** (create session, mount module, emit event) | Keep them **few and sharp**. Treat as ABI: narrow, stable, text-first errors.           |
| **Loadable drivers**  | **Modules** (providers, tools, hooks, orchestrators, context, agents)         | Compete at edges; comply with Protocols; no reliance on kernel internals.               |
| **VFS mountpoints**   | **Mount points** (`/mnt/providers`, `/mnt/tools`, `/mnt/hooks`, …)            | Think of each module as a device mounted at a path with a stable connector.             |
| **Signals/Netlink**   | **Event bus / hooks**                                                         | Kernel emits lifecycle events; hooks observe; failures never bubble up.                 |
| **/proc & dmesg**     | **Unified JSONL log** (schema v1)                                             | One canonical, structured stream for audit & replay. Redaction runs **before** logging. |
| **Capabilities/LSM**  | **Approvals & capability checks** (deny‑by‑default)                           | Least privilege; policy at edges; kernel enforces mechanism only.                       |
| **Scheduler**         | **Orchestrator module(s)**                                                    | Swap strategies by replacing the orchestrator, not by changing kernel.                  |
| **VM/Memory manager** | **Context manager + compaction**                                              | Deterministic compaction; emit `context:*` events around it.                            |
| **Fork/exec**         | **Session forking / parallel runs (planned)**                                 | Clone plans as new sessions; never mutate kernel to experiment.                         |

**Rule of thumb:** If two teams might want different behavior, **it is userspace policy** → build it as a module. The kernel exposes the studs; modules provide the shapes.

---

## 2) System call surface (Amplifier “syscalls”)

Keep these **minimal and stable**. Prefer **additive** evolution.

- `session_create(plan)` → returns `session_id`
- `mount(point, module, config)` / `unmount(point)`
- `emit(event, data)` — fire lifecycle events (never blocks critical flow)
- `register_hook(event, handler, priority)` — mechanism only; policy lives in handlers
- `get_context()` / `set_context_fragment(key, value)` — minimal state conduit
- (Optional) `fork_session(parent_id)` — safe cloning (future)

**Design notes**

- **Text‑first errors**; deterministic outcomes; no silent fallbacks.
- **No defaults or file I/O** in kernel. Plans and search paths come from the **app layer**.

---

## 3) Driver model: how modules fit

**Providers** (model backends), **Tools** (filesystem, bash, web), **Orchestrators**, **Hooks** (logging, redaction, approval), **Context**, **Agents** — all are **drivers** mounted at stable points. Protocols are the **driver contracts**. Modules may be regenerated at will as long as the connector stays the same.

**Driver author checklist**

- Implements the **Protocol** only (no core internals).
- Emits/consumes **canonical events** where appropriate.
- Uses `context.log` (no private log files). Unified JSONL only.
- Handles its own failures; never crash the kernel (non‑interference).

---

## 4) Filesystem & /proc: event taxonomy and logs

**Canonical events** (think `/proc` fields you can always read):
`session:start/end`, `prompt:submit/complete`, `plan:start/end`, `provider:request/response/error`, `tool:pre/post/error`, `context:pre_compact/post_compact`, `artifact:write/read`, `policy:violation`, `approval:required/granted/denied`.
Emit them consistently from orchestrator/tools/providers/context.

**/proc = JSONL**: Every significant moment becomes one line in the canonical log with schema v1: timestamp, level, schema, ids, event, component, module, status, duration, `data`, `error`. Redaction runs **before** logging. One file; everything funnels through it.

---

## 5) Security & capabilities (LSM analogy)

Kernel provides **capability checks and approvals** as **mechanisms** (no policy baked in). Modules request capabilities (e.g., `filesystem.write`); approval hooks decide policy; kernel enforces decision and emits `policy:*` events. Deny by default; least privilege; non‑interference.

---

## 6) Scheduling & execution

Scheduling strategies are **orchestrator policy**. The kernel only coordinates boundaries and events. Want planning, parallelism, or agents‑as‑processes? Ship a new orchestrator module; **do not expand kernel**.

---

## 7) Memory management (context & compaction)

Treat tokens like virtual memory. The **context manager** is the VM subsystem; **compaction** is GC. The kernel emits `context:pre_compact/post_compact` so hooks (logging, metrics) can observe without interfering. Keep policies (what to drop, summarization strategy) in context modules.

---

## 8) Decision playbook (when requirements are vague)

1. **Is this kernel work?** If it selects, optimizes, formats, routes, plans, or logs with opinion → **module**. Kernel changes must add **mechanism** that many policies could use.
2. **Do we have two implementations?** If not, prototype at the edge. Only extract to kernel after real convergence.
3. **Prefer regeneration over edits.** Keep connectors stable, regenerate modules to the new spec.
4. **Event‑first.** If it’s important, emit an event and let hooks observe. If it’s not observable, it didn’t happen.
5. **Text‑first, single source of truth.** All diagnostics flow into JSONL; external views derive from it.
6. **Ruthless simplicity.** If two designs solve it, pick the one with fewer moving parts and clearer failure modes.

---

## 9) Concrete mappings (use these names in code)

- **Syscalls** → functions in `amplifier_core.session` and `amplifier_core.coordinator` (creation, mount, emit).
- **Drivers** → Python packages with `project.entry-points."amplifier.modules"`; they register at mount points.
- **Events** → string constants under `amplifier_core.events` (or equivalent); do not invent ad‑hoc names.
- **/proc (logs)** → app‑initialized JSONL sink; hooks write via `context.log` only. No alternate sinks in modules.

---

## 10) Anti‑patterns (what to resist)

- Adding defaults or file I/O **inside** kernel.
- Logging to stdout or a private file from a module. Use unified JSONL only.
- Expanding kernel to pick providers, plan, or route tools (that’s policy).
- Inventing new event names ad‑hoc; extend the canonical taxonomy instead.
- Over‑general agents; compose focused modules per the “brick” model.

---

## 11) Quick start for newcomers

- **Read**: `KERNEL_PHILOSOPHY.md`, `IMPLEMENTATION_PHILOSOPHY.md` and `MODULAR_DESIGN_PHILOSOPHY.md` (10 minutes).
- **Load**: `AMPLIFIER_CONTEXT_GUIDE.md` and this file into your AI assistant.
- **Do**: For any task, write a **module‑level spec** → instrument **events** → emit **JSONL** → verify via `amplifier logs`.
- **Ask**: “Could another team want a different policy?” If yes, keep it out of the kernel.

---

## 12) Why this metaphor matters

Linux succeeded by keeping a **small, stable kernel** and pushing **innovation to drivers & userspace**. Amplifier mirrors that: a tiny coordinator with strong, text‑first contracts; modules that compete and improve rapidly; and observability that makes everything debuggable. Use this lens to make the **right small change** every time.
