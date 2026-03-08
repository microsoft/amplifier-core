import { hello } from '../index.js'
import { describe, it, expect } from 'vitest'

describe('amplifier-core native addon', () => {
  it('hello() returns expected greeting', () => {
    expect(hello()).toBe('Hello from amplifier-core native addon!')
  })
})
