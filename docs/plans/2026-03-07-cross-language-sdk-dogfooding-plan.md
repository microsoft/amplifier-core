# Cross-Language SDK Dogfooding — Integration Testing Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Close the gaps between "unit tests pass" and "real developer workflow works" for the cross-language SDK. Wire the Python and TypeScript hosts to actually load WASM modules in real sessions, fix the integration plumbing, build a real WASM module using the developer authoring workflow, and exercise mixed-transport sessions.

**Why this matters:** Phases 2–4 delivered ~1,000 passing tests across Rust, Python, and TypeScript. But the pieces have **never been connected end-to-end in a real session.** A developer writing `{"module": "my-wasm-tool"}` in a config today would get a no-op — the WASM module "loads" but never mounts into the coordinator, so it's not callable. This plan fixes that and proves the full pipeline works.

**Branch:** `dev/cross-language-sdk` (Phases 2–4 already complete, all tests passing)

**Design doc:** This plan is self-contained — no separate design doc. The gaps were identified by inspecting the actual source code against the developer workflow.

---

## Codebase Orientation (Read This First)

You are working in `/home/bkrabach/dev/rust-devrust-core/amplifier-core/` on branch `dev/cross-language-sdk`.

### What's the big picture?

Amplifier is a modular AI agent framework. A **session** loads **modules** (tools, hooks, providers, orchestrators, context managers) and wires them together via a **coordinator**. Modules can be written in Python (loaded via importlib), Rust (compiled to WASM and loaded via wasmtime), or run as gRPC services.

The cross-language SDK (Phases 1–4) built the plumbing to load WASM and gRPC modules alongside Python modules. But right now, the session initialization code (`_session_init.py`) still calls the old Python-only loader directly — it never touches the new cross-language dispatch layer. That's what we're fixing.

### Key files you'll interact with

| File | What it does | Why you care |
|---|---|---|
| `python/amplifier_core/_session_init.py` | Loads all modules when a session starts. Currently calls `loader.load()` 5 times (orchestrator, context, providers, tools, hooks). | **Task 0** — you'll wire this to `loader_dispatch.load_module()` instead. |
| `python/amplifier_core/loader_dispatch.py` | Routes module loading by transport (Python/WASM/gRPC). WASM branch currently returns `_noop_mount` — loads but doesn't mount. | **Task 2** — you'll replace `_noop_mount` with real WASM mounting. |
| `bindings/python/src/lib.rs` | PyO3 bindings. `load_wasm_from_path()` creates a test coordinator and discards the loaded module. | **Task 1** — you'll fix this to work with a real coordinator. |
| `bindings/node/src/lib.rs` | Napi-RS bindings. `resolveModule()` and `loadWasmFromPath()` for TypeScript. | **Task 6** — you'll use these from a TS script. |
| `crates/amplifier-core/src/module_resolver.rs` | Rust resolver: auto-detects transport and module type from a directory path. Works correctly (989 lines, well-tested). | Read-only reference — don't modify. |
| `crates/amplifier-core/src/transport.rs` | `load_wasm_tool()`, `load_wasm_hook()`, etc. Each takes `(&[u8], Arc<Engine>)` and returns `Arc<dyn Trait>`. | You'll call these indirectly via `load_module()`. |
| `crates/amplifier-core/src/coordinator.rs` | `Coordinator` struct with typed mount points (tools, hooks, orchestrator, etc.). | The loaded modules must be mounted here. |
| `crates/amplifier-guest/` | Guest SDK crate for authoring WASM modules in Rust. | **Task 4** — you'll use this to build a real module. |
| `tests/fixtures/wasm/` | 6 pre-compiled `.wasm` test fixtures: `echo-tool.wasm`, `deny-hook.wasm`, `memory-context.wasm`, `auto-approve.wasm`, `echo-provider.wasm`, `passthrough-orchestrator.wasm`. | Used in Tasks 2, 3, 5, 7. |
| `tests/fixtures/wasm/src/` | Source code for each fixture (Rust crates using `amplifier-guest`). | Reference for Task 4. |
| `wit/amplifier-modules.wit` | WIT interface definitions. Package `amplifier:modules@1.0.0`. | Reference only — defines the WASM contract. |

### How `_session_init.py` works today

When a session starts, `initialize_session()` runs this sequence:

