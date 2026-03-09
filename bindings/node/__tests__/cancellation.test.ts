import { describe, it, expect } from 'vitest'
import { JsCancellationToken } from '../index.js'

describe('JsCancellationToken', () => {
  it('creates with default state (not cancelled, not graceful, not immediate)', () => {
    const token = new JsCancellationToken()
    expect(token.isCancelled).toBe(false)
    expect(token.isGraceful).toBe(false)
    expect(token.isImmediate).toBe(false)
  })

  it('requestGraceful transitions to graceful', () => {
    const token = new JsCancellationToken()
    token.requestGraceful()
    expect(token.isCancelled).toBe(true)
    expect(token.isGraceful).toBe(true)
    expect(token.isImmediate).toBe(false)
  })

  it('requestImmediate transitions to immediate', () => {
    const token = new JsCancellationToken()
    token.requestImmediate()
    expect(token.isCancelled).toBe(true)
    expect(token.isImmediate).toBe(true)
  })

  it('graceful then immediate escalates', () => {
    const token = new JsCancellationToken()
    token.requestGraceful()
    expect(token.isCancelled).toBe(true)
    expect(token.isGraceful).toBe(true)
    token.requestImmediate()
    expect(token.isCancelled).toBe(true)
    expect(token.isImmediate).toBe(true)
  })

  it('reset returns to uncancelled state', () => {
    const token = new JsCancellationToken()
    token.requestGraceful()
    expect(token.isCancelled).toBe(true)
    token.reset()
    expect(token.isCancelled).toBe(false)
    expect(token.isGraceful).toBe(false)
    expect(token.isImmediate).toBe(false)
  })

})
