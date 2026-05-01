import { create } from 'zustand'

export interface AgentStatus {
  paneId: string
  status:
    | 'idle'
    | 'thinking'
    | 'working'
    | 'waiting_for_input'
    | 'completed'
    | 'error'
    | 'cancelled'
    | 'disconnected'
  message?: string
  progress?: { current: number; total: number; label: string }
  lastUpdatedAt: number
}

interface AgentStatusState {
  statuses: Record<string, AgentStatus>
  updateStatus: (paneId: string, update: Partial<AgentStatus>) => void
  removeStatus: (paneId: string) => void
}

export const useAgentStatusStore = create<AgentStatusState>((set) => ({
  statuses: {},
  updateStatus: (paneId, update) =>
    set((s) => ({
      statuses: {
        ...s.statuses,
        [paneId]: { ...s.statuses[paneId], paneId, lastUpdatedAt: Date.now(), ...update },
      },
    })),
  removeStatus: (paneId) =>
    set((s) => {
      const { [paneId]: _, ...rest } = s.statuses
      return { statuses: rest }
    }),
}))
