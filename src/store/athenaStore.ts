import { create } from 'zustand'

export interface AthenaMessage {
  id: string
  role: 'user' | 'athena'
  content: string
  timestamp: number
}

interface AthenaState {
  messages: AthenaMessage[]
  isOpen: boolean
  isPtyReady: boolean
  model: string
  bypassMode: boolean
  autoLaunch: boolean
  customCommand: string
  addMessage: (msg: AthenaMessage) => void
  setOpen: (open: boolean) => void
  toggleOpen: () => void
  setPtyReady: (ready: boolean) => void
  setModel: (model: string) => void
  setBypassMode: (bypass: boolean) => void
  setAutoLaunch: (auto: boolean) => void
  setCustomCommand: (cmd: string) => void
  clearMessages: () => void
}

export const useAthenaStore = create<AthenaState>((set) => ({
  messages: [],
  isOpen: false,
  isPtyReady: false,
  model: 'claude',
  bypassMode: true,
  autoLaunch: true,
  customCommand: '',
  addMessage: (msg) => set((s) => ({ messages: [...s.messages, msg] })),
  setOpen: (open) => set({ isOpen: open }),
  toggleOpen: () => set((s) => ({ isOpen: !s.isOpen })),
  setPtyReady: (ready) => set({ isPtyReady: ready }),
  setModel: (model) => set({ model }),
  setBypassMode: (bypass) => set({ bypassMode: bypass }),
  setAutoLaunch: (auto) => set({ autoLaunch: auto }),
  setCustomCommand: (cmd) => set({ customCommand: cmd }),
  clearMessages: () => set({ messages: [] }),
}))
