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

## 5. Proto as Source of Truth

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

## 6. Testing Philosophy

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

## 7. What NOT to Do

| Anti-pattern | Why |
|-------------|-----|
| Add Python-only features to the kernel | The kernel is Rust. Python-specific behavior belongs in the PyO3 bridge or in Python wrapper classes. |
| Use `unsafe` without a justifying comment | `unsafe` exists for FFI boundaries (PyO3). Every other use requires explicit justification. |
| Add dependencies without measuring compile-time impact | Run `cargo build --timings` before and after. Every dependency adds to CI and contributor build times. |
| Break the PyO3 bridge contract | The switchover tests exist for a reason. If they fail, you've broken the Python ecosystem. |
| Use grep when LSP can answer the question | Grep finds text. LSP finds truth. Especially important in a codebase with generated code where the same type name appears in both hand-written and generated forms. |
| Leave dead code | The compiler warns about it. Listen. Dead code is context poison for AI agents working on this repo. |
| Use `unwrap()` in production code paths | Use `?`, `.ok_or()`, `.unwrap_or_default()`, or explicit error handling. `unwrap()` is acceptable in tests and in provably-safe contexts with a comment explaining why. |
| Use macro-heavy abstractions that break LSP | rust-analyzer must be able to navigate the crate. If your macro makes `goToDefinition` fail, redesign it. |
| Duplicate proto types by hand | If a type exists in proto, use the generated version or convert from it. Don't create a parallel hand-written struct that drifts. |
