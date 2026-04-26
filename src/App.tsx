import { useState, useEffect, useRef } from 'react'
import { Minus, Square, X, Copy, Plus, Settings, Zap, Users, ChevronRight, Layers, FolderOpen, Bot, Brain } from 'lucide-react'
import { PanelGroup, Panel, PanelResizeHandle } from 'react-resizable-panels'
import { ErrorBoundary } from './components/shared/ErrorBoundary'
import { useUIStore } from './store/uiStore'
import { useWorkspaceStore } from './store/workspaceStore'
import { useTaskStore } from './store/taskStore'
import { Sidebar } from './components/Sidebar/Sidebar'
import { WorkspaceTabs } from './components/Workspace/WorkspaceTabs'
import { TerminalGrid } from './components/Terminal/TerminalGrid'
import { NewSpaceModal } from './components/Workspace/NewSpaceModal'
import { SettingsModal } from './components/Settings/SettingsModal'
import { BrowserPanel } from './components/Browser/BrowserPanel'
import { KanbanBoard } from './components/Kanban/KanbanBoard'
import { SwarmBoard } from './components/Swarm/SwarmBoard'
import { SwarmModal } from './components/Swarm/SwarmModal'
import { ToastContainer } from './components/shared/Toast'
import { AthenaPanel } from './components/Athena/AthenaPanel'
import { useAthenaStore } from './store/athenaStore'
import { NotificationBell } from './components/Notifications/NotificationBell'
import { themes, applyTheme, defaultTheme } from './themes/themes'
import type { ThemeName } from './types/theme'

