import { create } from 'zustand'

export interface OutputLine {
  paneId: string
  lineNum: number
  timestamp: number
  text: string
}

export interface AgentOutputInfo {
  paneId: string
  agentType: string
  lineCount: number
  createdAt: number
  lastActivityAt: number
}

interface SubscriptionState {
  subscriptionId: string | null
  paneId: string | null
  active: boolean
}

interface AgentOutputState {
  buffers: Record<string, OutputLine[]>
  agents: AgentOutputInfo[]
  selectedPaneId: string | null
  subscription: SubscriptionState
  inspectorOpen: boolean
  autoScroll: boolean

  setLines: (paneId: string, lines: OutputLine[]) => void
  appendLine: (line: OutputLine) => void
  clearBuffer: (paneId: string) => void
  setAgents: (agents: AgentOutputInfo[]) => void
  selectAgent: (paneId: string | null) => void
  setSubscription: (sub: SubscriptionState) => void
  clearSubscription: () => void
  setInspectorOpen: (open: boolean) => void
  setAutoScroll: (auto: boolean) => void
}

const MAX_LINES_PER_BUFFER = 5000

export const useAgentOutputStore = create<AgentOutputState>((set) => ({
  buffers: {},
  agents: [],
  selectedPaneId: null,
  subscription: { subscriptionId: null, paneId: null, active: false },
  inspectorOpen: false,
  autoScroll: true,

  setLines: (paneId, lines) =>
    set((s) => ({
      buffers: {
        ...s.buffers,
        [paneId]: lines.slice(-MAX_LINES_PER_BUFFER),
      },
    })),

  appendLine: (line) =>
    set((s) => {
      const existing = s.buffers[line.paneId] ?? []
      const next = [...existing, line]
      if (next.length > MAX_LINES_PER_BUFFER) {
        next.splice(0, next.length - MAX_LINES_PER_BUFFER)
      }
      return {
        buffers: {
          ...s.buffers,
          [line.paneId]: next,
        },
      }
    }),

  clearBuffer: (paneId) =>
    set((s) => {
      const { [paneId]: _, ...rest } = s.buffers
      return { buffers: rest }
    }),

  setAgents: (agents) => set({ agents }),

  selectAgent: (paneId) => set({ selectedPaneId: paneId }),

  setSubscription: (sub) => set({ subscription: sub }),

  clearSubscription: () =>
    set({ subscription: { subscriptionId: null, paneId: null, active: false } }),

  setInspectorOpen: (open) => set({ inspectorOpen: open }),

  setAutoScroll: (auto) => set({ autoScroll: auto }),
}))
