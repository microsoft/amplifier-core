#!/usr/bin/env bash
set -euo pipefail

# E2E Smoke Test for amplifier-core
# Runs a real amplifier session in an isolated Docker container
# with the locally-built amplifier-core wheel, using real LLM providers.
#
# Prerequisites:
#   - Docker installed and running
#   - ANTHROPIC_API_KEY set in environment (or in ~/.amplifier/keys.env)
#   - maturin installed (pip install maturin)
#
# Usage:
#   ./scripts/e2e-smoke-test.sh              # Build wheel + run test
#   ./scripts/e2e-smoke-test.sh --skip-build # Use existing wheel in dist/
#
#   # Test with local repo overrides (for cross-repo changes):
#   ./scripts/e2e-smoke-test.sh \
#       --local-source /path/to/amplifier-app-cli \
#       --local-source /path/to/amplifier-foundation
#
# Environment variables:
#   SMOKE_PROMPT     Override the default test prompt
#   SMOKE_TIMEOUT    Override the timeout in seconds (default: 180)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WHEEL_DIR="$REPO_DIR/dist"
CONTAINER_NAME="amplifier-e2e-smoke-$$"
SKIP_BUILD=false
SMOKE_PROMPT="${SMOKE_PROMPT:-Ask recipe author to run one of its example recipes}"
TIMEOUT_SECONDS="${SMOKE_TIMEOUT:-180}"
LOCAL_SOURCES=()

# Colors (defined early so fail() works during arg parsing)
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { echo -e "${YELLOW}[smoke-test]${NC} $*"; }
info() { echo -e "${CYAN}[smoke-test]${NC} $*"; }
pass() { echo -e "${GREEN}[PASS]${NC} $*"; }
fail() { echo -e "${RED}[FAIL]${NC} $*"; exit 1; }

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build) SKIP_BUILD=true; shift ;;
        --local-source)
            [[ -z "${2:-}" ]] && fail "--local-source requires a path argument"
            LOCAL_SOURCES+=("$2"); shift 2 ;;
        --help)
            echo "Usage: $0 [--skip-build] [--local-source /path/to/repo ...]"
            echo ""
            echo "Options:"
            echo "  --skip-build               Use existing wheel in dist/ instead of rebuilding"
            echo "  --local-source /path/to/repo  Override a dependency with a local checkout."
            echo "                              The repo is copied into the container and installed"
            echo "                              with 'pip install --force-reinstall --no-deps'."
            echo "                              Can be specified multiple times."
            echo "                              For bundles with modules in subdirectories, point at"
            echo "                              the module path (e.g., ../my-bundle/modules/my-module)."
            echo ""
            echo "Examples:"
            echo "  # Core-only smoke test (default):"
            echo "  $0"
            echo ""
            echo "  # Cross-repo smoke test with local overrides:"
            echo "  $0 --local-source ../amplifier-app-cli \\"
            echo "     --local-source ../amplifier-foundation \\"
            echo "     --local-source ../amplifier-bundle-modes/modules/hooks-mode"
            echo ""
            echo "Environment variables:"
            echo "  ANTHROPIC_API_KEY  Required (or set in ~/.amplifier/keys.env)"
            echo "  SMOKE_PROMPT       Test prompt (default: 'Ask recipe author to run one of its example recipes')"
            echo "  SMOKE_TIMEOUT      Timeout in seconds (default: 180)"
            exit 0
            ;;
        *) fail "Unknown argument: $1" ;;
    esac
done

