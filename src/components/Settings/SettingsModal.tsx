import { useState, useEffect } from 'react'
import {
  X,
  FolderOpen,
  Monitor,
  Bot,
  Palette,
  Keyboard,
  Info,
  Brain,
  Trash2,
  Plus,
} from 'lucide-react'
import { useUIStore } from '../../store/uiStore'
import { useAthenaStore } from '../../store/athenaStore'
import { useNotificationStore } from '../../store/notificationStore'
import { themes, applyTheme } from '../../themes/themes'
import type { ThemeName } from '../../types/theme'
import { nanoid } from 'nanoid'

interface SettingsModalProps {
  onClose: () => void
}

const TABS = [
  { id: 'general', label: 'General', icon: Monitor },
  { id: 'athena', label: 'Athena', icon: Brain },
  { id: 'agents', label: 'Agents', icon: Bot },
  { id: 'themes', label: 'Themes', icon: Palette },
  { id: 'shortcuts', label: 'Shortcuts', icon: Keyboard },
  { id: 'about', label: 'About', icon: Info },
] as const

type TabId = (typeof TABS)[number]['id']

const FONTS = [
  "'JetBrains Mono', monospace",
  "'Fira Code', monospace",
  "'Cascadia Code', monospace",
  "'Menlo', monospace",
  "'Consolas', monospace",
]

const SHORTCUTS = [
  ['Cmd/Ctrl+T', 'New workspace tab'],
  ['Cmd/Ctrl+W', 'Close current tab'],
  ['Cmd/Ctrl+P', 'Quick Open'],
  ['Cmd/Ctrl+J', 'Toggle Athena panel'],
  ['Cmd/Ctrl+B', 'Toggle browser'],
  ['Cmd/Ctrl+E', 'Toggle editor'],
  ['Cmd/Ctrl+K', 'Toggle Kanban'],
  ['Cmd/Ctrl+,', 'Settings'],
  ['Cmd/Ctrl+\\', 'Toggle sidebar'],
  ['Cmd/Ctrl+Shift+S', 'Launch Swarm'],
  ['Escape', 'Close modal/overlay'],
]

