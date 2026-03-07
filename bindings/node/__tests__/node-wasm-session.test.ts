import { describe, it, expect } from 'vitest'
import { resolveModule, loadWasmFromPath } from '../index.js'
import * as path from 'path'
import * as fs from 'fs'
import * as os from 'os'

/**
 * Tests the TypeScript → Napi-RS → Rust resolver → wasmtime → WASM tool pipeline.
 *
 * Uses a temp directory with a single echo-tool.wasm to ensure deterministic
 * resolution (the fixture directory has multiple .wasm files and readdir order
 * is filesystem-dependent).
 */
describe('Node WASM session pipeline', () => {
  const fixtureBase = path.resolve(__dirname, '..', '..', '..', 'tests', 'fixtures', 'wasm')

  function withEchoToolDir(fn: (dir: string) => void) {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'amplifier-node-wasm-test-'))
    try {
      fs.copyFileSync(
        path.join(fixtureBase, 'echo-tool.wasm'),
        path.join(tmpDir, 'echo-tool.wasm')
      )
      fn(tmpDir)
    } finally {
      fs.rmSync(tmpDir, { recursive: true })
    }
  }

  it('resolveModule returns transport=wasm and moduleType=tool for echo-tool', () => {
    withEchoToolDir((dir) => {
      const manifest = resolveModule(dir)
      expect(manifest.transport).toBe('wasm')
      expect(manifest.moduleType).toBe('tool')
      expect(manifest.artifactType).toBe('wasm')
    })
  })

  it('loadWasmFromPath loads echo-tool and returns loaded:Tool', () => {
    withEchoToolDir((dir) => {
      const result = loadWasmFromPath(dir)
      expect(result).toBe('loaded:Tool')
    })
  })

  it('example script exists at examples/node-wasm-session.ts', () => {
    const scriptPath = path.resolve(__dirname, '..', '..', '..', 'examples', 'node-wasm-session.ts')
    expect(fs.existsSync(scriptPath)).toBe(true)
  })
})