cleanup() {
    log "Cleaning up container $CONTAINER_NAME..."
    docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Step 0: Resolve API keys
# ---------------------------------------------------------------------------

# If ANTHROPIC_API_KEY is not set, try to load from ~/.amplifier/keys.env
if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
    KEYS_ENV="$HOME/.amplifier/keys.env"
    if [[ -f "$KEYS_ENV" ]]; then
        log "Loading API keys from $KEYS_ENV..."
        # shellcheck disable=SC1090
        set -a
        source "$KEYS_ENV"
        set +a
    fi
fi

[[ -z "${ANTHROPIC_API_KEY:-}" ]] && fail "ANTHROPIC_API_KEY not set. Set it in your environment or in ~/.amplifier/keys.env"
command -v docker &>/dev/null || fail "Docker not installed or not in PATH"

# ---------------------------------------------------------------------------
# Step 1: Build wheel
# ---------------------------------------------------------------------------

if [[ "$SKIP_BUILD" == "false" ]]; then
    log "Building wheel from local source (this takes ~2 minutes)..."
    mkdir -p "$WHEEL_DIR"
    rm -f "$WHEEL_DIR"/amplifier_core-*.whl
    (cd "$REPO_DIR" && maturin build --release --out "$WHEEL_DIR") \
        || fail "Wheel build failed — check maturin output above"
    log "Wheel build complete."
else
    log "Skipping build (--skip-build). Using existing wheel in $WHEEL_DIR/"
fi

WHEEL=$(ls "$WHEEL_DIR"/amplifier_core-*.whl 2>/dev/null | head -1)
[[ -z "$WHEEL" ]] && fail "No wheel found in $WHEEL_DIR/ — run without --skip-build first"
log "Using wheel: $(basename "$WHEEL")"

# ---------------------------------------------------------------------------
# Step 1b: Pristine-import preflight
# ---------------------------------------------------------------------------
# Catch the v1.4.0 class of bug: wheel requires a runtime dep not declared in
# pyproject.toml, but masked by transitive deps in the polluted CLI install
# environment from Steps 4–5. We install ONLY the wheel into a fresh
# python:3.12-slim image and verify the production import paths succeed.
#
# Required because Step 4's `uv tool install git+microsoft/amplifier@main`
# pulls a full dep closure (including pytest as a transitive) which can hide
# missing runtime deps. A clean end-user `pip install amplifier-core` doesn't
# get that pollution and would fail.
#
# See context/release-mandate.md Incident History #5 (v1.4.0 yank).

log "Pristine-import preflight: wheel must import on bare python:3.12-slim..."
WHEEL_BASENAME="$(basename "$WHEEL")"
docker run --rm \
    -v "$WHEEL":"/tmp/${WHEEL_BASENAME}":ro \
    -e WHEEL_BASENAME="$WHEEL_BASENAME" \
    python:3.12-slim \
    bash -c '
        set -e
        pip install -q "/tmp/${WHEEL_BASENAME}"
        python -c "
import sys
# Defensive: poison pytest so any leak is detected even on images that happen to ship it
sys.modules[\"pytest\"] = None
from amplifier_core.validation import (
    HookValidator, ToolValidator, OrchestratorValidator,
    ProviderValidator, ContextValidator,
)
from amplifier_core.validation.base import check_on_session_ready
import amplifier_core._session_init   # noqa: F401
import amplifier_core.loader          # noqa: F401
import amplifier_core.coordinator     # noqa: F401
import amplifier_core.hooks           # noqa: F401
print(\"pristine import OK\")
"
    ' || fail "Pristine-import preflight failed — wheel has runtime dep not declared in pyproject.toml"

log "Pristine-import preflight passed."

# ---------------------------------------------------------------------------
# Step 2: Create container
# ---------------------------------------------------------------------------

log "Creating isolated Docker container..."
docker run -d --name "$CONTAINER_NAME" \
    -e ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" \
    -e OPENAI_API_KEY="${OPENAI_API_KEY:-}" \
    -e AZURE_OPENAI_API_KEY="${AZURE_OPENAI_API_KEY:-}" \
    python:3.12-slim \
    sleep 3600 \
    || fail "Container creation failed"

info "Container: $CONTAINER_NAME"

# ---------------------------------------------------------------------------
# Step 3: Bootstrap container (git + uv)
# ---------------------------------------------------------------------------

log "Installing dependencies in container (git, uv)..."
docker exec "$CONTAINER_NAME" bash -c "
    apt-get update -qq && apt-get install -y -qq git >/dev/null 2>&1
    pip install -q uv
    echo 'Bootstrap OK'
" || fail "Dependency install failed"

# ---------------------------------------------------------------------------
# Step 4: Install amplifier from git
# ---------------------------------------------------------------------------

log "Installing amplifier from GitHub (amplifier-core from PyPI, CLI+foundation from git)..."
docker exec "$CONTAINER_NAME" bash -c "
    export PATH=/root/.local/bin:\$PATH
    uv tool install git+https://github.com/microsoft/amplifier@main 2>&1 | tail -5
    echo 'Install OK'
" || fail "Amplifier install failed"

# ---------------------------------------------------------------------------
# Step 5: Override amplifier-core with local wheel
# ---------------------------------------------------------------------------

log "Injecting local wheel into container..."
WHEEL_BASENAME=$(basename "$WHEEL")
docker cp "$WHEEL" "$CONTAINER_NAME:/tmp/$WHEEL_BASENAME" \
    || fail "Wheel copy to container failed"
log "Wheel copied as: $WHEEL_BASENAME"

log "Overriding amplifier-core with local wheel..."
OVERRIDE_OUTPUT=$(docker exec "$CONTAINER_NAME" bash -c "
    uv pip install \
        --python /root/.local/share/uv/tools/amplifier/bin/python3 \
        --force-reinstall --no-deps \
        '/tmp/$WHEEL_BASENAME' 2>&1
") || fail "Wheel override failed — uv pip install returned non-zero"

# Confirm uv actually installed the package (not silently skipped / errored)
if ! echo "$OVERRIDE_OUTPUT" | grep -qiE "installed|already satisfied"; then
    echo "$OVERRIDE_OUTPUT"
    fail "Wheel override failed — uv did not report a successful install. See output above."
fi
log "Override output: $(echo "$OVERRIDE_OUTPUT" | tail -3)"

# ---------------------------------------------------------------------------
# Step 5b: Override additional packages with local sources (if any)
# ---------------------------------------------------------------------------

if [[ ${#LOCAL_SOURCES[@]} -gt 0 ]]; then
    log "Injecting ${#LOCAL_SOURCES[@]} local source override(s)..."
    docker exec "$CONTAINER_NAME" mkdir -p /tmp/local-sources
    for LOCAL_SRC in "${LOCAL_SOURCES[@]}"; do
        # Resolve to absolute path
        LOCAL_SRC=$(cd "$LOCAL_SRC" && pwd)
        SRC_NAME=$(basename "$LOCAL_SRC")
        CONTAINER_PATH="/tmp/local-sources/$SRC_NAME"

        [[ -d "$LOCAL_SRC" ]] || fail "Local source not found: $LOCAL_SRC"

        info "  Copying $SRC_NAME -> $CONTAINER_PATH"
        docker cp "$LOCAL_SRC" "$CONTAINER_NAME:$CONTAINER_PATH" \
            || fail "Failed to copy $SRC_NAME into container"

        info "  Installing $SRC_NAME (--force-reinstall --no-deps)..."
        LOCAL_INSTALL_OUT=$(docker exec "$CONTAINER_NAME" bash -c "
            uv pip install \
                --python /root/.local/share/uv/tools/amplifier/bin/python3 \
                --force-reinstall --no-deps \
                '$CONTAINER_PATH' 2>&1
        ") || fail "Failed to install local source: $SRC_NAME"
        log "  $SRC_NAME: $(echo "$LOCAL_INSTALL_OUT" | tail -1)"
    done
    log "All local source overrides installed."
fi

# ---------------------------------------------------------------------------
# Step 6: Verify installed version
# ---------------------------------------------------------------------------

log "Verifying installed version..."
INSTALLED_VERSION=$(docker exec "$CONTAINER_NAME" bash -c "
    export PATH=/root/.local/bin:\$PATH
    amplifier --version 2>&1
")
info "Installed: $INSTALLED_VERSION"

# Confirm the core version actually changed to our local build
LOCAL_CORE_VER=$(echo "$WHEEL_BASENAME" | grep -oP '(?<=amplifier_core-)[^-]+' || true)
if [[ -n "$LOCAL_CORE_VER" ]]; then
    if ! echo "$INSTALLED_VERSION" | grep -q "core $LOCAL_CORE_VER"; then
        fail "Wheel override did not take effect — expected 'core $LOCAL_CORE_VER' but got: $INSTALLED_VERSION"
    fi
    log "Core version confirmed: $LOCAL_CORE_VER ✓"
fi

# ---------------------------------------------------------------------------
# Step 7: Run the smoke test
# ---------------------------------------------------------------------------

echo ""
log "============================================================"
log " SMOKE TEST START"
log " Prompt: '$SMOKE_PROMPT'"
log " Timeout: ${TIMEOUT_SECONDS}s"
log "============================================================"
echo ""

# Run the smoke test; capture output even if timeout exits non-zero
SMOKE_EXIT_CODE=0
SMOKE_OUTPUT=$(docker exec "$CONTAINER_NAME" bash -c "
    export PATH=/root/.local/bin:\$PATH
    timeout $TIMEOUT_SECONDS amplifier run '$SMOKE_PROMPT' 2>&1
" 2>&1) || SMOKE_EXIT_CODE=$?

# ---------------------------------------------------------------------------
# Step 8: Evaluate results
# ---------------------------------------------------------------------------

echo ""
echo "============================================================"
echo " SMOKE TEST OUTPUT (last 40 lines):"
echo "============================================================"
echo "$SMOKE_OUTPUT" | tail -40
echo "============================================================"
echo ""

# Check for Python exceptions / attribute errors (hard failures)
ERROR_PATTERNS="Traceback|TypeError|AttributeError|no attribute|object has no attribute|ImportError|ModuleNotFoundError|RuntimeError|KeyError|ValueError"
if echo "$SMOKE_OUTPUT" | grep -qE "$ERROR_PATTERNS"; then
    echo "============================================================"
    echo " ERRORS DETECTED:"
    echo "============================================================"
    echo "$SMOKE_OUTPUT" | grep -E "$ERROR_PATTERNS" | head -20
    echo "============================================================"
    fail "Smoke test FAILED — Python exceptions detected in output"
fi

# Check for tool failures (the exact pattern that caught our bugs)
TOOL_FAILURE_COUNT=$(echo "$SMOKE_OUTPUT" | grep -cE "Tool .+ failed:" || true)
if [[ "$TOOL_FAILURE_COUNT" -gt 0 ]]; then
    echo "============================================================"
    echo " TOOL FAILURES DETECTED ($TOOL_FAILURE_COUNT):"
    echo "============================================================"
    echo "$SMOKE_OUTPUT" | grep -E "Tool .+ failed:" | head -10
    echo "============================================================"
    fail "Smoke test FAILED — $TOOL_FAILURE_COUNT tool failure(s) detected"
fi

# Check for timeout (exit code 124 from the 'timeout' command)
if [[ "$SMOKE_EXIT_CODE" -eq 124 ]]; then
    fail "Smoke test TIMED OUT after ${TIMEOUT_SECONDS}s — increase SMOKE_TIMEOUT or investigate"
fi

# If we got here: no exceptions, no tool failures, no timeout
echo ""
pass "========================================================"
pass " SMOKE TEST PASSED"
pass " $INSTALLED_VERSION"
pass " No crashes, no tool failures, no timeout"
pass "========================================================"
echo ""
