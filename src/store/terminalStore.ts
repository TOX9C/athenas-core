import { create } from 'zustand'
import type { PtySession } from '../types/terminal'

interface TerminalState {
  sessions: Record<string, PtySession>
  setSession: (paneId: string, session: PtySession) => void
  updateSession: (paneId: string, updates: Partial<PtySession>) => void
  removeSession: (paneId: string) => void
}

export const useTerminalStore = create<TerminalState>((set) => ({
  sessions: {},
  setSession: (paneId, session) =>
    set((s) => ({ sessions: { ...s.sessions, [paneId]: session } })),
  updateSession: (paneId, updates) =>
    set((s) => ({
      sessions: {
        ...s.sessions,
        [paneId]: s.sessions[paneId]
          ? { ...s.sessions[paneId], ...updates }
          : s.sessions[paneId],
      },
    })),
  removeSession: (paneId) =>
    set((s) => {
      const next = { ...s.sessions }
      delete next[paneId]
      return { sessions: next }
    }),
}))
