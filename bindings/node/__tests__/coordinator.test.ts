import { describe, it, expect } from 'vitest'
import { JsCoordinator } from '../index.js'

describe('JsCoordinator', () => {
  it('creates with empty config (toolNames=[], providerNames=[], hasOrchestrator=false, hasContext=false)', () => {
    const coord = new JsCoordinator('{}')
    expect(coord.toolNames).toEqual([])
    expect(coord.providerNames).toEqual([])
    expect(coord.hasOrchestrator).toBe(false)
    expect(coord.hasContext).toBe(false)
  })

  it('registers and retrieves capabilities (registerCapability + getCapability roundtrip)', () => {
    const coord = new JsCoordinator('{}')
    coord.registerCapability('streaming', JSON.stringify({ enabled: true }))
    const result = coord.getCapability('streaming')
    expect(result).not.toBeNull()
    const parsed = JSON.parse(result!)
    expect(parsed).toEqual({ enabled: true })
  })

  it('getCapability returns null for missing', () => {
    const coord = new JsCoordinator('{}')
    const result = coord.getCapability('nonexistent')
    expect(result).toBeNull()
  })

  it('provides access to hooks subsystem (coord.hooks has listHandlers function)', () => {
    const coord = new JsCoordinator('{}')
    const hooks = coord.hooks
    expect(hooks).toBeDefined()
    expect(typeof hooks.listHandlers).toBe('function')
  })

  it('provides access to cancellation subsystem (coord.cancellation.isCancelled === false)', () => {
    const coord = new JsCoordinator('{}')
    const cancellation = coord.cancellation
    expect(cancellation).toBeDefined()
    expect(cancellation.isCancelled).toBe(false)
  })

  it('resetTurn resets turn tracking (should not throw)', () => {
    const coord = new JsCoordinator('{}')
    expect(() => coord.resetTurn()).not.toThrow()
  })

  it('toDict returns coordinator state (has tools, providers, has_orchestrator, has_context, capabilities)', () => {
    const coord = new JsCoordinator('{}')
    const dict = coord.toDict()
    expect(dict).toHaveProperty('tools')
    expect(dict).toHaveProperty('providers')
    expect(dict).toHaveProperty('has_orchestrator')
    expect(dict).toHaveProperty('has_context')
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
    const coord = new JsCoordinator('{}')
    await expect(coord.cleanup()).resolves.not.toThrow()
  })
})
