import { create } from 'zustand'
import { nanoid } from 'nanoid'
import type { PtySession, CommandBlock, ShellIntegrationEvent } from '../types/terminal'

const MAX_BLOCKS_PER_SESSION = 500

interface TerminalState {
  sessions: Record<string, PtySession>
  setSession: (paneId: string, session: PtySession) => void
  updateSession: (paneId: string, updates: Partial<PtySession>) => void
  removeSession: (paneId: string) => void
  handleShellIntegrationEvent: (event: ShellIntegrationEvent) => void
}

function trimBlocks(blocks: CommandBlock[]): CommandBlock[] {
  if (blocks.length <= MAX_BLOCKS_PER_SESSION) return blocks
  return blocks.slice(blocks.length - MAX_BLOCKS_PER_SESSION)
}

export const useTerminalStore = create<TerminalState>((set, get) => ({
  sessions: {},
  setSession: (paneId, session) => set((s) => ({ sessions: { ...s.sessions, [paneId]: session } })),
  updateSession: (paneId, updates) =>
    set((s) => ({
      sessions: {
        ...s.sessions,
        [paneId]: s.sessions[paneId] ? { ...s.sessions[paneId], ...updates } : s.sessions[paneId],
      },
    })),
  removeSession: (paneId) =>
    set((s) => {
      const next = { ...s.sessions }
      delete next[paneId]
      return { sessions: next }
    }),
  handleShellIntegrationEvent: (event) => {
    const { sessions } = get()
    const paneId = event.paneId
    const session = sessions[paneId]

    if (!session) {
      const fresh: PtySession = {
        paneId,
        status: 'idle',
        blocks: [],
      }
      set((s) => ({ sessions: { ...s.sessions, [paneId]: fresh } }))
    }

    const current = get().sessions[paneId]
    if (!current) return

    switch (event.type) {
      case 'commandStart': {
        const newBlock: CommandBlock = {
          id: nanoid(),
          command: event.command || '',
          output: '',
          exitCode: null,
          startedAt: event.timestamp,
          finishedAt: null,
          duration: null,
          collapsed: false,
        }
        const blocks = trimBlocks([...current.blocks, newBlock])
        set((s) => ({
          sessions: {
            ...s.sessions,
            [paneId]: {
              ...current,
              status: 'running',
              blocks,
              cwd: event.cwd ?? current.cwd,
            },
          },
        }))
        break
      }
      case 'commandExecuted': {
        break
      }
      case 'commandFinished': {
        const blocks = [...current.blocks]
        const activeIdx = blocks.findIndex(
          (b) => b.exitCode === null && b.startedAt <= event.timestamp,
        )
        if (activeIdx !== -1) {
          blocks[activeIdx] = {
            ...blocks[activeIdx],
            exitCode: event.exitCode ?? 0,
            finishedAt: event.timestamp,
            duration: event.duration ?? event.timestamp - blocks[activeIdx].startedAt,
          }
        }
        set((s) => ({
          sessions: {
            ...s.sessions,
            [paneId]: {
              ...current,
              status: 'idle',
              blocks,
              lastCommand: event.command || current.lastCommand,
              lastExitCode: event.exitCode ?? 0,
            },
          },
        }))
        break
      }
      case 'cwd': {
        set((s) => ({
          sessions: {
            ...s.sessions,
            [paneId]: {
              ...current,
              cwd: event.cwd ?? current.cwd,
            },
          },
        }))
        break
      }
      case 'prompt': {
        if (current.status !== 'idle') {
          set((s) => ({
            sessions: {
              ...s.sessions,
              [paneId]: { ...current, status: 'idle' },
            },
          }))
        }
        break
      }
      default:
        break
    }
  },
}))
