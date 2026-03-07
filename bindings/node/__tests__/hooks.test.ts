import { describe, it, expect } from 'vitest'
import { JsHookRegistry, HookAction } from '../index.js'

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
})
