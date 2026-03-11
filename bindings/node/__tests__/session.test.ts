import { describe, it, expect } from 'vitest'
import { JsAmplifierSession } from '../index.js'
import { validConfig } from './fixtures'

describe('JsAmplifierSession', () => {
  it('creates with valid config and generates session ID', () => {
    const session = new JsAmplifierSession(validConfig)
    expect(session.sessionId).toBeTruthy()
    expect(session.sessionId.length).toBeGreaterThan(0)
  })

  it('creates with custom session ID', () => {
    const session = new JsAmplifierSession(validConfig, 'custom-id')
    expect(session.sessionId).toBe('custom-id')
  })

  it('creates with parent ID', () => {
    const session = new JsAmplifierSession(validConfig, undefined, 'parent-123')
    expect(session.parentId).toBe('parent-123')
  })

  it('parentId is null when no parent', () => {
    const session = new JsAmplifierSession(validConfig)
    expect(session.parentId).toBeNull()
  })

  it('starts as not initialized', () => {
    const session = new JsAmplifierSession(validConfig)
    expect(session.isInitialized).toBe(false)
  })

  it('status starts as running', () => {
    const session = new JsAmplifierSession(validConfig)
    expect(session.status).toBe('running')
  })

  // coordinator is now a getter that returns the session's real Arc<Coordinator>.
  it('coordinator getter returns the session coordinator', () => {
    const session = new JsAmplifierSession(validConfig)
    const coord = session.coordinator
    expect(coord).toBeDefined()
    // Verify coordinator was constructed from the session's config, not a default
    const coordConfig = JSON.parse(coord.config)
    expect(coordConfig).toHaveProperty('session')
  })

  it('coordinator getter returns the same instance on repeated calls', () => {
    const session = new JsAmplifierSession(validConfig)
    const c1 = session.coordinator
    const c2 = session.coordinator
    // Both should wrap the same underlying Arc<Coordinator>.
    // We verify by registering a capability on one and reading it from the other.
    c1.registerCapability('test-cap', JSON.stringify(true))
    const result = c2.getCapability('test-cap')
    expect(result).not.toBeNull()
    expect(JSON.parse(result as string)).toBe(true)
  })

  it('setInitialized marks session as initialized', () => {
    const session = new JsAmplifierSession(validConfig)
    expect(session.isInitialized).toBe(false)
    session.setInitialized()
    expect(session.isInitialized).toBe(true)
  })

  it('rejects empty config', () => {
    expect(() => new JsAmplifierSession('{}')).toThrow()
  })

  it('rejects config without orchestrator', () => {
    const config = JSON.stringify({ session: { context: 'context-simple' } })
    expect(() => new JsAmplifierSession(config)).toThrow(/orchestrator/)
  })

  it('rejects config without context', () => {
    const config = JSON.stringify({ session: { orchestrator: 'loop-basic' } })
    expect(() => new JsAmplifierSession(config)).toThrow(/context/)
  })

  it('cleanup clears initialized flag', async () => {
    const session = new JsAmplifierSession(validConfig)
    session.setInitialized()
    expect(session.isInitialized).toBe(true)
    await session.cleanup()
    expect(session.isInitialized).toBe(false)
  })
})
