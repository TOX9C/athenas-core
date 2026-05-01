import { z } from 'zod'
import type { OutputBufferManager } from '../output-buffer.js'
import type { StreamSubscription } from '../types/index.js'

export const athenaStreamOutputSchema = z.object({
  paneId: z.string().min(1).describe('The pane ID to stream output from'),
})

export type AthenaStreamOutputInput = z.infer<typeof athenaStreamOutputSchema>

const STREAM_TIMEOUT_MS = 60_000
const MAX_STREAM_LINES = 100

export async function athenaStreamOutput(
  bufferManager: OutputBufferManager,
  input: AthenaStreamOutputInput,
): Promise<{ content: Array<{ type: 'text'; text: string }>; isError?: boolean }> {
  const existing = bufferManager.read(input.paneId)
  const recentLines = existing.slice(-20)

  return new Promise((resolve) => {
    const collected: string[] = []

    if (recentLines.length > 0) {
      const snapshot = recentLines
        .map((e) => `[${e.lineNumber}]${e.isStderr ? ' [stderr]' : ''} ${e.content}`)
        .join('\n')
      collected.push(`--- Snapshot (last ${recentLines.length} lines) ---\n${snapshot}`)
    }

    let sub: StreamSubscription | null = null
    let timer: ReturnType<typeof setTimeout> | null = setTimeout(() => {
      cleanup()
      resolve(buildStreamResult(collected, false))
    }, STREAM_TIMEOUT_MS)

    const cleanup = () => {
      if (timer) {
        clearTimeout(timer)
        timer = null
      }
      if (sub) {
        bufferManager.unsubscribe(sub)
        sub = null
      }
    }

    sub = bufferManager.subscribe(input.paneId, (entry) => {
      const line = `[${entry.lineNumber}]${entry.isStderr ? ' [stderr]' : ''} ${entry.content}`
      collected.push(line)

      if (collected.length >= MAX_STREAM_LINES) {
        cleanup()
        resolve(buildStreamResult(collected, true))
      }
    })
  })
}

function buildStreamResult(collected: string[], truncated: boolean) {
  const suffix = truncated
    ? '\n\n[Stream truncated — 100 lines reached. Call again to continue.]'
    : '\n\n[Stream ended — 60s timeout reached]'
  return {
    content: [
      {
        type: 'text' as const,
        text: collected.join('\n') + suffix,
      },
    ],
  }
}
