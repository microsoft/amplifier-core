#!/usr/bin/env bash
# Recompile all WASM test fixtures from source.
#
# Run from the amplifier-core root:
#   bash tests/fixtures/wasm/build-fixtures.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR"
SRC_DIR="$FIXTURES_DIR/src"

echo "=== Building WASM test fixtures ==="

for module_dir in "$SRC_DIR"/*/; do
    module_name=$(basename "$module_dir")
    echo "--- Building $module_name ---"
    (cd "$module_dir" && cargo component build --release)

    # Find the .wasm output
    wasm_file=$(find "$module_dir/target" -name "*.wasm" -path "*/release/*" | head -1)
    if [ -z "$wasm_file" ]; then
        echo "ERROR: No .wasm file found for $module_name"
        exit 1
    fi

    # Copy to fixtures directory with kebab-case name
    cp "$wasm_file" "$FIXTURES_DIR/$module_name.wasm"
    echo "  -> $FIXTURES_DIR/$module_name.wasm ($(wc -c < "$FIXTURES_DIR/$module_name.wasm") bytes)"
done

echo "=== All fixtures built successfully ==="
