import { create } from 'zustand'
import type { SidebarSection } from './uiStore'

export interface LayoutState {
  sidebarOpen: boolean
  sidebarWidth: number
  activeSidebarSection: SidebarSection
  activePanel: 'terminals' | 'swarm' | 'kanban'
  browserOpen: boolean
  editorOpen: boolean
  activeSpaceId: string | null
  athenaOpen: boolean
  panelSizes: Record<string, number>
}

const DEFAULT_LAYOUT: LayoutState = {
  sidebarOpen: true,
  sidebarWidth: 240,
  activeSidebarSection: 'spaces',
  activePanel: 'terminals',
  browserOpen: false,
  editorOpen: false,
  activeSpaceId: null,
  athenaOpen: false,
  panelSizes: {},
}

interface LayoutStore extends LayoutState {
  setLayout: (partial: Partial<LayoutState>) => void
  setPanelSize: (panelId: string, size: number) => void
  resetLayout: () => void
  hydrateFromSaved: (saved: Partial<LayoutState>) => void
}

let _persistTimer: ReturnType<typeof setTimeout> | null = null

function schedulePersist(state: LayoutState) {
  if (_persistTimer) clearTimeout(_persistTimer)
  _persistTimer = setTimeout(() => {
    const {
      setLayout: _,
      setPanelSize: _2,
      resetLayout: _3,
      hydrateFromSaved: _4,
      ...serializable
    } = state as LayoutStore
    window.athena.store.set('layout', serializable)
  }, 300)
}

export const useLayoutStore = create<LayoutStore>((set, get) => ({
  ...DEFAULT_LAYOUT,

  setLayout: (partial) => {
    set(partial)
    const state = get()
    schedulePersist({
      sidebarOpen: state.sidebarOpen,
      sidebarWidth: state.sidebarWidth,
      activeSidebarSection: state.activeSidebarSection,
      activePanel: state.activePanel,
      browserOpen: state.browserOpen,
      editorOpen: state.editorOpen,
      activeSpaceId: state.activeSpaceId,
      athenaOpen: state.athenaOpen,
      panelSizes: state.panelSizes,
    })
  },

  setPanelSize: (panelId, size) => {
    set((s) => ({ panelSizes: { ...s.panelSizes, [panelId]: size } }))
    const state = get()
    schedulePersist({
      sidebarOpen: state.sidebarOpen,
      sidebarWidth: state.sidebarWidth,
      activeSidebarSection: state.activeSidebarSection,
      activePanel: state.activePanel,
      browserOpen: state.browserOpen,
      editorOpen: state.editorOpen,
      activeSpaceId: state.activeSpaceId,
      athenaOpen: state.athenaOpen,
      panelSizes: state.panelSizes,
    })
  },

  resetLayout: () => {
    if (_persistTimer) clearTimeout(_persistTimer)
    set(DEFAULT_LAYOUT)
    window.athena.store.set('layout', DEFAULT_LAYOUT)
  },

  hydrateFromSaved: (saved) => {
    set({ ...DEFAULT_LAYOUT, ...saved })
  },
}))
