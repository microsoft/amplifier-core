/**
 * Node.js WASM Session Example
 *
 * Proves the TypeScript host → Napi-RS → Rust resolver → wasmtime → WASM tool pipeline.
 *
 * Run with:
 *   npx ts-node examples/node-wasm-session.ts
 *   node --experimental-strip-types examples/node-wasm-session.ts
 * Or compile first:
 *   npx tsc --esModuleInterop --module commonjs --target ES2022 --types node examples/node-wasm-session.ts
 *   node examples/node-wasm-session.js
 */

const { resolveModule, loadWasmFromPath } = require('../bindings/node/index.js')
const path = require('path')
const fs = require('fs')
const { tmpdir } = require('os')

// The fixture directory contains multiple .wasm files; readdir order is
// filesystem-dependent. Copy just the echo-tool fixture into a temp directory
// so the resolver deterministically picks it.
const fixtureBase = path.resolve(__dirname, '..', 'tests', 'fixtures', 'wasm')
const fixtureDir = fs.mkdtempSync(path.join(tmpdir(), 'amplifier-node-wasm-'))
fs.copyFileSync(
  path.join(fixtureBase, 'echo-tool.wasm'),
  path.join(fixtureDir, 'echo-tool.wasm')
)

try {
  // Step 1: Resolve the module
  console.log(`Resolving module from: ${fixtureDir}`)
  const manifest = resolveModule(fixtureDir)
  console.log(`  transport: ${manifest.transport}`)
  console.log(`  module_type: ${manifest.moduleType}`)

  // Step 2: Load the WASM module
  console.log(`\nLoading WASM module from: ${fixtureDir}`)
  const status = loadWasmFromPath(fixtureDir)
  console.log(`  status: ${status}`)
  console.log(`  module_type: ${manifest.moduleType}`)

  // Success
  console.log('\nTypeScript → Napi-RS → Rust resolver → wasmtime → WASM tool pipeline: SUCCESS')
} catch (err) {
  console.error(err)
  process.exit(1)
} finally {
  fs.rmSync(fixtureDir, { recursive: true })
}
