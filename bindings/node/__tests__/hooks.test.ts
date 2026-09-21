import { describe, it, expect } from 'vitest'
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { JsHookRegistry, HookAction } from '../index.js'

const compiledIndexPath = fileURLToPath(new URL('../index.js', import.meta.url))

function runAsyncContinueChild(): Promise<{
  code: number | null
  signal: string | null
  stdout: string
  stderr: string
}> {
  const childSource = `
    const { pathToFileURL } = await import('node:url')
    const { default: binding } = await import(pathToFileURL(process.argv[1]).href)
    const { JsHookRegistry, HookAction } = binding
    const registry = new JsHookRegistry()
    let handlerCalled = false
    let receivedEvent = ''

    registry.register('tool:pre', async (event, _data) => {
      handlerCalled = true
      receivedEvent = event
      await new Promise(resolve => setImmediate(resolve))
      return JSON.stringify({ action: 'continue' })
    }, 10, 'async-hook')

    const result = await registry.emit('tool:pre', '{"tool":"grep"}')
    if (!handlerCalled || receivedEvent !== 'tool:pre') {
      throw new Error('async handler did not receive the expected event')
    }
    console.log('afterEmit')
    if (result.action !== HookAction.Continue) {
      throw new Error('async handler did not continue')
    }
    console.log('resultContinue')
    process.exit(0)
  `

  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      ['--input-type=module', '--eval', childSource, compiledIndexPath],
      { stdio: ['ignore', 'pipe', 'pipe'] }
    )
    let stdout = ''
    let stderr = ''
    let settled = false
    const timeout = setTimeout(() => {
      settled = true
      child.kill('SIGKILL')
      reject(new Error(`async hook child timed out: ${stderr}`))
    }, 10_000)

    child.stdout.on('data', chunk => {
      stdout += chunk
    })
    child.stderr.on('data', chunk => {
      stderr += chunk
    })
    child.once('error', error => {
      if (!settled) {
        settled = true
        clearTimeout(timeout)
        reject(error)
      }
    })
    child.once('close', (code, signal) => {
      if (!settled) {
        settled = true
        clearTimeout(timeout)
        resolve({ code, signal, stdout, stderr })
      }
    })
  })
}

describe('JsHookRegistry', () => {
  it('creates empty registry (listHandlers returns empty object)', () => {
    const registry = new JsHookRegistry()
    const handlers = registry.listHandlers()
    expect(handlers).toEqual({})
  })

  it('emits with no handlers returns Continue', async () => {
    const registry = new JsHookRegistry()
    const result = await registry.emit('tool:pre', '{"tool":"grep"}')
    expect(result.action).toBe(HookAction.Continue)
  })

  it('registers and emits to a JS handler', async () => {
    const registry = new JsHookRegistry()
    let handlerCalled = false
    let receivedEvent = ''
    let receivedData = ''

    registry.register('tool:pre', (event: string, data: string) => {
      handlerCalled = true
      receivedEvent = event
      receivedData = data
      return JSON.stringify({ action: 'continue' })
    }, 10, 'my-hook')

    await registry.emit('tool:pre', '{"tool":"grep"}')

    expect(handlerCalled).toBe(true)
    expect(receivedEvent).toBe('tool:pre')
    expect(JSON.parse(receivedData)).toHaveProperty('tool', 'grep')
  })

  it('listHandlers returns registered handler names', () => {
    const registry = new JsHookRegistry()
    registry.register('tool:pre', (_event: string, _data: string) => {
      return JSON.stringify({ action: 'continue' })
    }, 10, 'my-hook')

    const handlers = registry.listHandlers()
    expect(handlers['tool:pre']).toContain('my-hook')
  })

  it('handler returning deny stops pipeline', async () => {
    const registry = new JsHookRegistry()
    registry.register('tool:pre', (_event: string, _data: string) => {
      return JSON.stringify({ action: 'deny', reason: 'blocked' })
    }, 10, 'deny-hook')

    const result = await registry.emit('tool:pre', '{"tool":"rm"}')
    expect(result.action).toBe(HookAction.Deny)
    expect(result.reason).toBe('blocked')
  })

  it('returns Deny when hook handler returns invalid JSON (fail-closed)', async () => {
    const registry = new JsHookRegistry()
    registry.register(
      'tool:pre',
      (_event: string, _data: string) => 'NOT VALID JSON {{{',
      10,
      'bad-json-hook'
    )
    const result = await registry.emit('tool:pre', '{}')
    expect(result.action).toBe(HookAction.Deny)
    expect(result.reason).toContain('invalid')
  })

  it('setDefaultFields merges into emit data', async () => {
    const registry = new JsHookRegistry()
    let receivedData = ''

    registry.register('tool:pre', (_event: string, data: string) => {
      receivedData = data
      return JSON.stringify({ action: 'continue' })
    }, 10, 'capture-hook')

    registry.setDefaultFields('{"session_id":"s-123","custom":"value"}')
    await registry.emit('tool:pre', '{"tool":"grep"}')

    const parsed = JSON.parse(receivedData)
    expect(parsed).toHaveProperty('session_id', 's-123')
    expect(parsed).toHaveProperty('custom', 'value')
    expect(parsed).toHaveProperty('tool', 'grep')
  })

  it('supports async handlers returning Promise<string>', async () => {
    const result = await runAsyncContinueChild()

    expect(result.signal).toBeNull()
    expect(result.code).toBe(0)
    expect(result.stdout).toContain('afterEmit')
    expect(result.stdout).toContain('resultContinue')
  })

  it('async handler returning deny short-circuits pipeline', async () => {
    const registry = new JsHookRegistry()
    let secondRan = false

    registry.register('tool:pre', async (_event: string, _data: string) => {
      await new Promise<void>(resolve => setImmediate(() => resolve()))
      return JSON.stringify({ action: 'deny', reason: 'async blocked' })
    }, 10, 'async-deny')
    registry.register('tool:pre', (_event: string, _data: string) => {
      secondRan = true
      return JSON.stringify({ action: 'continue' })
    }, 20, 'after')

    const result = await registry.emit('tool:pre', '{"tool":"rm"}')

    expect(result.action).toBe(HookAction.Deny)
    expect(result.reason).toBe('async blocked')
    expect(secondRan).toBe(false)
  })
})