```
1. Get or create a ModuleLoader
2. Load orchestrator:  await loader.load(orchestrator_id, config, source_hint=...)
3. Load context:       await loader.load(context_id, config, source_hint=...)
4. Load providers:     for each → await loader.load(module_id, config, source_hint=...)
5. Load tools:         for each → await loader.load(module_id, config, source_hint=...)
6. Load hooks:         for each → await loader.load(module_id, config, source_hint=...)
```

Each `loader.load()` call returns a **mount function**. The mount function is then called with the coordinator: `cleanup = await mount_fn(coordinator)`. This registers the module in the coordinator's mount points (e.g., `coordinator.mount_points["tools"]["my-tool"] = tool_instance`).

The problem: `loader.load()` only knows about Python modules via importlib. It has no concept of WASM or gRPC. The new `loader_dispatch.load_module()` knows about all transports, but nobody calls it.

### How `loader_dispatch.py` works today

```python
async def load_module(module_id, config, source_path, coordinator):
    # 1. Try Rust resolver → get transport type
    # 2. If "grpc" → call load_grpc_module()
    # 3. If "wasm" → call load_wasm_from_path() → return _noop_mount  ← BUG
    # 4. If "python" → fall through to loader.load()
```

The WASM branch calls `load_wasm_from_path()` (Rust via PyO3), which successfully loads the WASM bytes and creates the module instance — but then discards it. The `_noop_mount` function does nothing. The module is never registered in the coordinator.

### Test commands

```bash
# Rust unit tests (no WASM)
cargo test -p amplifier-core

# Rust unit tests (with WASM feature — enables WASM bridges + resolver)
cargo test -p amplifier-core --features wasm

# WASM lib tests only (resolver + bridges)
cargo test -p amplifier-core --features wasm --lib

# Module resolver tests specifically
cargo test -p amplifier-core --features wasm -- module_resolver

# WASM E2E integration tests
cargo test -p amplifier-core --features wasm --test wasm_e2e

# Module resolver E2E tests
cargo test -p amplifier-core --features wasm --test resolver_e2e

# Python tests
cd python && python -m pytest tests/ -x -q

# TypeScript tests (Napi-RS bindings)
cd bindings/node && npm test

# Clippy (lint)
cargo clippy -p amplifier-core --features wasm -- -D warnings
```

**⚠️ Known issue:** `cargo test --features wasm` can hang if another cargo process holds a build lock. If you see a test run stall for more than 30 seconds, kill it, run `cargo clean`, and retry. The tests themselves complete in <5 seconds when run cleanly.

### Fixture helper pattern (copy this for Rust tests)

```rust
fn fixture(name: &str) -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../tests/fixtures/wasm").join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture '{}' not found at {}: {}", name, path.display(), e))
}
```

---

## Task 0: Wire `_session_init.py` → `loader_dispatch.load_module()`

**What:** Replace the 5 `loader.load()` calls in `_session_init.py` with calls to `loader_dispatch.load_module()`. This is a wiring change only — Python modules must continue to work identically because `loader_dispatch` falls through to `ModuleLoader` for Python transport.

**Why:** Without this, WASM and gRPC modules can never load in a real session. The session initialization code doesn't know `loader_dispatch` exists.

**Files:**
- Modify: `python/amplifier_core/_session_init.py` — replace `loader.load()` calls with `loader_dispatch.load_module()`
- Modify: `python/amplifier_core/loader_dispatch.py` — adjust `load_module()` signature if needed to match what `_session_init.py` passes

### Step 1: Write the failing test

Create a test in `python/tests/test_session_init_dispatch.py` that verifies `_session_init.initialize_session()` calls `loader_dispatch.load_module()` instead of `loader.load()`:

```python
"""Test that _session_init routes through loader_dispatch."""
import asyncio
from unittest.mock import AsyncMock, patch


def test_initialize_session_calls_loader_dispatch():
    """Verify that initialize_session uses loader_dispatch.load_module
    instead of directly calling loader.load()."""
    config = {
        "session": {
            "orchestrator": {"module": "loop-basic", "source": "/path/to/orch"},
            "context": {"module": "context-simple", "source": "/path/to/ctx"},
        },
        "tools": [
            {"module": "tool-echo", "source": "/path/to/tool", "config": {}},
        ],
        "providers": [],
        "hooks": [],
    }

    mock_mount = AsyncMock(return_value=None)
    mock_load_module = AsyncMock(return_value=mock_mount)

    mock_coordinator = AsyncMock()
    mock_coordinator.loader = None
    mock_coordinator.register_cleanup = lambda x: None

    with patch(
        "amplifier_core._session_init.load_module", mock_load_module
    ):
        asyncio.get_event_loop().run_until_complete(
            __import__(
                "amplifier_core._session_init", fromlist=["initialize_session"]
            ).initialize_session(config, mock_coordinator, "test-session", None)
        )

    # Should have called load_module 3 times: orchestrator, context, tool
    assert mock_load_module.call_count == 3
```

