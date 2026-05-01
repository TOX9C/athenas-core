import { create } from 'zustand'
import type { Space, PaneConfig, GridTemplate } from '../types/workspace'

export function gridForPaneCount(count: number): GridTemplate {
  if (count <= 1) return '1x1'
  if (count <= 2) return '1x2'
  if (count <= 4) return '2x2'
  if (count <= 6) return '2x3'
  if (count <= 9) return '3x3'
  if (count <= 12) return '3x4'
  return '4x4'
}

interface WorkspaceState {
  spaces: Space[]
  activeSpaceId: string | null
  setActiveSpace: (id: string) => void
  addSpace: (space: Space) => void
  removeSpace: (id: string) => void
  updateSpace: (id: string, updates: Partial<Space>) => void
  addPaneToSpace: (spaceId: string, pane: PaneConfig) => void
  removePaneFromSpace: (spaceId: string, paneId: string) => void
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
          s.activeSpaceId === id ? (filtered[filtered.length - 1]?.id ?? null) : s.activeSpaceId,
      }
    }),
  updateSpace: (id, updates) =>
    set((s) => ({
      spaces: s.spaces.map((sp) => (sp.id === id ? { ...sp, ...updates } : sp)),
    })),
  addPaneToSpace: (spaceId, pane) =>
    set((s) => ({
      spaces: s.spaces.map((sp) => {
        if (sp.id !== spaceId) return sp
        const newPanes = [...sp.panes, pane]
        return {
          ...sp,
          panes: newPanes,
          grid: gridForPaneCount(newPanes.length),
        }
      }),
    })),
  removePaneFromSpace: (spaceId, paneId) =>
    set((s) => ({
      spaces: s.spaces.map((sp) => {
        if (sp.id !== spaceId) return sp
        const newPanes = sp.panes.filter((p) => p.id !== paneId)
        return {
          ...sp,
          panes: newPanes,
          grid: gridForPaneCount(newPanes.length),
        }
      }),
    })),
  setSpaces: (spaces) => set({ spaces }),
}))
