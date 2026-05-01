import { create } from 'zustand'
import type { EditorFile } from '../types/editor'

interface EditorState {
  openFiles: EditorFile[]
  activeFilePath: string | null
  openFile: (file: EditorFile) => void
  closeFile: (path: string) => void
  setActiveFile: (path: string) => void
  updateFile: (path: string, updates: Partial<EditorFile>) => void
}

export const useEditorStore = create<EditorState>((set) => ({
  openFiles: [],
  activeFilePath: null,
  openFile: (file) =>
    set((s) => {
      const exists = s.openFiles.find((f) => f.path === file.path)
      if (exists) return { activeFilePath: file.path }
      return {
        openFiles: [...s.openFiles, file],
        activeFilePath: file.path,
      }
    }),
  closeFile: (path) =>
    set((s) => {
      const filtered = s.openFiles.filter((f) => f.path !== path)
      return {
        openFiles: filtered,
        activeFilePath:
          s.activeFilePath === path
            ? (filtered[filtered.length - 1]?.path ?? null)
            : s.activeFilePath,
      }
    }),
  setActiveFile: (path) => set({ activeFilePath: path }),
  updateFile: (path, updates) =>
    set((s) => ({
      openFiles: s.openFiles.map((f) => (f.path === path ? { ...f, ...updates } : f)),
    })),
}))