This test will fail because `_session_init.py` currently calls `loader.load()`, not `loader_dispatch.load_module()`.

### Step 2: Implement

In `_session_init.py`:
1. Add `from .loader_dispatch import load_module` at the top of `initialize_session()`
2. Replace each `await loader.load(module_id, config, source_hint=source)` with `await load_module(module_id, config, source_path=source, coordinator=coordinator)`
3. Keep the `ModuleLoader` creation as a fallback — `loader_dispatch` uses it internally for Python modules

The key signature difference:
- Old: `loader.load(module_id, config, source_hint=source)` — returns a mount function
- New: `load_module(module_id, config, source_path=source, coordinator=coordinator)` — also returns a mount function

Both return mount functions with the same `async def mount(coordinator) -> cleanup_fn` contract, so the rest of the code (`cleanup = await mount_fn(coordinator)`) stays the same.

### Step 3: Verify

```bash
# The new test passes
cd python && python -m pytest tests/test_session_init_dispatch.py -x -q

# ALL existing Python tests still pass (zero regressions)
cd python && python -m pytest tests/ -x -q
```

**Commit message:** `feat(dogfood): wire _session_init.py → loader_dispatch.load_module()`

---

## Task 1: Fix `load_wasm_from_path` PyO3 Binding

**What:** The current `load_wasm_from_path` in `bindings/python/src/lib.rs` creates a `Coordinator::new_for_test()`, loads the WASM module into it, and throws both away — returning only `{"status": "loaded"}`. Fix it to accept a real coordinator and return the loaded module so it can be mounted.

**Why:** Without this, WASM modules load into a throwaway coordinator. Even if `loader_dispatch.py` calls this function, the module ends up in the wrong coordinator — the real session's coordinator never sees it.

**Files:**
- Modify: `bindings/python/src/lib.rs` — fix `load_wasm_from_path` to work with real coordinators
- May add: a new function `load_and_mount_wasm(coordinator, path, module_type)` that loads the WASM bytes, creates the bridge, and mounts it into the coordinator's mount points

### Step 1: Write the failing test

Add a Rust test in `bindings/python/src/lib.rs` (in the `#[cfg(test)] mod tests` block) that documents the expected behavior:

```rust
#[test]
fn load_wasm_from_path_should_accept_coordinator() {
    // This test documents the expected API change.
    // The function should accept a coordinator reference and mount
    // the WASM module into it, not create a throwaway test coordinator.
    //
    // Current behavior: creates Coordinator::new_for_test(), discards it.
    // Expected behavior: accepts PyCoordinator, mounts into its mount points.
    //
    // The actual integration test is in Python (Task 2), but this
    // documents the Rust-side contract.
}
```

The real test happens in Python (Task 2) — but document the contract here.

### Step 2: Implement

Two approaches (choose based on complexity):

**Approach A — New function:** Add `load_and_mount_wasm(coordinator: &PyCoordinator, path: String, module_type: String)` that:
1. Reads the `.wasm` bytes from `path`
2. Creates a `WasmEngine`
3. Calls the appropriate `load_wasm_*` function based on `module_type` (tool, hook, context, etc.)
4. Mounts the resulting `Arc<dyn Trait>` into the coordinator's Python-visible mount points
5. Returns a status dict

**Approach B — Fix existing function:** Modify `load_wasm_from_path` to accept an optional coordinator parameter. If provided, mount into it. If not, use test coordinator (backward compat with existing tests).

The challenge is bridging between the Rust `Arc<dyn Tool>` (or Hook, etc.) and the Python coordinator's mount points (which are Python dicts). You may need to create a thin Python wrapper that delegates calls to the Rust trait object via PyO3.

### Step 3: Verify

```bash
# Rust tests pass
cargo test -p amplifier-python-bindings

# Clippy clean
cargo clippy -p amplifier-python-bindings -- -D warnings
```

**Commit message:** `feat(dogfood): fix load_wasm_from_path to accept real coordinator`

---

## Task 2: Fix `loader_dispatch.py` WASM Mount Bridge

