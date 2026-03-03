# Core Development Principles

> This document governs development in the amplifier-core repository. It complements the ecosystem-wide [DESIGN_PHILOSOPHY.md](DESIGN_PHILOSOPHY.md) and [LANGUAGE_PHILOSOPHY.md](https://github.com/microsoft/amplifier-foundation/blob/main/context/LANGUAGE_PHILOSOPHY.md) with principles specific to the Rust kernel, the PyO3 bridge, and the polyglot transport layer.

---

## 1. Why the Kernel Is Rust

The kernel is the stability boundary of the entire Amplifier ecosystem. Every module, every session, every agent interaction passes through it. If the kernel has a bug, every consumer is affected. If the kernel has a type mismatch, every language binding is wrong.

Rust is the kernel language because the compiler is the only code reviewer that can enforce correctness at this scale. This is not about performance — LLM calls take seconds to minutes, and the kernel overhead is negligible. The value of Rust here is:

- **The compiler catches what humans miss.** Ownership, exhaustive matching, lifetime enforcement, and trait bounds eliminate entire categories of bugs that would surface as runtime errors in Python — errors that affect every downstream consumer.
- **Refactoring the kernel is safe.** Change a trait signature and the compiler identifies every implementation, every bridge, every test that needs updating. In a dynamic language, this kind of change produces silent failures that surface weeks later.
- **The kernel can serve as the source of truth.** Rust types are precise, self-documenting, and machine-verifiable. Proto generation, PyO3 bindings, and Napi-RS bindings can all be validated against the Rust types at compile time.

The Rust kernel does not mean the ecosystem is Rust-only. It means the center is as trustworthy as possible so the edges can move fast in any language.

---

## 2. The Polyglot Architecture

The kernel hosts modules written in any language via four transports:

| Transport | Mechanism | When used |
|-----------|-----------|-----------|
| **native** | Rust modules implement traits directly, stored as `Arc<dyn Trait>` | Rust modules (zero overhead) |
| **python** | PyO3 bridge translates between Python objects and Rust types | Python modules (existing ecosystem) |
| **grpc** | 6 gRPC bridges wrap remote services as `Arc<dyn Trait>` | Out-of-process modules in any language |
| **wasm** | wasmtime loads `.wasm` modules in-process | Cross-language portable modules |

**Proto is the source of truth for all contracts.** `proto/amplifier_module.proto` defines every service, every message, every enum. The 6 module traits in `src/traits.rs` and the 6 proto services are intentionally parallel — same operations, same semantics, different representations.

**Transport is invisible to developers.** A module author implements a trait (Rust), a Protocol (Python), or a proto service (any language). The kernel and its bridges handle the rest. No module author should need to know about gRPC, proto definitions, or bridge mechanics.

---

## 3. Semantic Tooling Is Non-Negotiable

Anyone working on amplifier-core — human or AI — must use rust-analyzer (LSP) for code navigation. This is not a suggestion.

- **Use LSP for understanding.** `goToDefinition`, `findReferences`, `incomingCalls`, `hover` — these trace actual code paths. Grep finds text, including dead code, comments, and string literals. LSP finds truth.
- **Validate grep results via LSP.** If you grep for a function name, verify it's on a live call path before building on it.
- **Report tool gaps honestly.** If rust-analyzer isn't available or indexed, say so. Don't fall back to grep and hope.

The crate is designed for semantic navigability:
- Explicit types everywhere — no inference-heavy generic chains that confuse tooling.
- Minimal macro usage — `#[derive]` and `#[tonic::async_trait]` are acceptable; custom proc macros that break LSP are not.
- No `pub use *` re-exports that obscure where symbols originate.

**Dead code is context poison.** The Rust compiler warns about unused code. Listen to it. Unused functions, unreachable branches, and orphaned modules are not harmless — they poison AI understanding and propagate errors through every interaction that touches them.

---

## 4. The PyO3 Bridge Contract

`bindings/python/src/lib.rs` is the compatibility layer between the Rust kernel and the Python ecosystem. It has one inviolable rule:

**Every existing Python import, method signature, and return type must continue to work unchanged.**

The bridge translates — it never leaks. Python consumers see `AmplifierSession`, `ModuleCoordinator`, `HookRegistry`, `CancellationToken` — the same types they always have. The fact that these are now Rust-backed via PyO3 is invisible to them.

When adding new Rust functionality:
1. Add the Rust implementation first (in `crates/amplifier-core/src/`)
2. Expose it via PyO3 in the bridge (`bindings/python/src/lib.rs`)
3. Verify the Python API surface hasn't changed (`uv run pytest tests/ bindings/python/tests/`)
4. The switchover tests in `bindings/python/tests/test_switchover_*.py` are the contract tests — they must always pass

---

## 5. Rust Is the Source of Truth — Language Layers Are Thin Translators

The Rust kernel (`crates/amplifier-core/src/`) is where logic, types, validation, and behavior live. Language-specific layers (PyO3 bindings, future Napi-RS bindings, etc.) are **thin translation layers** — they convert between the host language's types and Rust types. They do not contain business logic, validation rules, or behavioral decisions.

This means:

- **Logic goes in Rust, not in bindings.** If a function validates input, computes a result, or makes a decision, it belongs in the Rust crate. The binding layer calls the Rust function and translates the result — nothing more.
- **Types are defined in Rust (and proto), then projected.** Rust structs and proto messages are the canonical type definitions. Python classes, TypeScript interfaces, and Go structs are projections of these — generated or hand-written translations that must stay in sync.
- **Don't duplicate logic across languages.** If validation logic exists in Rust, the Python layer must NOT reimplement it in Python. Call through to Rust. If both languages need the logic independently (e.g., for a pure-Python fallback), extract the rules to a shared specification (proto or doc) and implement from that spec in both places.
- **Utility code follows the same rule.** If a utility function (string processing, config parsing, path resolution) is needed by multiple languages, implement it once in Rust and expose it via bindings. Don't create parallel implementations in `python/amplifier_core/utils/` and `crates/amplifier-core/src/` that drift apart.

**The decision matrix for where code lives:**

| Question | If yes → | If no → |
|----------|----------|---------|
| Does it contain logic, validation, or computation? | Rust (`crates/amplifier-core/src/`) | — |
| Is it purely type translation (Rust ↔ host language)? | Binding layer (`bindings/python/`, future `bindings/node/`) | — |
| Is it host-language-specific glue (e.g., Python `__init__.py` re-exports, async wrappers)? | Language-specific wrapper (`python/amplifier_core/`) | — |
| Is it a module contract (trait, protocol, service)? | Rust trait + proto definition | — |
| Is it a utility needed by multiple languages? | Rust, exposed via bindings | NOT duplicated per language |

> **Future work:** Some utility code currently lives in `python/amplifier_core/` (validation, module loading helpers) that predates the Rust kernel. As the Rust core matures, this code should migrate to Rust and be exposed via bindings. Track this as incremental work — don't disrupt existing consumers, but don't add NEW Python-only utilities either.

---

## 6. Proto as Source of Truth

`proto/amplifier_module.proto` defines all module contracts. This is the single source of truth shared by:
- Rust generated code (`crates/amplifier-core/src/generated/amplifier.module.rs`)
- Python generated stubs (`python/amplifier_core/_grpc_gen/`)
- Future TypeScript, Go, and C# generated stubs

### Rules

- **Generated code is committed, not gitignored.** The Rust generated file lives in `src/generated/` and is checked into git. This allows building without protoc installed (CI, contributor machines).
- **`build.rs` is graceful.** It checks for protoc availability. If protoc is missing, it uses the committed stubs and emits a cargo warning. If protoc is present, it regenerates.
- **When proto changes, regenerate and commit.** Run `cargo build -p amplifier-core` (with protoc installed), then `python -m grpc_tools.protoc ...` for Python stubs, then commit both.
- **Proto equivalence tests verify sync.** The tests in `src/generated/equivalence_tests.rs` and `tests/test_proto_compilation.py` verify that generated code matches the proto definition.

---

## 7. Testing Philosophy

Each test layer has a distinct purpose:

| Layer | What it verifies | Where |
|-------|-----------------|-------|
| **Rust unit tests** | Structural correctness — types, traits, compilation | `crates/amplifier-core/src/**` (inline `#[cfg(test)]`) |
| **Rust integration tests** | End-to-end paths — gRPC round-trips, native tool execution | `crates/amplifier-core/tests/` |
| **Proto equivalence tests** | Proto expansion matches hand-written Rust types | `src/generated/equivalence_tests.rs` |
| **Python tests** | Behavioral compatibility — the PyO3 bridge works correctly | `tests/`, `bindings/python/tests/` |
| **Switchover tests** | Python API contract — same imports, same behavior after Rust migration | `bindings/python/tests/test_switchover_*.py` |

**The compiler is the first test.** If it compiles and clippy is clean, the structural correctness bar is already met. Tests then verify behavior, not shape.

**Run the full suite before committing:**
```bash
cargo test -p amplifier-core              # Rust
cargo clippy -p amplifier-core -- -D warnings  # Lint
cargo fmt -p amplifier-core --check       # Format
maturin develop && uv run pytest tests/ bindings/python/tests/  # Python
```

---

## 8. Transport Is Invisible to Developers

The kernel hosts modules via four transports (native, python/PyO3, gRPC, WASM). Developers never choose which transport to use — they say `{"module": "tool-bash"}` and the framework resolves the optimal path.

This means:
- **No transport details in public APIs.** The `Tool`, `Provider`, and other traits are the interface. Whether a module is in-process Rust, a gRPC bridge, or a WASM sandbox is hidden behind `Arc<dyn Trait>`.
- **gRPC is infrastructure, not interface.** Module authors never write proto services or start gRPC servers. The bridges and transport dispatch handle this.
- **Proto is the contract definition format**, not a developer-facing API. It defines what the bridges serialize — developers interact with Rust traits and Python Protocols.

---

## 9. The Backward Compatibility Guarantee

Every existing Python module, bundle, and application works unchanged. This is not negotiable.

The kernel is the stability boundary. Modules, apps, and the entire Python ecosystem depend on it. Changes to the kernel must be:
- **Backward compatible** — existing imports, method signatures, and return types must not break.
- **Additive** — new capabilities are added alongside existing ones, not replacing them.
- **Tested against the existing ecosystem** — the switchover tests (`bindings/python/tests/test_switchover_*.py`) are the contract tests.

This guarantee does NOT mean we can't evolve. It means evolution is additive — new traits, new transport options, new language bindings — without disturbing what already works.

**Polyglot is a capability, not a migration.**

---

## 10. The Release Gate: Every Merge Gets a Release

**Every PR merged to `amplifier-core` main MUST be immediately followed by a version bump, release commit, `v{version}` tag, and tag push. No exceptions.**

This rule exists because `amplifier-core` occupies a unique position in the ecosystem: it is the **only repo published to PyPI**. The failure mode is concrete and was observed in production:

- Users install `amplifier-core` from PyPI and get a pinned version (e.g., v1.0.7).
- Downstream modules (`amplifier-module-*`, provider repos) install from git and track `main` directly.
- A PR merges to `main` that changes the API. No release is cut. PyPI still serves v1.0.7.
- Any user who installs or updates a module that tracks the new API now has a version skew. It breaks silently or with a confusing error.

**This happened.** Commit `580ecc0` ("eliminate Python RetryConfig") merged on March 3, 2026 without a release. `provider-anthropic` was updated to use `initial_delay` instead of `min_delay`. All v1.0.7 PyPI users broke immediately. An emergency v1.0.8 hotfix was required.

### Scope: amplifier-core Only

This rule applies **specifically to amplifier-core** because of its PyPI distribution. Other ecosystem repos — `amplifier-module-*`, `amplifier-bundle-*`, `amplifier-app-*`, provider repos — currently use `git+https` references for Python. Individual repo authors choose their own release process for those repos. Do not apply this mandate to them.

### The Release Checklist (Every Merge)

1. **Determine the version increment** (semver rules):
   - PATCH (`X.Y.Z+1`) — bug fixes, no API changes
   - MINOR (`X.Y+1.0`) — additive API additions (new fields, new methods, backward compatible)
   - MAJOR (`X+1.0.0`) — breaking API changes (removed fields, changed signatures)

2. **Bump all three version files atomically** using the script:
   ```bash
   python scripts/bump_version.py X.Y.Z
   ```
   This updates in sync:
   - `pyproject.toml` (line 3)
   - `crates/amplifier-core/Cargo.toml` (line 3)
   - `bindings/python/Cargo.toml` (line 3)

3. **Commit, tag, and push:**
   ```bash
   git commit -am "chore: bump version to X.Y.Z"
   git tag vX.Y.Z
   git push origin main --tags
   ```

4. **Verify CI triggers.** The `v*` tag triggers `rust-core-wheels.yml`, which builds wheels for all platforms (Linux x86/aarch64, macOS, Windows) and publishes to PyPI. The next PR does not start until PyPI publish is confirmed.

### Why the Script Exists

The three version files must stay in sync. Manual edits to individual files are error-prone and caused divergence in the past. `scripts/bump_version.py` reads all three, warns if they are already out of sync (canary for prior manual edits), and writes all three atomically.

---

## 11. What NOT to Do

| Anti-pattern | Why |
|-------------|-----|
| Merge to main without cutting a release | amplifier-core is on PyPI. git HEAD and PyPI diverge immediately. Any module that tracks main and uses the new API will break all PyPI users until a release is cut. See §10. |
| Bump version files individually by hand | The three version files must stay in sync. Manual edits drift. Use `python scripts/bump_version.py X.Y.Z`. |
| Add Python-only features to the kernel | The kernel is Rust. Python-specific behavior belongs in the PyO3 bridge or in Python wrapper classes. |
| Use `unsafe` without a justifying comment | `unsafe` exists for FFI boundaries (PyO3). Every other use requires explicit justification. |
| Add dependencies without measuring compile-time impact | Run `cargo build --timings` before and after. Every dependency adds to CI and contributor build times. |
| Break the PyO3 bridge contract | The switchover tests exist for a reason. If they fail, you've broken the Python ecosystem. |
| Use grep when LSP can answer the question | Grep finds text. LSP finds truth. Especially important in a codebase with generated code where the same type name appears in both hand-written and generated forms. |
| Leave dead code | The compiler warns about it. Listen. Dead code is context poison for AI agents working on this repo. |
| Use `unwrap()` in production code paths | Use `?`, `.ok_or()`, `.unwrap_or_default()`, or explicit error handling. `unwrap()` is acceptable in tests and in provably-safe contexts with a comment explaining why. |
| Use macro-heavy abstractions that break LSP | rust-analyzer must be able to navigate the crate. If your macro makes `goToDefinition` fail, redesign it. |
| Duplicate proto types by hand | If a type exists in proto, use the generated version or convert from it. Don't create a parallel hand-written struct that drifts. |
