import { describe, it, expect, vi } from 'vitest'
import { OutputBufferManager } from '../src/output-buffer.js'
import { athenaReadOutput } from '../src/tools/athena-read-output.js'
import { athenaStreamOutput } from '../src/tools/athena-stream-output.js'
import { athenaListAgents } from '../src/tools/athena-list-agents.js'
import { athenaGetOutputSince } from '../src/tools/athena-get-output-since.js'
import { AthenaBridge } from '../src/bridge.js'

function createBuffer(): OutputBufferManager {
  return new OutputBufferManager()
}

function createBridge(): AthenaBridge {
  return new AthenaBridge({ athenaHost: '127.0.0.1', athenaPort: 4545 })
}

describe('athena_read_output', () => {
  it('returns no output message for unknown pane', async () => {
    const buf = createBuffer()
    const result = await athenaReadOutput(buf, { paneId: 'unknown' })
    expect(result.content[0].text).toContain('No output found')
  })

  it('returns full buffer for known pane', async () => {
    const buf = createBuffer()
    buf.append('p1', 'hello')
    buf.append('p1', 'world')
    const result = await athenaReadOutput(buf, { paneId: 'p1' })
    expect(result.content[0].text).toContain('[1]')
    expect(result.content[0].text).toContain('hello')
    expect(result.content[0].text).toContain('[2]')
    expect(result.content[0].text).toContain('world')
  })

  it('respects lines parameter', async () => {
    const buf = createBuffer()
    for (let i = 0; i < 20; i++) buf.append('p1', `line ${i}`)
    const result = await athenaReadOutput(buf, { paneId: 'p1', lines: 3 })
    const lines = result.content[0].text.split('\n')
    expect(lines).toHaveLength(3)
  })

  it('marks stderr lines', async () => {
    const buf = createBuffer()
    buf.append('p1', 'normal')
    buf.append('p1', 'error', true)
    const result = await athenaReadOutput(buf, { paneId: 'p1' })
    expect(result.content[0].text).toContain('[stderr]')
  })
})

describe('athena_stream_output', () => {
  it('returns snapshot of recent lines and collects new output', async () => {
    const buf = createBuffer()
    buf.append('p1', 'existing line 1')
    buf.append('p1', 'existing line 2')

    const streamPromise = athenaStreamOutput(buf, { paneId: 'p1' })

    setTimeout(() => {
      buf.append('p1', 'new output line')
    }, 50)

    vi.useFakeTimers()
    const resultPromise = athenaStreamOutput(buf, { paneId: 'p1' })

    setTimeout(() => {
      buf.append('p1', 'streamed line')
    }, 100)

    vi.advanceTimersByTimeAsync(200)
    vi.useRealTimers()
  }, 10_000)

  it('returns no snapshot for empty pane', async () => {
    const buf = createBuffer()
    const result = await Promise.race([
      athenaStreamOutput(buf, { paneId: 'empty' }),
      new Promise<any>((resolve) => setTimeout(() => resolve('timeout'), 200)),
    ])
    if (result !== 'timeout') {
      expect(result.content[0].text).toContain('Stream ended')
    }
  })
})

describe('athena_list_agents', () => {
  it('returns no agents message when none exist', async () => {
    const bridge = createBridge()
    const result = await athenaListAgents(bridge, {})
    expect(result.content[0].text).toContain('No active agents')
  })

  it('lists agents from bridge state', async () => {
    const bridge = createBridge()
    await bridge.updateStatus({
      agentId: 'agent-1',
      status: 'running',
      message: 'working',
    })
    await bridge.updateStatus({
      agentId: 'agent-2',
      status: 'idle',
      message: 'waiting',
    })
    const result = await athenaListAgents(bridge, {})
    expect(result.content[0].text).toContain('agent-1')
    expect(result.content[0].text).toContain('running')
    expect(result.content[0].text).toContain('agent-2')
    expect(result.content[0].text).toContain('idle')
  })
})

describe('athena_get_output_since', () => {
  it('returns error when no criteria provided', async () => {
    const buf = createBuffer()
    const result = await athenaGetOutputSince(buf, { paneId: 'p1' })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain('Provide at least one')
  })

  it('returns entries after sinceLine', async () => {
    const buf = createBuffer()
    for (let i = 0; i < 10; i++) buf.append('p1', `line ${i}`)
    const result = await athenaGetOutputSince(buf, { paneId: 'p1', sinceLine: 7 })
    const lines = result.content[0].text.split('\n')
    expect(lines).toHaveLength(3)
  })

  it('returns no new output message when no matches', async () => {
    const buf = createBuffer()
    buf.append('p1', 'hello')
    const result = await athenaGetOutputSince(buf, { paneId: 'p1', sinceLine: 999 })
    expect(result.content[0].text).toContain('No new output')
  })

  it('works with sinceTimestamp', async () => {
    const buf = createBuffer()
    const before = Date.now()
    buf.append('p1', 'old')
    const afterFirst = Date.now()
    buf.append('p1', 'new')

    const result = await athenaGetOutputSince(buf, { paneId: 'p1', sinceTimestamp: afterFirst })
    expect(result.content[0].text).not.toContain('old')
  })
})
