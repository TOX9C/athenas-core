import { describe, it, expect } from 'vitest'
import { controlPause } from '../src/tools/control-pause.js'
import { controlResume } from '../src/tools/control-resume.js'
import { controlCancel } from '../src/tools/control-cancel.js'

describe('Phase 2 stubs', () => {
  it('control_pause returns not yet available', async () => {
    const result = await controlPause({ paneId: 'pane-1' })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain('not yet available')
  })

  it('control_resume returns not yet available', async () => {
    const result = await controlResume({ paneId: 'pane-1' })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain('not yet available')
  })

  it('control_cancel returns not yet available', async () => {
    const result = await controlCancel({ paneId: 'pane-1', force: false })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain('not yet available')
  })
})
