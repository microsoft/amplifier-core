import { describe, it, expect } from 'vitest'
import { JsCoordinator } from '../index.js'
import { emptyConfig } from './fixtures'

describe('JsCoordinator', () => {
  it('creates with empty config (toolNames=[], providerNames=[], hasOrchestrator=false, hasContext=false)', () => {
    const coord = new JsCoordinator(emptyConfig)
    expect(coord.toolNames).toEqual([])
    expect(coord.providerNames).toEqual([])
    expect(coord.hasOrchestrator).toBe(false)
    expect(coord.hasContext).toBe(false)
  })

  it('throws on invalid JSON config', () => {
    expect(() => new JsCoordinator('invalid json')).toThrow()
  })

  it('registers and retrieves capabilities (registerCapability + getCapability roundtrip)', () => {
    const coord = new JsCoordinator(emptyConfig)
    coord.registerCapability('streaming', JSON.stringify({ enabled: true }))
    const result = coord.getCapability('streaming')
    expect(result).not.toBeNull()
    const parsed = JSON.parse(result as string)
    expect(parsed).toEqual({ enabled: true })
  })

  it('getCapability returns null for missing', () => {
    const coord = new JsCoordinator(emptyConfig)
    const result = coord.getCapability('nonexistent')
    expect(result).toBeNull()
  })

  // hooks is now a getter that returns the coordinator's real Arc<HookRegistry>.
  it('hooks getter returns a JsHookRegistry with listHandlers', () => {
    const coord = new JsCoordinator(emptyConfig)
    const hooks = coord.hooks
    expect(hooks).toBeDefined()
    expect(typeof hooks.listHandlers).toBe('function')
  })

  it('hooks getter shares state — handlers registered on one call are visible on next', () => {
    const coord = new JsCoordinator(emptyConfig)
    const h1 = coord.hooks
    // Register a handler on h1
    h1.register('test:event', (_event: string, _data: string) => {
      return JSON.stringify({ action: 'Continue' })
    }, 0, 'test-handler')
    // h2 should see the same handler
    const h2 = coord.hooks
    const handlers = h2.listHandlers()
    expect(handlers['test:event']).toContain('test-handler')
  })

  it('provides access to cancellation subsystem (coord.cancellation.isCancelled === false)', () => {
    const coord = new JsCoordinator(emptyConfig)
    const cancellation = coord.cancellation
    expect(cancellation).toBeDefined()
    expect(cancellation.isCancelled).toBe(false)
  })

  it('resetTurn resets turn tracking (should not throw)', () => {
    const coord = new JsCoordinator(emptyConfig)
    expect(() => coord.resetTurn()).not.toThrow()
  })

  it('toDict returns coordinator state (has tools, providers, has_orchestrator, has_context, capabilities)', () => {
    const coord = new JsCoordinator(emptyConfig)
    const dict = coord.toDict()
    expect(dict.tools).toEqual([])
    expect(dict.providers).toEqual([])
    expect(dict.has_orchestrator).toBe(false)
    expect(dict.has_context).toBe(false)
    expect(dict).toHaveProperty('capabilities')
  })

  it('config returns original config (coord.config is defined)', () => {
    const coord = new JsCoordinator('{"key":"value"}')
    const config = coord.config
    expect(config).toBeDefined()
    const parsed = JSON.parse(config)
    expect(parsed).toEqual({ key: 'value' })
  })

  it('cleanup completes without error', async () => {
    const coord = new JsCoordinator(emptyConfig)
    await coord.cleanup()
  })
})
