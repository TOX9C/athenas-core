import { create } from 'zustand'
import type { ThemeName } from '../types/theme'
import { activatePanel, registerUIStore } from './panelManager'

export type SidebarSection = 'spaces' | 'files' | 'agents' | 'plugins'

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

export const useUIStore = create<UIState>((set, get) => {
  const storeApi = {
    getState: () => ({
      browserOpen: get().browserOpen,
      editorOpen: get().editorOpen,
    }),
    setState: (partial: any) => set(partial),
  }

  registerUIStore(storeApi)

  return {
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
    toggleBrowser: () => {
      const { browserOpen } = get()
      activatePanel(browserOpen ? null : 'browser')
    },
    toggleEditor: () => {
      const { editorOpen } = get()
      activatePanel(editorOpen ? null : 'editor')
    },
    toggleSettings: () => set((s) => ({ settingsOpen: !s.settingsOpen })),
    setTheme: (t) => set({ theme: t }),
    setFontFamily: (f) => set({ fontFamily: f }),
    setFontSize: (s) => set({ fontSize: s }),
  }
})