export function SettingsModal({ onClose }: SettingsModalProps) {
  const [tab, setTab] = useState<TabId>('general')
  const { theme, setTheme, fontSize, setFontSize, fontFamily, setFontFamily } = useUIStore()
  const {
    model,
    setModel,
    bypassMode,
    setBypassMode,
    autoLaunch,
    setAutoLaunch,
    customAgents,
    addCustomAgent,
    removeCustomAgent,
    provider,
    setProvider,
  } = useAthenaStore()
  const { muted, setMuted } = useNotificationStore()

  const [newAgentName, setNewAgentName] = useState('')
  const [newAgentCmd, setNewAgentCmd] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [defaultWorkspaceDir, setDefaultWorkspaceDir] = useState('')

  useEffect(() => {
    window.athena.store.get('athena.apiKey').then((val: any) => {
      setApiKey(val || '')
    })
    window.athena.store.get('athena-defaultWorkspaceDir').then((val: any) => {
      if (val) setDefaultWorkspaceDir(val)
    })
    window.athena.store.get('athena.provider').then((val: any) => {
      if (val) setProvider(val)
    })
    window.athena.store.get('athena-model').then((val: any) => {
      if (val) setModel(val)
    })
  }, [])

  const handleApiKeyChange = (val: string) => {
    setApiKey(val)
    window.athena.store.set('athena.apiKey', val)
  }

  const handleProviderChange = (val: string) => {
    setProvider(val)
    window.athena.store.set('athena.provider', val)
  }

  const handleThemeChange = (name: ThemeName) => {
    setTheme(name)
    applyTheme(themes[name])
    window.athena.store.set('theme', name)
  }

  const handleModelChange = (val: string) => {
    setModel(val)
    window.athena.store.set('athena-model', val)
  }

  const handleBypassChange = (val: boolean) => {
    setBypassMode(val)
    window.athena.store.set('athena-bypassMode', val)
  }

  const handleAutoLaunchChange = (val: boolean) => {
    setAutoLaunch(val)
    window.athena.store.set('athena-autoLaunch', val)
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: 'rgba(0,0,0,0.6)', backdropFilter: 'blur(4px)' }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className="rounded-xl shadow-2xl flex overflow-hidden"
        style={{
          width: 700,
          height: 500,
          background: 'var(--bgSecondary)',
          border: '1px solid var(--border)',
        }}
      >
        {/* Sidebar */}
        <div
          className="w-[180px] shrink-0 border-r flex flex-col py-2"
          style={{ borderColor: 'var(--border)', background: 'var(--bg)' }}
        >
          <div className="px-4 py-2 mb-1">
            <span className="text-xs font-semibold" style={{ color: 'var(--textMuted)' }}>
              Settings
            </span>
          </div>
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className="flex items-center gap-2 px-4 py-1.5 mx-1 rounded-md text-xs transition-colors"
              style={{
                background: tab === t.id ? 'var(--bgTertiary)' : 'transparent',
                color: tab === t.id ? 'var(--text)' : 'var(--textMuted)',
              }}
            >
              <t.icon size={14} />
              {t.label}
            </button>
          ))}
        </div>

        {/* Content */}
        <div className="flex-1 flex flex-col min-w-0">
          <div
            className="flex items-center justify-between px-5 py-3 border-b"
            style={{ borderColor: 'var(--border)' }}
          >
            <h3 className="text-sm font-semibold" style={{ color: 'var(--text)' }}>
              {TABS.find((t) => t.id === tab)?.label}
            </h3>
            <button onClick={onClose} className="p-1 rounded hover:bg-white/10 transition-colors">
              <X size={16} style={{ color: 'var(--textMuted)' }} />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto p-5">
            {tab === 'general' && (
              <div className="flex flex-col gap-5">
                <SettingRow label="Default Workspace">
                  <div className="flex items-center gap-2">
                    <input
                      value={defaultWorkspaceDir}
                      onChange={(e) => {
                        setDefaultWorkspaceDir(e.target.value)
                        window.athena.store.set('athena-defaultWorkspaceDir', e.target.value)
                      }}
                      placeholder="/Users/my/projects"
                      className="px-2 py-1 rounded w-48 text-xs outline-none bg-transparent"
                      style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
                    />
                    <button
                      onClick={async () => {
                        const selected = await window.athena.fs.showOpenDialog()
                        if (selected) {
                          setDefaultWorkspaceDir(selected)
                          window.athena.store.set('athena-defaultWorkspaceDir', selected)
                        }
                      }}
                      className="px-2 py-1 rounded text-[11px] transition-colors flex items-center justify-center"
                      style={{
                        background: 'var(--bgTertiary)',
                        border: '1px solid var(--border)',
                        color: 'var(--text)',
                      }}
                    >
                      <FolderOpen size={12} className="mr-1" /> Browse
                    </button>
                  </div>
                </SettingRow>
                <SettingRow label="Font family">
                  <select
                    value={fontFamily}
                    onChange={(e) => setFontFamily(e.target.value)}
                    className="px-2 py-1 rounded text-xs outline-none"
                    style={{
                      background: 'var(--bg)',
                      color: 'var(--text)',
                      border: '1px solid var(--border)',
                    }}
                  >
                    {FONTS.map((f) => (
                      <option key={f} value={f}>
                        {f.split("'")[1]}
                      </option>
                    ))}
                  </select>
                </SettingRow>
                <SettingRow label="Font size">
                  <div className="flex items-center gap-2">
                    <input
                      type="range"
                      min={10}
                      max={24}
                      value={fontSize}
                      onChange={(e) => setFontSize(Number(e.target.value))}
                      className="w-32"
                    />
                    <span className="text-xs w-6 text-right" style={{ color: 'var(--text)' }}>
                      {fontSize}
                    </span>
                  </div>
                </SettingRow>
                <SettingRow label="Mute notifications">
                  <button
                    onClick={() => setMuted(!muted)}
                    className="w-8 h-4 rounded-full transition-colors relative"
                    style={{ background: muted ? 'var(--accent)' : 'var(--bgTertiary)' }}
                  >
                    <div
                      className="w-3 h-3 rounded-full absolute top-0.5 transition-all"
                      style={{
                        background: '#fff',
                        left: muted ? 16 : 2,
                      }}
                    />
                  </button>
                </SettingRow>
              </div>
            )}

            {tab === 'agents' && (
              <div className="flex flex-col gap-4">
                <div className="flex flex-col gap-2">
                  {customAgents.map((ag) => (
                    <div
                      key={ag.id}
                      className="flex items-center justify-between p-2 rounded border"
                      style={{ borderColor: 'var(--border)', background: 'var(--bg)' }}
                    >
                      <div className="flex flex-col">
                        <span className="text-sm" style={{ color: 'var(--text)' }}>
                          {ag.name}
                        </span>
                        <span className="text-xs font-mono" style={{ color: 'var(--textMuted)' }}>
                          {ag.command}
                        </span>
                      </div>
                      <button
                        onClick={() => {
                          removeCustomAgent(ag.id)
                          window.athena.store.set(
                            'athena-customAgents',
                            useAthenaStore.getState().customAgents.filter((a) => a.id !== ag.id),
                          )
                        }}
                        className="p-1 hover:text-red-500 transition-colors"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  ))}
                  {customAgents.length === 0 && (
                    <div className="text-xs italic" style={{ color: 'var(--textMuted)' }}>
                      No custom agents configured.
                    </div>
                  )}
                </div>
                <div
                  className="flex items-end gap-2 p-3 rounded bg-black/20"
                  style={{ border: '1px solid var(--border)' }}
                >
                  <div className="flex-1 flex flex-col gap-1">
                    <span className="text-xs" style={{ color: 'var(--textMuted)' }}>
                      Agent Name
                    </span>
                    <input
                      value={newAgentName}
                      onChange={(e) => setNewAgentName(e.target.value)}
                      placeholder="My Super Agent"
                      className="px-2 py-1 flex-1 rounded text-xs outline-none bg-transparent"
                      style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
                    />
                  </div>
                  <div className="flex-1 flex flex-col gap-1">
                    <span className="text-xs" style={{ color: 'var(--textMuted)' }}>
                      CLI Command
                    </span>
                    <input
                      value={newAgentCmd}
                      onChange={(e) => setNewAgentCmd(e.target.value)}
                      placeholder="my-agent-cli --flag"
                      className="px-2 py-1 flex-1 rounded text-xs outline-none bg-transparent"
                      style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
                    />
                  </div>
                  <button
                    onClick={() => {
                      if (!newAgentName || !newAgentCmd) return
                      const newAgent = { id: nanoid(), name: newAgentName, command: newAgentCmd }
                      addCustomAgent(newAgent)
                      window.athena.store.set('athena-customAgents', [
                        ...useAthenaStore.getState().customAgents,
                        newAgent,
                      ])
                      setNewAgentName('')
                      setNewAgentCmd('')
                    }}
                    className="px-3 py-1 flex items-center justify-center rounded text-xs transition-colors h-6"
                    style={{ background: 'var(--accent)', color: '#fff' }}
                  >
                    <Plus size={14} className="mr-1" /> Add
                  </button>
                </div>
              </div>
            )}

            {tab === 'athena' && (
              <div className="flex flex-col gap-5">
                <SettingRow label="Provider">
                  <select
                    value={provider}
                    onChange={(e) => handleProviderChange(e.target.value)}
                    className="px-2 py-1 rounded text-xs outline-none"
                    style={{
                      background: 'var(--bg)',
                      color: 'var(--text)',
                      border: '1px solid var(--border)',
                    }}
                  >
                    <option value="anthropic">Anthropic</option>
                    <option value="nvidia_nim">NVIDIA NIM</option>
                  </select>
                </SettingRow>
                <SettingRow label="API Key">
                  <input
                    type="password"
                    value={apiKey}
                    onChange={(e) => handleApiKeyChange(e.target.value)}
                    placeholder={`Enter ${provider === 'anthropic' ? 'Anthropic' : 'NVIDIA NIM'} API Key`}
                    className="px-2 py-1 rounded w-64 text-xs outline-none bg-transparent"
                    style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
                  />
                </SettingRow>
                <SettingRow label="Model">
                  <input
                    type="text"
                    value={model}
                    onChange={(e) => handleModelChange(e.target.value)}
                    placeholder={
                      provider === 'anthropic'
                        ? 'claude-3-5-sonnet-20241022'
                        : 'minimax/minimax-text-01'
                    }
                    className="px-2 py-1 rounded w-64 text-xs outline-none bg-transparent"
                    style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
                  />
                </SettingRow>
              </div>
            )}

            {tab === 'themes' && (
              <div className="grid grid-cols-5 gap-2">
                {Object.values(themes).map((t) => (
                  <button
                    key={t.name}
                    onClick={() => handleThemeChange(t.name)}
                    className="flex flex-col items-center gap-1.5 p-2 rounded-lg transition-all"
                    style={{
                      border:
                        theme === t.name
                          ? `2px solid ${t.colors.accent}`
                          : '2px solid var(--border)',
                      background: t.colors.bg,
                    }}
                  >
                    <div
                      className="w-full h-8 rounded-md flex items-end gap-0.5 p-1"
                      style={{ background: t.colors.bgSecondary }}
                    >
                      <div className="w-2 h-3 rounded-sm" style={{ background: t.colors.accent }} />
                      <div
                        className="w-2 h-2 rounded-sm"
                        style={{ background: t.colors.success }}
                      />
                      <div
                        className="w-2 h-4 rounded-sm"
                        style={{ background: t.colors.bgTertiary }}
                      />
                    </div>
                    <span className="text-[10px] font-medium" style={{ color: t.colors.text }}>
                      {t.label}
                    </span>
                  </button>
                ))}
              </div>
            )}

            {tab === 'shortcuts' && (
              <div className="flex flex-col gap-1">
                {SHORTCUTS.map(([key, action]) => (
                  <div
                    key={key}
                    className="flex items-center justify-between py-1.5 px-2 rounded"
                    style={{ background: 'var(--bg)' }}
                  >
                    <span className="text-xs" style={{ color: 'var(--text)' }}>
                      {action}
                    </span>
                    <kbd
                      className="text-[10px] px-1.5 py-0.5 rounded font-mono"
                      style={{
                        background: 'var(--bgTertiary)',
                        color: 'var(--textMuted)',
                        border: '1px solid var(--border)',
                      }}
                    >
                      {key}
                    </kbd>
                  </div>
                ))}
              </div>
            )}

            {tab === 'about' && (
              <div className="flex flex-col gap-3">
                <p className="text-sm font-semibold" style={{ color: 'var(--text)' }}>
                  Athena's Core
                </p>
                <p className="text-xs" style={{ color: 'var(--textMuted)' }}>
                  Version 1.0.0
                </p>
                <p className="text-xs" style={{ color: 'var(--textDim)' }}>
                  Agent Development Environment
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

function SettingRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
        {label}
      </span>
      {children}
    </div>
  )
}