**What:** Replace the `_noop_mount` function in `loader_dispatch.py` with a real bridge that calls the Rust `load_and_mount_wasm()` via PyO3 and mounts the WASM module into the coordinator's mount points.

**Why:** This is the critical gap. Today: WASM module "loads" (Rust creates the module object), but `_noop_mount` does nothing — the module is never registered in the coordinator, so it can't be called. After this fix: the module loads AND mounts, making it callable like any Python module.

**Files:**
- Modify: `python/amplifier_core/loader_dispatch.py` — replace `_noop_mount` with real WASM mounting logic

### Step 1: Write the failing test

Create `python/tests/test_loader_dispatch_wasm.py`:

```python
"""Test that loader_dispatch actually mounts WASM modules."""
import asyncio
import os


def test_wasm_tool_mounts_into_coordinator():
    """Loading a WASM tool via loader_dispatch should mount it
    into the coordinator's tools mount point, not just return a no-op."""
    from amplifier_core.loader_dispatch import load_module

    # Use the echo-tool fixture
    fixture_dir = os.path.join(
        os.path.dirname(__file__),
        "..", "..", "tests", "fixtures", "wasm"
    )

    # Create a mock coordinator with real mount points
    coordinator = MockCoordinator()

    mount_fn = asyncio.get_event_loop().run_until_complete(
        load_module("echo-tool", {}, fixture_dir, coordinator)
    )

    # Mount into the coordinator
    asyncio.get_event_loop().run_until_complete(mount_fn(coordinator))

    # The tool should now be in the coordinator's tools mount point
    assert "echo-tool" in coordinator.mount_points["tools"], \
        "WASM tool was not mounted into coordinator — _noop_mount is still in place"
```

This test will fail because `_noop_mount` doesn't actually mount anything.

### Step 2: Implement

In `loader_dispatch.py`, replace the WASM branch:

```python
if transport == "wasm":
    try:
        from amplifier_core._engine import load_and_mount_wasm

        # load_and_mount_wasm handles: read .wasm → create engine →
        # load bridge → mount into coordinator
        result = load_and_mount_wasm(coordinator, source_path, module_id)
        logger.info(f"[module:mount] {module_id} mounted via WASM: {result}")

        async def _wasm_mount(coord):
            # Already mounted by load_and_mount_wasm above
            pass

        return _wasm_mount
    except ImportError:
        raise NotImplementedError(...)
```

Or, if the mounting needs to happen at mount-time (deferred):

```python
if transport == "wasm":
    try:
        from amplifier_core._engine import load_and_mount_wasm

        async def _wasm_mount(coord):
            result = load_and_mount_wasm(coord, source_path, module_id)
            logger.info(f"[module:mount] {module_id} mounted via WASM: {result}")

        return _wasm_mount
    except ImportError:
        raise NotImplementedError(...)
```

### Step 3: Verify

```bash
# The new test passes
cd python && python -m pytest tests/test_loader_dispatch_wasm.py -x -q

# All existing tests still pass
cd python && python -m pytest tests/ -x -q
cargo test -p amplifier-core --features wasm
```

**Commit message:** `feat(dogfood): replace _noop_mount with real WASM mounting in loader_dispatch`

---

## Task 3: Add `amplifier.toml` to WASM Fixture Directories

**What:** Create `amplifier.toml` files in each WASM fixture's source directory so the manifest-based detection path works alongside the auto-detection path.

**Why:** The module resolver has two detection paths: (1) auto-detect by inspecting `.wasm` file Component Model metadata, and (2) read `amplifier.toml` for an explicit declaration. Path 1 already works. Path 2 has no test fixtures. Both paths should work so developers can choose either approach.

**Files (all new):**
- Create: `tests/fixtures/wasm/src/echo-tool/amplifier.toml`
- Create: `tests/fixtures/wasm/src/deny-hook/amplifier.toml`
- Create: `tests/fixtures/wasm/src/memory-context/amplifier.toml`
- Create: `tests/fixtures/wasm/src/auto-approve/amplifier.toml`
- Create: `tests/fixtures/wasm/src/echo-provider/amplifier.toml`
- Create: `tests/fixtures/wasm/src/passthrough-orchestrator/amplifier.toml`

### Step 1: Write the failing test

Add a Rust test in `crates/amplifier-core/src/module_resolver.rs` (in the test module):

