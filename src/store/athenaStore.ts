import { create } from 'zustand'

export interface AthenaMessage {
  id: string
  role: 'user' | 'athena'
  content: string
  timestamp: number
}

export interface CustomAgent {
  id: string;
  name: string;
  command: string;
}

interface AthenaState {
  messages: AthenaMessage[]
  isOpen: boolean
  isPtyReady: boolean
  model: string
  bypassMode: boolean
  autoLaunch: boolean
  customAgents: CustomAgent[]
  addMessage: (msg: AthenaMessage) => void
  setOpen: (open: boolean) => void
  toggleOpen: () => void
  setPtyReady: (ready: boolean) => void
  setModel: (model: string) => void
  setBypassMode: (bypass: boolean) => void
  setAutoLaunch: (auto: boolean) => void
  addCustomAgent: (agent: CustomAgent) => void
  removeCustomAgent: (id: string) => void
  clearMessages: () => void
}

export const useAthenaStore = create<AthenaState>((set) => ({
  messages: [],
  isOpen: false,
  isPtyReady: false,
  model: 'claude',
  bypassMode: true,
  autoLaunch: true,
  customAgents: [],
  addMessage: (msg) => set((s) => ({ messages: [...s.messages, msg] })),
  setOpen: (open) => set({ isOpen: open }),
  toggleOpen: () => set((s) => ({ isOpen: !s.isOpen })),
  setPtyReady: (ready) => set({ isPtyReady: ready }),
  setModel: (model) => set({ model }),
  setBypassMode: (bypass) => set({ bypassMode: bypass }),
  setAutoLaunch: (auto) => set({ autoLaunch: auto }),
  addCustomAgent: (agent) => set((s) => ({ customAgents: [...s.customAgents, agent] })),
  removeCustomAgent: (id) => set((s) => ({ customAgents: s.customAgents.filter(a => a.id !== id) })),
  clearMessages: () => set({ messages: [] }),
}))
