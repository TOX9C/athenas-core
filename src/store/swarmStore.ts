import { create } from 'zustand'
import type { SwarmState } from '../types/swarm'

interface SwarmStoreState {
  activeSwarm: SwarmState | null
  setSwarm: (s: SwarmState | null) => void
  updateSwarm: (updates: Partial<SwarmState>) => void
}

export const useSwarmStore = create<SwarmStoreState>((set) => ({
  activeSwarm: null,
  setSwarm: (s) => set({ activeSwarm: s }),
  updateSwarm: (updates) =>
    set((state) => ({
      activeSwarm: state.activeSwarm
        ? { ...state.activeSwarm, ...updates }
        : null,
    })),
}))