```rust
#[test]
fn resolve_fixture_via_amplifier_toml() {
    // The echo-tool fixture source directory should have an amplifier.toml
    // that declares transport = "wasm" and type = "tool".
    let fixture_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/wasm/src/echo-tool");

    // This should work via the amplifier.toml path (priority 1)
    let manifest = resolve_module(&fixture_src).expect("should resolve via amplifier.toml");
    assert_eq!(manifest.transport, Transport::Wasm);
    assert_eq!(manifest.module_type, ModuleType::Tool);
}
```

This test will fail because the `amplifier.toml` file doesn't exist yet.

### Step 2: Implement

Create each `amplifier.toml` with this format:

```toml
[module]
transport = "wasm"
type = "tool"   # varies per fixture
```

The `type` value for each fixture:

| Fixture | `type` value |
|---|---|
| `echo-tool` | `"tool"` |
| `deny-hook` | `"hook"` |
| `memory-context` | `"context"` |
| `auto-approve` | `"approval"` |
| `echo-provider` | `"provider"` |
| `passthrough-orchestrator` | `"orchestrator"` |

### Step 3: Verify

```bash
# The new test passes
cargo test -p amplifier-core --features wasm -- resolve_fixture_via_amplifier_toml

# All existing resolver tests still pass
cargo test -p amplifier-core --features wasm -- module_resolver
```

**Commit message:** `feat(dogfood): add amplifier.toml to all WASM fixture source directories`

---

## Task 4: Build a "Real" WASM Tool (Calculator)

**What:** Create a slightly useful WASM tool — not a test fixture, but something a developer would actually build — to prove the developer authoring workflow. A calculator tool that evaluates simple math expressions.

**Why:** The test fixtures are minimal echo/passthrough modules. Building a real (if simple) module proves:
- The `amplifier-guest` SDK is usable
- `cargo component build` works for a fresh project
- The resulting `.wasm` binary loads via the resolver
- The tool actually does something when called

