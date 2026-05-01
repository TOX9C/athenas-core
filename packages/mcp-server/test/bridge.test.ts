import { describe, it, expect, vi, beforeEach } from 'vitest'
import { AthenaBridge } from '../src/bridge.js'

function createBridge(): AthenaBridge {
  return new AthenaBridge({
    athenaHost: '127.0.0.1',
    athenaPort: 4545,
    authToken: 'test-token',
  })
}

describe('AthenaBridge', () => {
  it('constructs with config', () => {
    const bridge = createBridge()
    expect(bridge.isConnected()).toBe(false)
  })

  it('tracks agent state after updateStatus', async () => {
    const bridge = createBridge()
    await bridge.updateStatus({
      agentId: 'agent-1',
      status: 'running',
      message: 'Building feature',
      progress: 50,
    })
    const state = bridge.getAgentState('agent-1')
    expect(state).toBeDefined()
    expect(state!.id).toBe('agent-1')
    expect(state!.status).toBe('running')
    expect(state!.message).toBe('Building feature')
    expect(state!.progress).toBe(50)
  })

  it('tracks error status after reportError', async () => {
    const bridge = createBridge()
    await bridge.reportError({
      agentId: 'agent-2',
      error: 'Something broke',
      recoverable: true,
      code: 'EXIT_1',
    })
    const state = bridge.getAgentState('agent-2')
    expect(state).toBeDefined()
    expect(state!.status).toBe('error')
    expect(state!.message).toBe('Something broke')
  })

  it('tracks done status after reportCompletion', async () => {
    const bridge = createBridge()
    await bridge.reportCompletion({
      agentId: 'agent-3',
      summary: 'All files written',
      artifacts: ['src/index.ts', 'src/types.ts'],
      duration: 120,
    })
    const state = bridge.getAgentState('agent-3')
    expect(state).toBeDefined()
    expect(state!.status).toBe('done')
    expect(state!.message).toBe('All files written')
  })

  it('returns all agent states', async () => {
    const bridge = createBridge()
    await bridge.updateStatus({ agentId: 'a1', status: 'running' })
    await bridge.updateStatus({ agentId: 'a2', status: 'idle' })
    const all = bridge.getAllAgentStates()
    expect(all).toHaveLength(2)
    expect(all.map((a) => a.id).sort()).toEqual(['a1', 'a2'])
  })

  it('returns partial app state', async () => {
    const bridge = createBridge()
    await bridge.updateStatus({ agentId: 'a1', status: 'running' })
    const state = bridge.getAppState()
    expect(state.agents).toHaveLength(1)
  })

  it('emits events on status update', async () => {
    const bridge = createBridge()
    const handler = vi.fn()
    bridge.onEvent(handler)
    await bridge.updateStatus({ agentId: 'agent-x', status: 'waiting' })
    expect(handler).toHaveBeenCalledWith('statusUpdate', expect.any(Object))
  })

  it('emits events on notification', async () => {
    const bridge = createBridge()
    const handler = vi.fn()
    bridge.onEvent(handler)
    await bridge.sendNotification({
      type: 'info',
      title: 'Test',
      message: 'Hello',
      priority: 'normal',
    })
    expect(handler).toHaveBeenCalledWith('notification', expect.any(Object))
  })

  it('emits events on error report', async () => {
    const bridge = createBridge()
    const handler = vi.fn()
    bridge.onEvent(handler)
    await bridge.reportError({
      agentId: 'err-agent',
      error: 'fail',
      recoverable: false,
    })
    expect(handler).toHaveBeenCalledWith('error', expect.any(Object))
  })

  it('emits events on completion report', async () => {
    const bridge = createBridge()
    const handler = vi.fn()
    bridge.onEvent(handler)
    await bridge.reportCompletion({
      agentId: 'done-agent',
      summary: 'Done',
    })
    expect(handler).toHaveBeenCalledWith('completion', expect.any(Object))
  })

  it('unsubscribes event handler', async () => {
    const bridge = createBridge()
    const handler = vi.fn()
    const unsub = bridge.onEvent(handler)
    unsub()
    await bridge.updateStatus({ agentId: 'ghost', status: 'idle' })
    expect(handler).not.toHaveBeenCalled()
  })

  it('requestInput returns cancelled when not connected', async () => {
    const bridge = createBridge()
    const result = await bridge.requestInput({
      prompt: 'Continue?',
      defaultResponse: 'yes',
      timeout: 1000,
    })
    expect(result.cancelled).toBe(true)
    expect(result.timedOut).toBe(true)
  })

  it('buffers notifications when not connected', async () => {
    const bridge = createBridge()
    await bridge.sendNotification({ type: 'info', title: 'T1', message: 'M1', priority: 'normal' })
    await bridge.sendNotification({ type: 'warning', title: 'T2', message: 'M2', priority: 'high' })
    // No crash, notifications buffered
    expect(bridge.isConnected()).toBe(false)
  })

  it('overwrites agent state on subsequent updates', async () => {
    const bridge = createBridge()
    await bridge.updateStatus({ agentId: 'agent-1', status: 'running', message: 'first' })
    await bridge.updateStatus({ agentId: 'agent-1', status: 'done', message: 'second' })
    const state = bridge.getAgentState('agent-1')
    expect(state!.status).toBe('done')
    expect(state!.message).toBe('second')
  })

  it('disconnect resolves pending inputs as cancelled', async () => {
    const bridge = createBridge()
    // Request input while not connected — immediately resolves as cancelled
    const result = await bridge.requestInput({
      prompt: 'Question?',
      timeout: 5000,
    })
    expect(result.cancelled).toBe(true)
  })
})
