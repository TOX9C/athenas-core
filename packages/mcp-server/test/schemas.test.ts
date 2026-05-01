import { describe, it, expect } from 'vitest'
import { notifySchema, statusUpdateSchema, requestInputSchema } from '../src/tools/notify.js'
import { statusUpdateSchema as statusSchema } from '../src/tools/status-update.js'
import { requestInputSchema as inputSchema } from '../src/tools/request-input.js'
import {
  athenaNotifySchema,
  athenaRequestInputSchema,
  athenaUpdateStatusSchema,
  athenaReportErrorSchema,
  athenaReportCompletionSchema,
} from '../src/tools/index.js'
import { controlPauseSchema, controlResumeSchema, controlCancelSchema } from '../src/tools/index.js'

describe('notify schema', () => {
  it('validates required fields', () => {
    const result = notifySchema.safeParse({ level: 'info', message: 'Hello' })
    expect(result.success).toBe(true)
  })

  it('rejects missing required fields', () => {
    const result = notifySchema.safeParse({ level: 'info' })
    expect(result.success).toBe(false)
  })

  it('validates all levels', () => {
    for (const level of ['info', 'warning', 'error', 'success']) {
      const result = notifySchema.safeParse({ level, message: 'Test' })
      expect(result.success).toBe(true)
    }
  })

  it('validates optional fields', () => {
    const result = notifySchema.safeParse({
      level: 'warning',
      message: 'Careful',
      title: 'Heads up',
      metadata: { taskId: 'abc' },
      actions: [{ id: 'dismiss', label: 'Dismiss' }],
      priority: 'high',
      agentId: 'agent-1',
    })
    expect(result.success).toBe(true)
  })

  it('rejects invalid level', () => {
    const result = notifySchema.safeParse({ level: 'critical', message: 'Bad' })
    expect(result.success).toBe(false)
  })
})

describe('status_update schema', () => {
  it('validates required status field', () => {
    const result = statusSchema.safeParse({ status: 'working' })
    expect(result.success).toBe(true)
  })

  it('validates all statuses', () => {
    for (const status of [
      'idle',
      'thinking',
      'working',
      'waiting_for_input',
      'completed',
      'error',
      'cancelled',
    ]) {
      const result = statusSchema.safeParse({ status })
      expect(result.success).toBe(true)
    }
  })

  it('validates progress object', () => {
    const result = statusSchema.safeParse({
      status: 'working',
      progress: { current: 3, total: 10, label: 'Step 3' },
    })
    expect(result.success).toBe(true)
  })

  it('validates artifacts array', () => {
    const result = statusSchema.safeParse({
      status: 'completed',
      artifacts: [
        { path: '/src/foo.ts', type: 'file' },
        { path: 'https://example.com', type: 'url' },
      ],
    })
    expect(result.success).toBe(true)
  })

  it('rejects missing status', () => {
    const result = statusSchema.safeParse({ message: 'no status' })
    expect(result.success).toBe(false)
  })
})

describe('request_input schema', () => {
  it('validates required prompt', () => {
    const result = inputSchema.safeParse({ prompt: 'Continue?' })
    expect(result.success).toBe(true)
  })

  it('validates with all options', () => {
    const result = inputSchema.safeParse({
      prompt: 'Choose',
      options: ['yes', 'no'],
      allowFreeText: false,
      timeoutMs: 30000,
    })
    expect(result.success).toBe(true)
  })

  it('rejects empty prompt', () => {
    const result = inputSchema.safeParse({ prompt: '' })
    expect(result.success).toBe(false)
  })

  it('applies defaults', () => {
    const result = inputSchema.safeParse({ prompt: 'Hello' })
    if (result.success) {
      expect(result.data.allowFreeText).toBe(true)
      expect(result.data.timeoutMs).toBe(120000)
    }
  })
})

describe('athena_notify schema', () => {
  it('validates required fields', () => {
    const result = athenaNotifySchema.safeParse({
      type: 'success',
      title: 'Done',
      message: 'Task complete',
      priority: 'normal',
    })
    expect(result.success).toBe(true)
  })

  it('rejects missing title', () => {
    const result = athenaNotifySchema.safeParse({
      type: 'info',
      message: 'Hello',
      priority: 'low',
    })
    expect(result.success).toBe(false)
  })
})

describe('athena_update_status schema', () => {
  it('validates required fields', () => {
    const result = athenaUpdateStatusSchema.safeParse({
      agentId: 'agent-1',
      status: 'running',
    })
    expect(result.success).toBe(true)
  })

  it('validates progress percentage', () => {
    const result = athenaUpdateStatusSchema.safeParse({
      agentId: 'a1',
      status: 'running',
      progress: 75,
    })
    expect(result.success).toBe(true)
  })

  it('rejects progress over 100', () => {
    const result = athenaUpdateStatusSchema.safeParse({
      agentId: 'a1',
      status: 'running',
      progress: 150,
    })
    expect(result.success).toBe(false)
  })
})

describe('athena_report_error schema', () => {
  it('validates required fields', () => {
    const result = athenaReportErrorSchema.safeParse({
      agentId: 'agent-1',
      error: 'Crashed',
      recoverable: false,
    })
    expect(result.success).toBe(true)
  })

  it('defaults recoverable to true', () => {
    const result = athenaReportErrorSchema.safeParse({
      agentId: 'agent-1',
      error: 'Minor issue',
    })
    if (result.success) {
      expect(result.data.recoverable).toBe(true)
    }
  })
})

describe('athena_report_completion schema', () => {
  it('validates required fields', () => {
    const result = athenaReportCompletionSchema.safeParse({
      agentId: 'agent-1',
      summary: 'Done building',
    })
    expect(result.success).toBe(true)
  })

  it('validates with artifacts and metrics', () => {
    const result = athenaReportCompletionSchema.safeParse({
      agentId: 'agent-1',
      summary: 'All done',
      artifacts: ['src/foo.ts'],
      metrics: { filesChanged: 5, linesAdded: 120 },
      duration: 300,
    })
    expect(result.success).toBe(true)
  })
})

describe('control stubs schema', () => {
  it('validates control_pause', () => {
    const result = controlPauseSchema.safeParse({ paneId: 'pane-1' })
    expect(result.success).toBe(true)
  })

  it('validates control_resume', () => {
    const result = controlResumeSchema.safeParse({ paneId: 'pane-1' })
    expect(result.success).toBe(true)
  })

  it('validates control_cancel with force', () => {
    const result = controlCancelSchema.safeParse({ paneId: 'pane-1', force: true })
    expect(result.success).toBe(true)
  })

  it('control_cancel defaults force to false', () => {
    const result = controlCancelSchema.safeParse({ paneId: 'pane-1' })
    if (result.success) {
      expect(result.data.force).toBe(false)
    }
  })
})
