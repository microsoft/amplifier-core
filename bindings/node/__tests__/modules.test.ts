import { describe, it, expect } from 'vitest'
import { JsToolBridge } from '../index.js'

describe('JsToolBridge', () => {
  it('creates a JsToolBridge wrapping a TS tool object', () => {
    const tool = new JsToolBridge(
      'echo',
      'Echoes back the input',
      '{"type": "object", "properties": {"message": {"type": "string"}}}',
      async (inputJson: string) => {
        const input = JSON.parse(inputJson)
        return JSON.stringify({ success: true, output: input.message })
      }
    )

    expect(tool.name).toBe('echo')
    expect(tool.description).toBe('Echoes back the input')
  })

  it('executes a tool through the bridge', async () => {
    const tool = new JsToolBridge(
      'greet',
      'Greets someone by name',
      '{"type": "object", "properties": {"name": {"type": "string"}}}',
      async (inputJson: string) => {
        const input = JSON.parse(inputJson)
        return JSON.stringify({ success: true, output: `Hello, ${input.name}!` })
      }
    )

    const resultJson = await tool.execute(JSON.stringify({ name: 'World' }))
    const result = JSON.parse(resultJson)

    expect(result.output).toBe('Hello, World!')
    expect(result.success).toBe(true)
  })

  it('handles tool execution errors', async () => {
    const tool = new JsToolBridge(
      'failing',
      'A tool that always fails',
      '{}',
      async (_inputJson: string) => {
        return JSON.stringify({ success: false, error: 'Something went wrong' })
      }
    )

    const resultJson = await tool.execute('{}')
    const result = JSON.parse(resultJson)

    expect(result.success).toBe(false)
    expect(result.error).toBe('Something went wrong')
  })

  it('getSpec returns valid JSON with name, description, and parameters', () => {
    const params = '{"type": "object", "properties": {"x": {"type": "number"}}}'
    const tool = new JsToolBridge(
      'calc',
      'A calculator tool',
      params,
      async (_inputJson: string) => '{}'
    )

    const spec = JSON.parse(tool.getSpec())

    expect(spec.name).toBe('calc')
    expect(spec.description).toBe('A calculator tool')
    expect(spec.parameters).toEqual(JSON.parse(params))
  })

  it('rejects when the JS callback throws an exception', async () => {
    const tool = new JsToolBridge(
      'thrower',
      'A tool whose callback throws',
      '{}',
      async (_inputJson: string) => {
        throw new Error('callback exploded')
      }
    )

    await expect(tool.execute('{}')).rejects.toThrow('callback exploded')
  })

  it('getSpec falls back to empty object for malformed parametersJson', () => {
    const tool = new JsToolBridge(
      'broken',
      'Tool with bad params',
      'not valid json{{{',
      async (_inputJson: string) => '{}'
    )

    const spec = JSON.parse(tool.getSpec())

    expect(spec.name).toBe('broken')
    expect(spec.description).toBe('Tool with bad params')
    expect(spec.parameters).toEqual({})
  })
})
