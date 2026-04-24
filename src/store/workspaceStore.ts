import { create } from 'zustand'
import type { Space } from '../types/workspace'

interface WorkspaceState {
  spaces: Space[]
  activeSpaceId: string | null
  setActiveSpace: (id: string) => void
  addSpace: (space: Space) => void
  removeSpace: (id: string) => void
  updateSpace: (id: string, updates: Partial<Space>) => void
  setSpaces: (spaces: Space[]) => void
}

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  spaces: [],
  activeSpaceId: null,
  setActiveSpace: (id) => set({ activeSpaceId: id }),
  addSpace: (space) =>
    set((s) => ({
      spaces: [...s.spaces, space],
      activeSpaceId: space.id,
    })),
  removeSpace: (id) =>
    set((s) => {
      const filtered = s.spaces.filter((sp) => sp.id !== id)
      return {
        spaces: filtered,
        activeSpaceId:
          s.activeSpaceId === id
            ? filtered[filtered.length - 1]?.id ?? null
            : s.activeSpaceId,
      }
    }),
  updateSpace: (id, updates) =>
    set((s) => ({
      spaces: s.spaces.map((sp) => (sp.id === id ? { ...sp, ...updates } : sp)),
    })),
  setSpaces: (spaces) => set({ spaces }),
}))