export default function App() {
  const [platform, setPlatform] = useState<string>('darwin')
  const [isMaximized, setIsMaximized] = useState(false)
  const [showNewSpace, setShowNewSpace] = useState(false)
  const [showSwarmModal, setShowSwarmModal] = useState(false)
  const [mountedSpaces, setMountedSpaces] = useState<Set<string>>(new Set())
  const isHydrating = useRef(true)

  const {
    sidebarOpen, settingsOpen, toggleSettings, theme,
    browserOpen, toggleBrowser,
    activePanel, setActivePanel, toggleSidebar,
  } = useUIStore()

  const { spaces, activeSpaceId } = useWorkspaceStore()
  const activeSpace = spaces.find((s) => s.id === activeSpaceId)

  useEffect(() => {
    if (activeSpaceId) {
      setMountedSpaces((prev) => {
        if (prev.has(activeSpaceId)) return prev
        const next = new Set(prev)
        next.add(activeSpaceId)
        return next
      })
    }
  }, [activeSpaceId])

  useEffect(() => {
    window.athena.window.platform().then(setPlatform)
    window.athena.window.isMaximized().then(setIsMaximized)
  }, [])

  useEffect(() => {
    applyTheme(defaultTheme)
  }, [])

  useEffect(() => {
    window.athena.store.get('theme').then((saved: ThemeName | undefined) => {
      if (saved && themes[saved]) {
        useUIStore.getState().setTheme(saved)
        applyTheme(themes[saved])
      }
    })
    window.athena.store.get('spaces').then((saved: any) => {
      if (saved && Array.isArray(saved) && saved.length > 0) {
        useWorkspaceStore.getState().setSpaces(saved)
        useWorkspaceStore.getState().setActiveSpace(saved[saved.length - 1].id)
      }
      isHydrating.current = false
    })
    window.athena.store.get('tasks').then((saved: any) => {
      if (saved && Array.isArray(saved)) {
        useTaskStore.getState().setTasks(saved)
      }
    })
    window.athena.store.get('athena-model').then((saved: any) => {
      if (typeof saved === 'string') useAthenaStore.getState().setModel(saved)
    })
    window.athena.store.get('athena-bypassMode').then((saved: any) => {
      if (typeof saved === 'boolean') useAthenaStore.getState().setBypassMode(saved)
    })
    window.athena.store.get('athena-autoLaunch').then((saved: any) => {
      if (typeof saved === 'boolean') useAthenaStore.getState().setAutoLaunch(saved)
    })
    window.athena.store.get('athena-customAgents').then((saved: any) => {
      if (saved && Array.isArray(saved)) {
        useAthenaStore.getState().setCustomAgents(saved);
      }
    })
  }, [])


  useEffect(() => {
        const unsubClose = window.athena.pty.onAthenaClosePanes((paneIds: string[]) => {
      const state = useWorkspaceStore.getState();
      const activeSpaceId = state.activeSpaceId;
      if (!activeSpaceId || !Array.isArray(paneIds)) return;
      
      // Let's drop them physically from the active space
      paneIds.forEach(paneId => {
        state.removePaneFromSpace(activeSpaceId, paneId);
      });
    });

    const unsub = window.athena.pty.onAthenaSpawn((data: any) => {
      const spaceId = useWorkspaceStore.getState().activeSpaceId;
      if (!spaceId) return;
      useWorkspaceStore.getState().addPaneToSpace(spaceId, {
        id: data.id,
        agentType: data.agentType || 'claude',
        customCmd: data.agentCmd,
        label: '' // Ensure no forced hardcoded names!
      });
    });
    return () => {
      unsub();
      unsubClose();
    };
  }, []);

  useEffect(() => {
    if (isHydrating.current) return
    if (spaces.length > 0) {
      window.athena.store.set('spaces', spaces)
    }
  }, [spaces])

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const mod = platform === 'darwin' ? e.metaKey : e.ctrlKey

      if (mod && e.key === 't') { e.preventDefault(); setShowNewSpace(true) }
      if (mod && e.key === ',') { e.preventDefault(); useUIStore.getState().toggleSettings() }
      if (mod && e.key === '\\') { e.preventDefault(); useUIStore.getState().toggleSidebar() }
      if (mod && e.key === 'b') { e.preventDefault(); useUIStore.getState().toggleBrowser() }
      if (mod && e.key === 'j') { e.preventDefault(); useAthenaStore.getState().toggleOpen() }
      if (mod && e.key === 'k') {
        e.preventDefault()
        const currentPanel = useUIStore.getState().activePanel
        useUIStore.getState().setActivePanel(currentPanel === 'kanban' ? 'terminals' : 'kanban')
      }
      if (mod && e.shiftKey && e.key === 'S') {
        e.preventDefault()
        setShowSwarmModal(true)
      }
      if (mod && e.key >= '1' && e.key <= '9') {
        e.preventDefault()
        const idx = parseInt(e.key) - 1
        if (spaces[idx]) {
          useWorkspaceStore.getState().setActiveSpace(spaces[idx].id)
        }
      }
      if (e.key === 'Escape') {
        setShowNewSpace(false)
        setShowSwarmModal(false)
        if (useUIStore.getState().settingsOpen) useUIStore.getState().toggleSettings()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [platform, spaces])


  const isMac = platform === 'darwin'

  const renderMainContent = () => {
    if (!activeSpace) return <EmptyState onNewSpace={() => setShowNewSpace(true)} />

    switch (activePanel) {
      case 'kanban':
        return <KanbanBoard />
      case 'swarm':
        return <SwarmBoard />
      case 'terminals':
      default:
        return (
          <div className="flex-1 h-full w-full min-h-0 relative">
            {spaces.filter((space) => mountedSpaces.has(space.id)).map((space) => (
              <div
                key={space.id}
                className="absolute inset-0"
                style={{ display: space.id === activeSpaceId ? 'flex' : 'none' }}
              >
                <TerminalGrid space={space} />
              </div>
            ))}
          </div>
        )
    }
  }

  return (
    <ErrorBoundary>
    <div className="h-screen w-screen flex flex-col overflow-hidden" style={{ background: 'var(--bg)' }}>
      {/* Titlebar */}
      <div
        className="drag-region flex items-center shrink-0 border-b"
        style={{ height: 38, borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
      >
        {isMac && <div className="w-[72px] shrink-0" />}

        {!isMac && (
          <div className="flex items-center gap-1 px-3 no-drag shrink-0">
            <Zap size={16} style={{ color: 'var(--accent)' }} />
            <span className="text-xs font-semibold tracking-wide" style={{ color: 'var(--text)' }}>
              ATHENA
            </span>
          </div>
        )}

        <div className="flex-1 flex items-center justify-center gap-1 px-2 min-w-0">
          <WorkspaceTabs onNewTab={() => setShowNewSpace(true)} />
        </div>

        <div className="flex items-center gap-1 pr-2 no-drag shrink-0">
          {/* Panel switcher */}
          {activeSpace && (
            <div className="flex items-center gap-0.5 mr-1">
              {(['terminals', 'kanban', 'swarm'] as const).map((p) => (
                <button
                  key={p}
                  onClick={() => setActivePanel(p)}
                  className="px-2 py-0.5 rounded text-[10px] font-medium transition-colors capitalize"
                  style={{
                    background: activePanel === p ? 'var(--bgTertiary)' : 'transparent',
                    color: activePanel === p ? 'var(--text)' : 'var(--textDim)',
                  }}
                >
                  {p}
                </button>
              ))}
            </div>
          )}

          <button
            onClick={() => useAthenaStore.getState().toggleOpen()}
            className="p-1.5 rounded-md hover:bg-white/10 transition-colors"
            title="Athena (Cmd+J)"
          >
            <Brain size={13} style={{ color: 'var(--textMuted)' }} />
          </button>
          <button
            onClick={() => setShowSwarmModal(true)}
            className="p-1.5 rounded-md hover:bg-white/10 transition-colors"
            title="Launch Swarm"
          >
            <Users size={13} style={{ color: 'var(--textMuted)' }} />
          </button>
          <NotificationBell />
          <button
            onClick={toggleSettings}
            className="p-1.5 rounded-md hover:bg-white/10 transition-colors"
            title="Settings"
          >
            <Settings size={13} style={{ color: 'var(--textMuted)' }} />
          </button>
        </div>

        {!isMac && (
          <div className="flex items-center shrink-0 no-drag">
            <button
              onClick={() => window.athena.window.minimize()}
              className="h-[38px] w-[46px] flex items-center justify-center hover:bg-white/10 transition-colors"
            >
              <Minus size={14} style={{ color: 'var(--textMuted)' }} />
            </button>
            <button
              onClick={() => { window.athena.window.maximize(); setIsMaximized(!isMaximized) }}
              className="h-[38px] w-[46px] flex items-center justify-center hover:bg-white/10 transition-colors"
            >
              {isMaximized ? <Copy size={12} style={{ color: 'var(--textMuted)' }} /> : <Square size={12} style={{ color: 'var(--textMuted)' }} />}
            </button>
            <button
              onClick={() => window.athena.window.close()}
              className="h-[38px] w-[46px] flex items-center justify-center hover:bg-red-500/80 transition-colors"
            >
              <X size={14} style={{ color: 'var(--textMuted)' }} />
            </button>
          </div>
        )}
      </div>

      {/* Main content */}
      <div className="flex flex-1 min-h-0">
        {sidebarOpen ? (
          <Sidebar onNewSpace={() => setShowNewSpace(true)} />
        ) : (
          <SidebarRail onExpand={toggleSidebar} />
        )}

        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <div className="flex-1 flex min-h-0">
            <div className="flex-1 min-w-0 min-h-0 flex flex-col" style={{ flexBasis: browserOpen ? '60%' : '100%' }}>
              {renderMainContent()}
            </div>
            {browserOpen && (
              <>
                <div className="w-[3px] shrink-0 cursor-col-resize hover:bg-[var(--accent)] transition-colors" style={{ background: 'var(--border)' }} />
                <div className="min-w-0 min-h-0 flex flex-col overflow-hidden" style={{ flexBasis: '40%' }}>
                  <BrowserPanel />
                </div>
              </>
            )}
          </div>
        </div>

        <AthenaPanel />
      </div>

      {/* Status bar */}
      <div
        className="shrink-0 flex items-center px-3 border-t text-[11px]"
        style={{
          height: 22,
          borderColor: 'var(--border)',
          background: 'var(--bgSecondary)',
          color: 'var(--textDim)',
        }}
      >
        <span>{activeSpace?.name ?? 'No workspace'}</span>
        <span className="mx-2">|</span>
        <span>{activeSpace ? `${activeSpace.panes.length} panes` : ''}</span>
        <span className="mx-2">|</span>
        <span className="capitalize">{activePanel}</span>
        <div className="flex-1" />
        <span className="capitalize">{theme}</span>
      </div>

      {/* Modals & overlays */}
      {showNewSpace && <NewSpaceModal onClose={() => setShowNewSpace(false)} />}
      {showSwarmModal && <SwarmModal onClose={() => setShowSwarmModal(false)} />}
      {settingsOpen && <SettingsModal onClose={toggleSettings} />}
      <ToastContainer />
    </div>
    </ErrorBoundary>
  )
}

function SidebarRail({ onExpand }: { onExpand: () => void }) {
  const { setSidebarSection } = useUIStore()

  const handleSectionClick = (section: 'spaces' | 'agents') => {
    setSidebarSection(section)
    onExpand()
  }

  return (
    <div
      className="shrink-0 flex flex-col items-center py-2 gap-2 border-r"
      style={{
        width: 28,
        background: 'var(--bgSecondary)',
        borderColor: 'var(--border)',
      }}
    >
      <button
        onClick={onExpand}
        className="p-1 rounded hover:bg-white/10 transition-colors"
        title="Expand sidebar"
      >
        <ChevronRight size={14} style={{ color: 'var(--textMuted)' }} />
      </button>
      <button
        onClick={() => handleSectionClick('spaces')}
        className="p-1 rounded hover:bg-white/10 transition-colors"
        title="Spaces"
      >
        <Layers size={12} style={{ color: 'var(--textDim)' }} />
      </button>
      <button
        onClick={() => handleSectionClick('files')}
        className="p-1 rounded hover:bg-white/10 transition-colors"
        title="Files"
      >
        <FolderOpen size={12} style={{ color: 'var(--textDim)' }} />
      </button>
      <button
        onClick={() => handleSectionClick('agents')}
        className="p-1 rounded hover:bg-white/10 transition-colors"
        title="Agents"
      >
        <Bot size={12} style={{ color: 'var(--textDim)' }} />
      </button>
    </div>
  )
}

function EmptyState({ onNewSpace }: { onNewSpace: () => void }) {
  return (
    <div className="flex-1 flex flex-col items-center justify-center gap-6" style={{ color: 'var(--textDim)' }}>
      <div className="flex flex-col items-center gap-2">
        <Zap size={48} style={{ color: 'var(--accent)', opacity: 0.4 }} />
        <h2 className="text-lg font-semibold" style={{ color: 'var(--textMuted)' }}>
          Athena's Core
        </h2>
        <p className="text-sm">Create a workspace to get started</p>
      </div>
      <button
        onClick={onNewSpace}
        className="flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors"
        style={{ background: 'var(--accent)', color: '#fff' }}
        onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--accentHover)')}
        onMouseLeave={(e) => (e.currentTarget.style.background = 'var(--accent)')}
      >
        <Plus size={16} />
        New Workspace
      </button>
    </div>
  )
}
