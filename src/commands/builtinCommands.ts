import {
  Plus,
  Settings,
  Zap,
  Brain,
  Users,
  LayoutGrid,
  PanelLeftClose,
  PanelLeftOpen,
  Globe,
  Terminal,
  Command,
  FilePlus,
  FileSearch,
  X,
  FolderOpen,
  FileText,
  Eye,
  EyeOff,
  Columns,
  RotateCcw,
  Keyboard,
} from 'lucide-react'
import type { Command as CmdType } from '../types/command'
import { useCommandStore } from '../store/commandStore'
import { useUIStore } from '../store/uiStore'
import { useAthenaStore } from '../store/athenaStore'
import { useWorkspaceStore } from '../store/workspaceStore'
import { useEditorStore } from '../store/editorStore'

let registered = false

export function registerBuiltinCommands() {
  if (registered) return
  registered = true

  const commands: CmdType[] = [
    {
      id: 'palette.open',
      label: 'Command Palette',
      category: 'navigation',
      description: 'Open the command palette to search and run commands',
      icon: Command,
      shortcut: 'Mod+Shift+P',
      keywords: ['search', 'find', 'run', 'action'],
      handler: () => useCommandStore.getState().toggle(),
    },
    {
      id: 'workspace.new',
      label: 'New Workspace',
      category: 'workspace',
      description: 'Create a new workspace tab',
      icon: Plus,
      shortcut: 'Mod+T',
      keywords: ['space', 'tab', 'create', 'add'],
      handler: () => {
        const el = document.querySelector('[data-new-space-trigger]') as HTMLElement | null
        el?.click()
      },
    },
    {
      id: 'workspace.close',
      label: 'Close Workspace',
      category: 'workspace',
      description: 'Close the current workspace tab',
      icon: X,
      shortcut: 'Mod+W',
      keywords: ['remove', 'tab', 'delete'],
      handler: () => {
        const { activeSpaceId, spaces, removeSpace } = useWorkspaceStore.getState()
        if (activeSpaceId && spaces.length > 0) {
          removeSpace(activeSpaceId)
        }
      },
    },
    {
      id: 'athena.toggle',
      label: 'Toggle Athena Chat',
      category: 'athena',
      description: 'Show or hide the Athena AI chat panel',
      icon: Brain,
      shortcut: 'Mod+J',
      keywords: ['ai', 'chat', 'assistant', 'llm'],
      handler: () => useAthenaStore.getState().toggleOpen(),
    },
    {
      id: 'panel.terminals',
      label: 'Switch to Terminals',
      category: 'panel',
      description: 'Show the terminal grid view',
      icon: Terminal,
      keywords: ['shell', 'pty', 'grid'],
      handler: () => useUIStore.getState().setActivePanel('terminals'),
    },
    {
      id: 'panel.kanban',
      label: 'Switch to Kanban Board',
      category: 'panel',
      description: 'Show the Kanban task board',
      icon: LayoutGrid,
      keywords: ['tasks', 'board', 'cards', 'todo'],
      handler: () => useUIStore.getState().setActivePanel('kanban'),
    },
    {
      id: 'panel.swarm',
      label: 'Switch to Swarm',
      category: 'panel',
      description: 'Show the agent swarm coordination view',
      icon: Users,
      keywords: ['agents', 'coordinate', 'workers'],
      handler: () => useUIStore.getState().setActivePanel('swarm'),
    },
    {
      id: 'sidebar.toggle',
      label: 'Toggle Sidebar',
      category: 'panel',
      description: 'Show or hide the sidebar',
      icon: PanelLeftOpen,
      shortcut: 'Mod+\\',
      keywords: ['explorer', 'files', 'tree', 'collapse'],
      handler: () => useUIStore.getState().toggleSidebar(),
    },
    {
      id: 'browser.toggle',
      label: 'Toggle Browser Panel',
      category: 'panel',
      description: 'Show or hide the embedded browser',
      icon: Globe,
      shortcut: 'Mod+B',
      keywords: ['web', 'preview', 'url'],
      handler: () => useUIStore.getState().toggleBrowser(),
    },
    {
      id: 'settings.toggle',
      label: 'Open Settings',
      category: 'settings',
      description: 'Open the settings modal',
      icon: Settings,
      shortcut: 'Mod+,',
      keywords: ['preferences', 'config', 'options'],
      handler: () => useUIStore.getState().toggleSettings(),
    },
    {
      id: 'swarm.launch',
      label: 'Launch Swarm',
      category: 'athena',
      description: 'Open the swarm configuration modal',
      icon: Zap,
      shortcut: 'Mod+Shift+S',
      keywords: ['agents', 'workers', 'coordinate', 'dispatch'],
      handler: () => {
        const el = document.querySelector('[data-swarm-trigger]') as HTMLElement | null
        el?.click()
      },
    },
    {
      id: 'file.new',
      label: 'New File',
      category: 'file',
      description: 'Create a new empty file in the editor',
      icon: FilePlus,
      shortcut: 'Mod+N',
      keywords: ['create', 'untitled', 'blank'],
      handler: () => {
        useEditorStore.getState().openFile({
          path: `untitled-${Date.now()}`,
          content: '',
          language: 'plaintext',
          isDirty: false,
          cursorPosition: { line: 1, column: 1 },
        })
      },
    },
    {
      id: 'file.open',
      label: 'Open File',
      category: 'file',
      description: 'Open a file from disk in the editor',
      icon: FolderOpen,
      shortcut: 'Mod+O',
      keywords: ['read', 'load', 'import', 'disk'],
      handler: () => {
        ;(window as any).athena?.fs?.showOpenDialog?.()
      },
    },
    {
      id: 'file.save',
      label: 'Save File',
      category: 'file',
      description: 'Save the current file to disk',
      icon: FileText,
      shortcut: 'Mod+S',
      keywords: ['write', 'disk', 'persist'],
      handler: () => {
        const { openFiles, activeFilePath } = useEditorStore.getState()
        const active = openFiles.find((f) => f.path === activeFilePath)
        if (active) {
          ;(window as any).athena?.fs?.writeFile?.(active.path, active.content)
        }
      },
    },
    {
      id: 'file.close',
      label: 'Close File',
      category: 'file',
      description: 'Close the current editor file',
      icon: X,
      shortcut: 'Mod+Shift+W',
      keywords: ['tab', 'editor', 'dismiss'],
      handler: () => {
        const { activeFilePath, closeFile } = useEditorStore.getState()
        if (activeFilePath) closeFile(activeFilePath)
      },
    },
    {
      id: 'file.search',
      label: 'Search in Files',
      category: 'file',
      description: 'Search across all files in the workspace',
      icon: FileSearch,
      shortcut: 'Mod+Shift+F',
      keywords: ['find', 'grep', 'ripgrep', 'content'],
      handler: () => {
        useUIStore.getState().setSidebarSection('files')
        if (!useUIStore.getState().sidebarOpen) {
          useUIStore.getState().toggleSidebar()
        }
      },
    },
    {
      id: 'terminal.new',
      label: 'New Terminal',
      category: 'terminal',
      description: 'Add a new terminal pane to the current workspace',
      icon: Terminal,
      shortcut: 'Mod+Shift+T',
      keywords: ['shell', 'pane', 'pty', 'spawn'],
      handler: () => {
        const { activeSpaceId, spaces, addPaneToSpace } = useWorkspaceStore.getState()
        if (!activeSpaceId) return
        const space = spaces.find((s) => s.id === activeSpaceId)
        if (!space) return
        const id = `pane-${Date.now()}`
        addPaneToSpace(activeSpaceId, {
          id,
          agentType: 'shell',
        })
      },
    },
    {
      id: 'terminal.close',
      label: 'Close Terminal Pane',
      category: 'terminal',
      description: 'Close the focused terminal pane',
      icon: X,
      keywords: ['kill', 'pane', 'remove'],
      handler: () => {},
    },
    {
      id: 'terminal.clear',
      label: 'Clear Terminal',
      category: 'terminal',
      description: 'Clear the output in the active terminal',
      icon: RotateCcw,
      keywords: ['reset', 'clean', 'scrollback'],
      handler: () => {},
    },
    {
      id: 'terminal.split',
      label: 'Split Terminal',
      category: 'terminal',
      description: 'Split the active terminal pane horizontally',
      icon: Columns,
      keywords: ['pane', 'split', 'horizontal'],
      handler: () => {
        const { activeSpaceId, spaces, addPaneToSpace } = useWorkspaceStore.getState()
        if (!activeSpaceId) return
        const id = `pane-${Date.now()}`
        addPaneToSpace(activeSpaceId, {
          id,
          agentType: 'shell',
        })
      },
    },
    {
      id: 'editor.toggle',
      label: 'Toggle Editor',
      category: 'panel',
      description: 'Show or hide the code editor panel',
      icon: FileText,
      shortcut: 'Mod+E',
      keywords: ['code', 'monaco', 'edit'],
      handler: () => useUIStore.getState().toggleEditor(),
    },
    {
      id: 'view.toggle-sidebar-section-spaces',
      label: 'Show Workspaces in Sidebar',
      category: 'navigation',
      description: 'Switch the sidebar to the workspaces list',
      icon: FolderOpen,
      keywords: ['spaces', 'projects', 'sidebar'],
      handler: () => {
        const store = useUIStore.getState()
        store.setSidebarSection('spaces')
        if (!store.sidebarOpen) store.toggleSidebar()
      },
    },
    {
      id: 'view.toggle-sidebar-section-agents',
      label: 'Show Agents in Sidebar',
      category: 'navigation',
      description: 'Switch the sidebar to the agents panel',
      icon: Eye,
      keywords: ['status', 'inspect', 'sidebar'],
      handler: () => {
        const store = useUIStore.getState()
        store.setSidebarSection('agents')
        if (!store.sidebarOpen) store.toggleSidebar()
      },
    },
    {
      id: 'view.toggle-sidebar-section-plugins',
      label: 'Show Plugins in Sidebar',
      category: 'navigation',
      description: 'Switch the sidebar to the plugins panel',
      icon: EyeOff,
      keywords: ['extensions', 'mcp', 'sidebar'],
      handler: () => {
        const store = useUIStore.getState()
        store.setSidebarSection('plugins')
        if (!store.sidebarOpen) store.toggleSidebar()
      },
    },
    {
      id: 'shortcuts.view',
      label: 'View Keyboard Shortcuts',
      category: 'settings',
      description: 'Open settings to the shortcuts tab',
      icon: Keyboard,
      keywords: ['keybindings', 'hotkeys', 'keymap'],
      handler: () => {
        useUIStore.getState().toggleSettings()
      },
    },
  ]

  useCommandStore.getState().registerCommands(commands)
}
