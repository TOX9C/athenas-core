import { create } from 'zustand'
import type { ThemeName } from '../types/theme'

export type SidebarSection = 'spaces' | 'files' | 'agents'

interface UIState {
  sidebarOpen: boolean
  sidebarWidth: number
  activeSidebarSection: SidebarSection
  activePanel: 'terminals' | 'swarm' | 'kanban'
  browserOpen: boolean
  editorOpen: boolean
  settingsOpen: boolean
  theme: ThemeName
  fontFamily: string
  fontSize: number
  toggleSidebar: () => void
  setSidebarWidth: (w: number) => void
  setSidebarSection: (s: SidebarSection) => void
  setActivePanel: (p: 'terminals' | 'swarm' | 'kanban') => void
  toggleBrowser: () => void
  toggleEditor: () => void
  toggleSettings: () => void
  setTheme: (t: ThemeName) => void
  setFontFamily: (f: string) => void
  setFontSize: (s: number) => void
}

export const useUIStore = create<UIState>((set) => ({
  sidebarOpen: true,
  sidebarWidth: 240,
  activeSidebarSection: 'spaces',
  activePanel: 'terminals',
  browserOpen: false,
  editorOpen: false,
  settingsOpen: false,
  theme: 'void',
  fontFamily: "'JetBrains Mono', monospace",
  fontSize: 14,
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
  setSidebarWidth: (w) => set({ sidebarWidth: w }),
  setSidebarSection: (s) => set({ activeSidebarSection: s }),
  setActivePanel: (p) => set({ activePanel: p }),
  toggleBrowser: () => set((s) => ({ browserOpen: !s.browserOpen })),
  toggleEditor: () => set((s) => ({ editorOpen: !s.editorOpen })),
  toggleSettings: () => set((s) => ({ settingsOpen: !s.settingsOpen })),
  setTheme: (t) => set({ theme: t }),
  setFontFamily: (f) => set({ fontFamily: f }),
  setFontSize: (s) => set({ fontSize: s }),
}))