**Files:**
- Create: `examples/wasm-modules/calculator-tool/Cargo.toml`
- Create: `examples/wasm-modules/calculator-tool/src/lib.rs`
- Create: `examples/wasm-modules/calculator-tool/amplifier.toml`
- Generated: `examples/wasm-modules/calculator-tool/target/.../calculator_tool.wasm` (build artifact — don't commit the `target/` dir)
- Create: `examples/wasm-modules/calculator-tool.wasm` (committed pre-built binary, like the test fixtures)

### Step 1: Write the failing test

Add a Rust integration test that expects the calculator tool to exist and be loadable:

```rust
#[test]
fn calculator_tool_loads_and_resolves() {
    let wasm_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/wasm-modules/calculator-tool.wasm");

    assert!(wasm_path.exists(), "calculator-tool.wasm not built yet");

    let bytes = std::fs::read(&wasm_path).unwrap();
    let engine = WasmEngine::new().unwrap();
    let coordinator = Arc::new(Coordinator::new_for_test());
    let tool = load_wasm_tool(&bytes, engine.inner(), coordinator).unwrap();

    let spec = futures::executor::block_on(tool.spec());
    assert_eq!(spec.name, "calculator");
}
```

### Step 2: Implement

Create the crate at `examples/wasm-modules/calculator-tool/`:

**`Cargo.toml`:**
```toml
[package]
name = "calculator-tool"
version = "0.1.0"
edition = "2021"

[dependencies]
amplifier-guest = { path = "../../../crates/amplifier-guest" }

[lib]
crate-type = ["cdylib"]
```

**`src/lib.rs`:**
```rust
use amplifier_guest::{Tool, ToolSpec, ToolResult, Value, Param};

struct CalculatorTool;

impl Tool for CalculatorTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "calculator".to_string(),
            description: "Evaluates simple math expressions (+, -, *, /)".to_string(),
            parameters: vec![
                Param {
                    name: "expression".to_string(),
                    description: "Math expression to evaluate (e.g., '2 + 3 * 4')".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
            ],
        }
    }

    fn execute(&self, args: Value) -> ToolResult {
        // Extract expression from args
        let expr = args.get("expression")
            .and_then(|v| v.as_str())
            .unwrap_or("0");

        // Simple evaluation (real implementation would use a parser)
        match eval_simple(expr) {
            Ok(result) => ToolResult::success(format!("{result}")),
            Err(e) => ToolResult::error(format!("Calculation error: {e}")),
        }
    }
}

fn eval_simple(expr: &str) -> Result<f64, String> {
    // Minimal evaluator for "a op b" expressions
    // A real tool would use a proper math parser
    let expr = expr.trim();
    // Try to parse as a plain number first
    if let Ok(n) = expr.parse::<f64>() {
        return Ok(n);
    }
    // Look for operators
    for op in ['+', '-', '*', '/'] {
        if let Some(pos) = expr.rfind(op) {
            if pos == 0 { continue; } // negative number
            let left: f64 = expr[..pos].trim().parse()
                .map_err(|e| format!("Bad left operand: {e}"))?;
            let right: f64 = expr[pos+1..].trim().parse()
                .map_err(|e| format!("Bad right operand: {e}"))?;
            return match op {
                '+' => Ok(left + right),
                '-' => Ok(left - right),
                '*' => Ok(left * right),
                '/' => {
                    if right == 0.0 { Err("Division by zero".into()) }
                    else { Ok(left / right) }
                }
                _ => unreachable!()
            };
        }
    }
    Err(format!("Cannot parse expression: {expr}"))
}

amplifier_guest::export_tool!(CalculatorTool);
```

**`amplifier.toml`:**
```toml
[module]
transport = "wasm"
type = "tool"
```

Build with:
```bash
cd examples/wasm-modules/calculator-tool
cargo component build --release
cp target/wasm32-wasip1/release/calculator_tool.wasm ../calculator-tool.wasm
```

### Step 3: Verify

```bash
# The calculator tool resolves correctly
cargo test -p amplifier-core --features wasm -- calculator_tool_loads_and_resolves

# The module resolver auto-detects it
cargo test -p amplifier-core --features wasm -- module_resolver
```

**Commit message:** `feat(dogfood): add calculator-tool example WASM module`

---

## Task 5: Load WASM Tool in a Python Session

**What:** Create `examples/python-wasm-session.py` — a minimal Python script that creates a session with a WASM tool, initializes it, and verifies the tool is mounted and callable.

**Why:** This is the acid test for the Python host → PyO3 → Rust resolver → wasmtime → WASM tool pipeline. If this works, a real developer can write `{"module": "my-tool", "source": "/path/to/wasm/dir"}` in their config and it Just Works.

**Files:**
- Create: `examples/python-wasm-session.py`

### Step 1: Write the failing test

The example script itself IS the test. Create `examples/python-wasm-session.py`:

```python
#!/usr/bin/env python3
"""Dogfood test: Load a WASM tool in a real Python session.

This script proves the full pipeline:
  Python host → PyO3 → Rust resolver → wasmtime → WASM tool

Run from the amplifier-core directory:
  python examples/python-wasm-session.py
"""
import asyncio
import os
import sys

# Ensure amplifier_core is importable
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))


async def main():
    from amplifier_core._engine import RustSession

    fixture_dir = os.path.join(
        os.path.dirname(__file__), "..", "tests", "fixtures", "wasm"
    )

    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        },
        "tools": [
            {
                "module": "echo-tool",
                "source": fixture_dir,
                "config": {},
            },
        ],
        "providers": [],
        "hooks": [],
    }

    session = RustSession(config)
    await session.initialize()

    # Check that the WASM tool is mounted
    coord = session.coordinator
    tools = coord.mount_points.get("tools", {})
    assert "echo-tool" in tools, f"WASM tool not mounted! Tools: {list(tools.keys())}"
    print(f"✅ WASM tool 'echo-tool' is mounted in the coordinator")

    # Try calling it
    tool = tools["echo-tool"]
    spec = await tool.spec()
    print(f"✅ Tool spec: name={spec.name}, params={len(spec.parameters)}")

    result = await tool.execute({"input": "hello from Python"})
    print(f"✅ Tool result: {result}")

    await session.cleanup()
    print("✅ Session cleaned up successfully")
    print("\n🎉 Full Python → WASM pipeline works!")


if __name__ == "__main__":
    asyncio.run(main())
```

### Step 2: Implement

This task has no code to write beyond the script itself. The implementation work was done in Tasks 0–2. If the script doesn't work, it means Tasks 0–2 have a bug — go back and fix them.

Common failure modes to check:
- `_session_init.py` doesn't pass `source` to `loader_dispatch` → fix Task 0
- `loader_dispatch.py` can't find the `.wasm` file in the fixture dir → check resolver path handling
- `load_and_mount_wasm` doesn't bridge to Python mount points correctly → fix Task 1
- The tool mounts but isn't callable (wrong Python wrapper) → fix Task 2

### Step 3: Verify

```bash
cd /home/bkrabach/dev/rust-devrust-core/amplifier-core
python examples/python-wasm-session.py
```

Expected output:
```
✅ WASM tool 'echo-tool' is mounted in the coordinator
✅ Tool spec: name=echo-tool, params=1
✅ Tool result: ...
✅ Session cleaned up successfully

🎉 Full Python → WASM pipeline works!
```

**Commit message:** `feat(dogfood): add Python WASM session example — proves full pipeline`

---

## Task 6: Load WASM Tool from TypeScript Host

**What:** Create `examples/node-wasm-session.ts` — a minimal TypeScript script that uses the Napi-RS bindings to load a WASM tool and call it.

**Why:** Proves the TypeScript host → Napi-RS → Rust resolver → wasmtime → WASM tool pipeline. This is the second host language, and it must work independently of the Python host.

**Files:**
- Create: `examples/node-wasm-session.ts`

### Step 1: Write the failing test

Create `examples/node-wasm-session.ts`:

```typescript
/**
 * Dogfood test: Load a WASM tool from the TypeScript/Node.js host.
 *
 * Proves the full pipeline:
 *   TypeScript host → Napi-RS → Rust resolver → wasmtime → WASM tool
 *
 * Run from the amplifier-core directory:
 *   npx ts-node examples/node-wasm-session.ts
 *
 * Or compile first:
 *   npx tsc examples/node-wasm-session.ts && node examples/node-wasm-session.js
 */

import * as path from 'path';

// Import from the built Napi-RS bindings
const { resolveModule, loadWasmFromPath } = require('../bindings/node');

async function main() {
  const fixtureDir = path.join(__dirname, '..', 'tests', 'fixtures', 'wasm');

  // Step 1: Resolve the module
  console.log('Resolving echo-tool from fixture directory...');
  const manifest = resolveModule(fixtureDir);
  console.log(`✅ Resolved: transport=${manifest.transport}, type=${manifest.module_type}`);

  // Step 2: Load the WASM module
  console.log('Loading WASM module...');
  const result = loadWasmFromPath(fixtureDir);
  console.log(`✅ Loaded: status=${result.status}, type=${result.module_type}`);

  console.log('\n🎉 Full TypeScript → WASM pipeline works!');
}

main().catch((err) => {
  console.error('❌ Failed:', err);
  process.exit(1);
});
```

### Step 2: Implement

Like Task 5, the implementation was done in prior phases (Napi-RS bindings in Phase 4, Task 9). If this doesn't work, it's a bindings bug — fix in `bindings/node/src/lib.rs`.

### Step 3: Verify

```bash
cd /home/bkrabach/dev/rust-devrust-core/amplifier-core
npx ts-node examples/node-wasm-session.ts
```

Also run the existing Napi-RS test suite to confirm no regressions:

```bash
cd bindings/node && npm test
```

**Commit message:** `feat(dogfood): add TypeScript WASM session example — proves Node.js pipeline`

---

## Task 7: Mixed-Transport Session Test

**What:** Create an integration test that runs a session with **both Python and WASM modules** loaded simultaneously — the ultimate dogfood test.

**Why:** Amplifier sessions will commonly have a mix of Python modules (e.g., orchestrator, provider) and WASM modules (e.g., tools, hooks). This test proves the coordinator can handle modules from different transports in the same session.

**Files:**
- Create: `tests/mixed_transport_e2e.py`

### Step 1: Write the failing test

Create `tests/mixed_transport_e2e.py`:

```python
#!/usr/bin/env python3
"""E2E test: Mixed Python + WASM modules in one session.

This is the ultimate dogfood test. A single session loads:
  - Python orchestrator (loop-basic) — loaded via importlib
  - Python context manager (context-simple) — loaded via importlib
  - WASM tool (echo-tool) — loaded via wasmtime
  - WASM hook (deny-hook) — loaded via wasmtime

All four modules must coexist and be callable in the same session.
"""
import asyncio
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))


async def test_mixed_transport_session():
    from amplifier_core._engine import RustSession

    wasm_fixture_dir = os.path.join(
        os.path.dirname(__file__), "fixtures", "wasm"
    )

    config = {
        "session": {
            "orchestrator": "loop-basic",
            "context": "context-simple",
        },
        "tools": [
            {
                "module": "echo-tool",
                "source": wasm_fixture_dir,
                "config": {},
            },
        ],
        "hooks": [
            {
                "module": "deny-hook",
                "source": wasm_fixture_dir,
                "config": {},
            },
        ],
        "providers": [],
    }

    session = RustSession(config)
    await session.initialize()
    coord = session.coordinator

    # ---- Verify all modules are mounted ----
    # Python orchestrator
    assert coord.mount_points.get("orchestrator") is not None, \
        "Python orchestrator not mounted"
    print("✅ Python orchestrator mounted")

    # Python context manager
    assert coord.mount_points.get("context") is not None, \
        "Python context manager not mounted"
    print("✅ Python context manager mounted")

    # WASM tool
    tools = coord.mount_points.get("tools", {})
    assert "echo-tool" in tools, \
        f"WASM tool not mounted! Tools: {list(tools.keys())}"
    print("✅ WASM echo-tool mounted")

    # WASM hook
    hooks = coord.mount_points.get("hooks", {})
    assert "deny-hook" in hooks, \
        f"WASM hook not mounted! Hooks: {list(hooks.keys())}"
    print("✅ WASM deny-hook mounted")

    # ---- Call the WASM tool ----
    tool = tools["echo-tool"]
    result = await tool.execute({"input": "mixed-transport test"})
    print(f"✅ WASM tool returned: {result}")

    # ---- Fire a hook event ----
    hook_result = await coord.hooks.emit("tool:pre", {"tool": "echo-tool"})
    print(f"✅ Hook pipeline returned: {hook_result}")

    await session.cleanup()
    print("\n🎉 Mixed Python + WASM session works!")


if __name__ == "__main__":
    asyncio.run(test_mixed_transport_session())
```

### Step 2: Implement

This task has no new code to write. It exercises everything built in Tasks 0–6. If it fails, the failure will point to which integration seam is broken.

Possible issues to debug:
- Multiple WASM modules from the same directory: the resolver needs to distinguish `echo-tool.wasm` vs `deny-hook.wasm` in the same `fixtures/wasm/` dir. The `source` path may need to point to a directory containing only one `.wasm` file, or the resolver may need a `module_name` hint.
- Hook mounting: WASM hooks need to register into the `HookRegistry`, not just the mount points dict. Check that the hook bridge registers itself correctly.

### Step 3: Verify

```bash
cd /home/bkrabach/dev/rust-devrust-core/amplifier-core
python tests/mixed_transport_e2e.py
```

Also run the full test battery to confirm nothing is broken:

```bash
cargo test -p amplifier-core --features wasm
cd python && python -m pytest tests/ -x -q
```

**Commit message:** `test(dogfood): add mixed Python + WASM transport session E2E test`

---

## Success Criteria

After all 8 tasks are complete:

| Criterion | How to verify |
|---|---|
| All existing Python tests still pass (zero regressions) | `cd python && python -m pytest tests/ -x -q` |
| All existing Rust tests still pass | `cargo test -p amplifier-core --features wasm` |
| All existing TypeScript tests still pass | `cd bindings/node && npm test` |
| A WASM tool loads and executes in a real Python session | `python examples/python-wasm-session.py` |
| A WASM tool loads and executes from a TypeScript host | `npx ts-node examples/node-wasm-session.ts` |
| Mixed Python + WASM modules work in the same session | `python tests/mixed_transport_e2e.py` |
| A developer can build a new WASM module using `amplifier-guest` + `cargo component build` and it loads automatically | The calculator-tool example proves this |
| `amplifier.toml` manifest path works for all 6 fixture types | `cargo test -- resolve_fixture_via_amplifier_toml` |

## Current Test Counts (baseline — must not decrease)

| Suite | Count |
|---|---|
| Rust lib tests (no WASM) | 390 |
| Rust WASM lib tests | 34 |
| Rust WASM E2E tests | 7 |
| Rust module resolver E2E tests | 14 |
| Python tests | ~465 |
| TypeScript Vitest tests | 64 |

## Dependency Graph

```
Task 0 (wire _session_init) ─┐
Task 1 (fix PyO3 binding) ───┼──→ Task 2 (fix loader_dispatch mount) ──→ Task 5 (Python session)
                              │                                          Task 7 (mixed transport)
Task 3 (amplifier.toml) ─────┘
Task 4 (calculator tool) ──────────────────────────────────────────────→ Task 5 (can use calculator)
Task 6 (TypeScript session) — independent after Phase 4 Napi-RS bindings
```

Tasks 0, 1, 3, 4, and 6 can be worked on in parallel.
Tasks 2 and 5 depend on Tasks 0 and 1.
Task 7 depends on Task 2 (and transitively on 0 and 1).