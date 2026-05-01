import { describe, it, expect } from 'vitest'
import { AthenaBridge } from '../src/bridge.js'
import { notify } from '../src/tools/notify.js'
import { statusUpdate } from '../src/tools/status-update.js'
import { requestInput } from '../src/tools/request-input.js'
import { athenaNotify } from '../src/tools/athena-notify.js'
import { athenaUpdateStatus } from '../src/tools/athena-update-status.js'
import { athenaReportError } from '../src/tools/athena-report-error.js'
import { athenaReportCompletion } from '../src/tools/athena-report-completion.js'

function createBridge(): AthenaBridge {
  return new AthenaBridge({ athenaHost: '127.0.0.1', athenaPort: 4545 })
}

describe('notify tool', () => {
  it('returns confirmation text', async () => {
    const bridge = createBridge()
    const result = await notify(bridge, {
      level: 'info',
      message: 'Task done',
      title: 'Complete',
    })
    expect(result.content[0].text).toBe('Notification delivered.')
  })

  it('uses message prefix as title fallback', async () => {
    const bridge = createBridge()
    const handler = vi.fn()
    bridge.onEvent(handler)
    await notify(bridge, {
      level: 'warning',
      message: 'A very long message that exceeds sixty chars for testing',
    })
    expect(handler).toHaveBeenCalledWith(
      'notification',
      expect.objectContaining({
        title: 'A very long message that exceeds sixty chars for testing',
      }),
    )
  })
})

import { vi } from 'vitest'

describe('statusUpdate tool', () => {
  it('maps spec statuses to internal statuses', async () => {
    const bridge = createBridge()
    await statusUpdate(bridge, { status: 'working', agentId: 'a1' })
    const state = bridge.getAgentState('a1')
    expect(state!.status).toBe('running')
  })

  it('maps waiting_for_input to waiting', async () => {
    const bridge = createBridge()
    await statusUpdate(bridge, { status: 'waiting_for_input', agentId: 'a2' })
    const state = bridge.getAgentState('a2')
    expect(state!.status).toBe('waiting')
  })

  it('maps completed to done', async () => {
    const bridge = createBridge()
    await statusUpdate(bridge, { status: 'completed', agentId: 'a3' })
    const state = bridge.getAgentState('a3')
    expect(state!.status).toBe('done')
  })

  it('calculates progress percentage', async () => {
    const bridge = createBridge()
    await statusUpdate(bridge, {
      status: 'working',
      agentId: 'a4',
      progress: { current: 3, total: 10 },
    })
    const state = bridge.getAgentState('a4')
    expect(state!.progress).toBe(30)
  })

  it('returns status text', async () => {
    const bridge = createBridge()
    const result = await statusUpdate(bridge, { status: 'idle' })
    expect(result.content[0].text).toContain('idle')
  })
})

describe('requestInput tool', () => {
  it('returns error when not connected (timed out)', async () => {
    const bridge = createBridge()
    const result = await requestInput(bridge, {
      prompt: 'Continue?',
      timeoutMs: 100,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain('timed out')
  })

  it('returns timed out when not connected with 0 timeout', async () => {
    const bridge = createBridge()
    const result = await requestInput(bridge, {
      prompt: 'Yes or no?',
      options: ['yes', 'no'],
      timeoutMs: 0,
    })
    // 0 timeout gets converted to 600000ms default; since not connected it returns timed out
    expect(result.isError).toBe(true)
  })
})

describe('athena_notify tool', () => {
  it('returns confirmation with priority', async () => {
    const bridge = createBridge()
    const result = await athenaNotify(bridge, {
      type: 'error',
      title: 'Fail',
      message: 'Something broke',
      priority: 'critical',
    })
    expect(result.content[0].text).toContain('critical')
    expect(result.content[0].text).toContain('ERROR')
  })
})

describe('athena_update_status tool', () => {
  it('returns status confirmation', async () => {
    const bridge = createBridge()
    const result = await athenaUpdateStatus(bridge, {
      agentId: 'a1',
      status: 'running',
      progress: 50,
    })
    expect(result.content[0].text).toContain('running')
    expect(result.content[0].text).toContain('50%')
  })
})

describe('athena_report_error tool', () => {
  it('reports recoverable error', async () => {
    const bridge = createBridge()
    const result = await athenaReportError(bridge, {
      agentId: 'a1',
      error: 'Minor issue',
      recoverable: true,
    })
    expect(result.content[0].text).toContain('RECOVERABLE')
  })

  it('reports fatal error', async () => {
    const bridge = createBridge()
    const result = await athenaReportError(bridge, {
      agentId: 'a1',
      error: 'Total crash',
      recoverable: false,
      code: 137,
    })
    expect(result.content[0].text).toContain('FATAL')
    expect(result.content[0].text).toContain('137')
  })
})

describe('athena_report_completion tool', () => {
  it('reports completion with artifacts count', async () => {
    const bridge = createBridge()
    const result = await athenaReportCompletion(bridge, {
      agentId: 'a1',
      summary: 'Built the feature',
      artifacts: ['src/foo.ts', 'src/bar.ts', 'README.md'],
      duration: 60,
    })
    expect(result.content[0].text).toContain('3 artifacts')
    expect(result.content[0].text).toContain('60s')
  })

  it('reports completion without artifacts', async () => {
    const bridge = createBridge()
    const result = await athenaReportCompletion(bridge, {
      agentId: 'a1',
      summary: 'Done',
    })
    expect(result.content[0].text).toContain('completed')
  })
})
