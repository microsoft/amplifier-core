import { describe, it, expect } from 'vitest'
import { JsAmplifierSession, amplifierErrorToJs } from '../index.js'

describe('Error bridging — session constructor', () => {
  it('invalid JSON config throws with /Invalid config JSON/ message', () => {
    expect(() => new JsAmplifierSession('not json')).toThrow(/Invalid config JSON/)
  })

  it('missing orchestrator throws with /orchestrator/ in message', () => {
    const config = JSON.stringify({ session: { context: 'context-simple' } })
    expect(() => new JsAmplifierSession(config)).toThrow(/orchestrator/)
  })

  it('missing context throws with /context/ in message', () => {
    const config = JSON.stringify({ session: { orchestrator: 'loop-basic' } })
    expect(() => new JsAmplifierSession(config)).toThrow(/context/)
  })
})

describe('amplifierErrorToJs — variant to typed error object', () => {
  it('converts session variant to SessionError code', () => {
    const err = amplifierErrorToJs('session', 'not initialized')
    expect(err.code).toBe('SessionError')
    expect(err.message).toBe('not initialized')
  })

  it('converts tool variant to ToolError code', () => {
    const err = amplifierErrorToJs('tool', 'tool not found: bash')
    expect(err.code).toBe('ToolError')
  })

  it('converts provider variant to ProviderError code', () => {
    const err = amplifierErrorToJs('provider', 'rate limited')
    expect(err.code).toBe('ProviderError')
  })

  it('converts hook variant to HookError code', () => {
    const err = amplifierErrorToJs('hook', 'handler failed')
    expect(err.code).toBe('HookError')
  })

  it('converts context variant to ContextError code', () => {
    const err = amplifierErrorToJs('context', 'compaction failed')
    expect(err.code).toBe('ContextError')
  })
})
