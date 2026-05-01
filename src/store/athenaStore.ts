import { create } from 'zustand'
import type { ExclusivePanel } from './panelManager'
import { activatePanel, togglePanel, registerAthenaStore } from './panelManager'

export interface ImageAttachment {
  id: string
  base64: string
  mediaType: 'image/jpeg' | 'image/png' | 'image/gif' | 'image/webp'
  name?: string
}

export interface PlanStepBlock {
  id: string
  title: string
  description: string
  agent_type: string
  status: 'pending' | 'in_progress' | 'completed' | 'failed'
  assigned_pane_id?: string
  result_summary?: string
}

export interface PlanBlock {
  type: 'plan'
  planId: string
  goal: string
  steps: PlanStepBlock[]
  status: 'pending' | 'in_progress' | 'completed' | 'failed'
}

export interface AskUserBlock {
  type: 'ask_user'
  requestId: string
  question: string
  options: { label: string; description: string }[]
  answered?: boolean
  selectedAnswer?: string
}

export interface EvaluationBlock {
  type: 'evaluation'
  planId: string
  overallStatus: string
  stepEvaluations: { step_id: string; status: string; summary: string }[]
  nextAction: string
  reasoning: string
}

export type ContentBlock = PlanBlock | AskUserBlock | EvaluationBlock

export interface AthenaMessage {
  id: string
  role: 'user' | 'athena'
  content: string
  timestamp: number
  isError?: boolean
  images?: ImageAttachment[]
  blocks?: ContentBlock[]
}

export interface CustomAgent {
  id: string
  name: string
  command: string
}

interface AthenaState {
  messages: AthenaMessage[]
  isOpen: boolean
  isLoading: boolean
  isStreaming: boolean
  streamingStatus: string | null
  error: string | null
  model: string
  provider: string
  bypassMode: boolean
  autoLaunch: boolean
  customAgents: CustomAgent[]
  addMessage: (msg: AthenaMessage) => void
  setOpen: (open: boolean) => void
  toggleOpen: () => void
  setLoading: (loading: boolean) => void
  setStreaming: (streaming: boolean) => void
  setStreamingStatus: (status: string | null) => void
  setError: (error: string | null) => void
  clearError: () => void
  setModel: (model: string) => void
  setProvider: (provider: string) => void
  setBypassMode: (bypass: boolean) => void
  setAutoLaunch: (auto: boolean) => void
  addCustomAgent: (agent: CustomAgent) => void
  setCustomAgents: (agents: CustomAgent[]) => void
  removeCustomAgent: (id: string) => void
  clearMessages: () => void
  setMessages: (messages: AthenaMessage[]) => void
}

export const useAthenaStore = create<AthenaState>((set, get) => {
  const storeApi = {
    setState: (partial: { isOpen: boolean }) => set(partial),
    getState: () => ({ isOpen: get().isOpen }),
  }

  registerAthenaStore(storeApi)

  return {
    messages: [],
    isOpen: false,
    isLoading: false,
    isStreaming: false,
    streamingStatus: null,
    error: null,
    model: 'claude',
    provider: 'anthropic',
    bypassMode: true,
    autoLaunch: true,
    customAgents: [],

    addMessage: (msg) => set((s) => ({ messages: [...s.messages, msg].slice(-100) })),

    setOpen: (open: boolean) => {
      activatePanel(open ? 'athena' : null)
    },

    toggleOpen: () => {
      togglePanel('athena')
    },

    setLoading: (loading) => set({ isLoading: loading }),
    setStreaming: (streaming) => set({ isStreaming: streaming }),
    setStreamingStatus: (status) => set({ streamingStatus: status }),
    setError: (error) => set({ error }),
    clearError: () => set({ error: null }),

    setModel: (model) => set({ model }),
    setProvider: (provider) => set({ provider }),
    setBypassMode: (bypass) => set({ bypassMode: bypass }),
    setAutoLaunch: (auto) => set({ autoLaunch: auto }),
    addCustomAgent: (agent) => set((s) => ({ customAgents: [...s.customAgents, agent] })),
    setCustomAgents: (agents) => set({ customAgents: agents }),
    removeCustomAgent: (id) =>
      set((s) => ({ customAgents: s.customAgents.filter((a) => a.id !== id) })),
    clearMessages: () => set({ messages: [], error: null }),
    setMessages: (messages) => set({ messages, error: null }),
  }
})
