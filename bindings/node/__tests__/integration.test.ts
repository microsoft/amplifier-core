import { describe, it, expect } from 'vitest'
import {
  JsAmplifierSession,
  JsCoordinator,
  JsHookRegistry,
  JsCancellationToken,
  JsToolBridge,
  HookAction,
  ContextInjectionRole,
  UserMessageLevel,
} from '../index.js'
import { validConfig, emptyConfig } from './fixtures'

describe('Full session lifecycle', () => {
  it('session -> coordinator -> hooks -> cancel lifecycle', async () => {
    // Create session
    const session = new JsAmplifierSession(validConfig)
    expect(session.sessionId).toBeTruthy()
    expect(session.isInitialized).toBe(false)

    // Access coordinator
    const coord = session.coordinator
    expect(coord).toBeDefined()

    // Register capability and verify roundtrip
    coord.registerCapability('streaming', JSON.stringify({ enabled: true, format: 'sse' }))
    const cap = coord.getCapability('streaming')
    expect(cap).not.toBeNull()
    const parsed = JSON.parse(cap as string)
    expect(parsed).toEqual({ enabled: true, format: 'sse' })

    // Use cancellation: graceful
    const cancellation = coord.cancellation
    cancellation.requestGraceful('user stop')
    expect(cancellation.isCancelled).toBe(true)
    expect(cancellation.isGraceful).toBe(true)

    // Reset cancellation
    cancellation.reset()
    expect(cancellation.isCancelled).toBe(false)

    // Cleanup session
    session.setInitialized()
    expect(session.isInitialized).toBe(true)
    await session.cleanup()
    expect(session.isInitialized).toBe(false)
  })
})

describe('Hook handler roundtrip', () => {
  it('JS handler receives event data and returns HookResult', async () => {
    const registry = new JsHookRegistry()
    let receivedEvent = ''
    let receivedData: Record<string, unknown> | null = null

    registry.register('tool:pre', (event: string, data: string) => {
      receivedEvent = event
      receivedData = JSON.parse(data)
      return JSON.stringify({ action: 'continue' })
    }, 5, 'capture-handler')

    const result = await registry.emit('tool:pre', JSON.stringify({ tool_name: 'bash', command: 'ls' }))

    expect(receivedEvent).toBe('tool:pre')
    expect(receivedData).toHaveProperty('tool_name', 'bash')
    expect(receivedData).toHaveProperty('command', 'ls')
    expect(result.action).toBe(HookAction.Continue)
  })

  it('deny handler short-circuits pipeline', async () => {
    const registry = new JsHookRegistry()
    let secondHandlerCalled = false

    // Denier at priority 0 (runs first — lower priority = first)
    registry.register('tool:pre', (_event: string, _data: string) => {
      return JSON.stringify({ action: 'deny', reason: 'not allowed' })
    }, 0, 'denier')

    // After-deny at priority 10 (should NOT run)
    registry.register('tool:pre', (_event: string, _data: string) => {
      secondHandlerCalled = true
      return JSON.stringify({ action: 'continue' })
    }, 10, 'after-deny')

    const result = await registry.emit('tool:pre', JSON.stringify({ tool_name: 'rm' }))

    expect(result.action).toBe(HookAction.Deny)
    expect(result.reason).toBe('not allowed')
    expect(secondHandlerCalled).toBe(false)
  })
})

describe('Tool bridge execution', () => {
  it('creates calculator tool and verifies name, spec, and execution', async () => {
    const calculator = new JsToolBridge(
      'calculator',
      'Adds two numbers',
      JSON.stringify({
        type: 'object',
        properties: {
          a: { type: 'number' },
          b: { type: 'number' },
        },
      }),
      async (inputJson: string) => {
        const input = JSON.parse(inputJson)
        const sum = input.a + input.b
        return JSON.stringify({ success: true, output: String(sum) })
      }
    )

    // Verify name
    expect(calculator.name).toBe('calculator')

    // Verify getSpec() roundtrip
    const spec = JSON.parse(calculator.getSpec())
    expect(spec.name).toBe('calculator')
    expect(spec.parameters.type).toBe('object')

    // Execute and verify result
    const resultJson = await calculator.execute(JSON.stringify({ a: 3, b: 4 }))
    const result = JSON.parse(resultJson)
    expect(result.success).toBe(true)
    expect(result.output).toBe('7')
  })
})

describe('CancellationToken state machine', () => {
  it('full cycle: None -> Graceful -> Immediate -> reset -> None', () => {
    const token = new JsCancellationToken()

    // Initial state: None
    expect(token.isCancelled).toBe(false)
    expect(token.isGraceful).toBe(false)
    expect(token.isImmediate).toBe(false)

    // None -> Graceful
    token.requestGraceful()
    expect(token.isCancelled).toBe(true)
    expect(token.isGraceful).toBe(true)
    expect(token.isImmediate).toBe(false)

    // Graceful -> Immediate
    token.requestImmediate()
    expect(token.isCancelled).toBe(true)
    expect(token.isGraceful).toBe(false)
    expect(token.isImmediate).toBe(true)

    // Immediate -> reset -> None
    token.reset()
    expect(token.isCancelled).toBe(false)
    expect(token.isGraceful).toBe(false)
    expect(token.isImmediate).toBe(false)
  })
})

describe('Type fidelity', () => {
  it('SessionConfig validates required fields with extra providers/metadata', () => {
    const config = JSON.stringify({
      session: { orchestrator: 'loop-basic', context: 'context-simple' },
      providers: [{ name: 'openai', model: 'gpt-4' }],
      metadata: { user: 'test-user', env: 'ci' },
    })
    const session = new JsAmplifierSession(config)
    expect(session.sessionId).toBeTruthy()
    expect(session.status).toBe('running')
  })

  it('HookResult fields roundtrip with inject_context action', async () => {
    const registry = new JsHookRegistry()

    registry.register('tool:pre', (_event: string, _data: string) => {
      return JSON.stringify({
        action: 'inject_context',
        context_injection: 'You are a helpful assistant',
        context_injection_role: 'system',
        ephemeral: true,
        suppress_output: false,
        user_message: 'Context injected',
        user_message_level: 'info',
        user_message_source: 'integration-test',
      })
    }, 5, 'inject-handler')

    const result = await registry.emit('tool:pre', '{}')

    expect(result.action).toBe(HookAction.InjectContext)
    expect(result.contextInjection).toBe('You are a helpful assistant')
    expect(result.contextInjectionRole).toBe(ContextInjectionRole.System)
    expect(result.ephemeral).toBe(true)
    expect(result.suppressOutput).toBe(false)
    expect(result.userMessage).toBe('Context injected')
    expect(result.userMessageLevel).toBe(UserMessageLevel.Info)
    expect(result.userMessageSource).toBe('integration-test')
  })

  it('Coordinator toDict returns all expected fields', () => {
    const coord = new JsCoordinator(emptyConfig)
    const dict = coord.toDict()

    // Arrays
    expect(Array.isArray(dict.tools)).toBe(true)
    expect(Array.isArray(dict.providers)).toBe(true)
    expect(dict.capabilities).toBeDefined()
    expect(typeof dict.capabilities).toBe('object')

    // Booleans
    expect(typeof dict.has_orchestrator).toBe('boolean')
    expect(typeof dict.has_context).toBe('boolean')
  })
})
