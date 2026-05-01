import { useState, useEffect, useRef } from 'react'
import { X, FolderOpen, ChevronRight, Terminal, Users, Plus, Minus } from 'lucide-react'
import { nanoid } from 'nanoid'
import { useWorkspaceStore, gridForPaneCount } from '../../store/workspaceStore'
import { useAthenaStore } from '../../store/athenaStore'
import { useUIStore } from '../../store/uiStore'
import { useSwarmStore } from '../../store/swarmStore'
import { themes } from '../../themes/themes'
import { GridTemplateSelector } from './GridTemplateSelector'
import type { AgentType, GridTemplate, PaneConfig, Space } from '../../types/workspace'
import type { AgentRole, SwarmState, SwarmAgent } from '../../types/swarm'
import { getAgentLabel, getAgentCommand, getAgentColor } from '../../utils/agentCommands'

const GRID_CELL_COUNT: Record<GridTemplate, number> = {
  '1x1': 1,
  '1x2': 2,
  '2x2': 4,
  '2x3': 6,
  '3x3': 9,
  '3x4': 12,
  '4x4': 16,
}
const TAB_COLORS = [
  '#0ea5e9',
  '#22c55e',
  '#f59e0b',
  '#ef4444',
  '#06b6d4',
  '#06b6d4',
  '#f97316',
  '#64748b',
]

const ROLE_OPTIONS: { role: AgentRole; label: string; color: string }[] = [
  { role: 'coordinator', label: 'Coordinator', color: '#0ea5e9' },
  { role: 'builder', label: 'Builder', color: '#22c55e' },
  { role: 'scout', label: 'Scout', color: '#f59e0b' },
  { role: 'reviewer', label: 'Reviewer', color: '#06b6d4' },
]

const AGENT_TYPES: AgentType[] = ['claude', 'codex', 'opencode', 'gemini', 'shell']

const ROLE_PROMPTS: Record<AgentRole, string> = {
  coordinator:
    'You are the COORDINATOR. Break the goal into tasks and assign them to builders. Write the plan to .ade/swarm-state.json. Monitor progress every 30 seconds. Send messages via the mailbox system.',
  builder:
    'You are a BUILDER agent. Read your assigned task from .ade/swarm-state.json. Check your mailbox for instructions. Only modify files listed in your ownedFiles. Update your status to "review" when done. Report "blocked" if stuck.',
  scout:
    'You are a SCOUT agent. Explore the codebase and write reports to .ade/scout-report.md. Answer builder questions. You are READ-ONLY — do not modify source files.',
  reviewer:
    'You are a REVIEWER agent. Monitor for tasks with status "review". Read the ownedFiles for each task. Approve (set status to "done") or reject (set status to "building" with feedback). Write verdicts to .ade/reviews/',
}

type Mode = 'terminal' | 'swarm'
interface AgentSlot {
  role: AgentRole
  agentType: AgentType
}
interface NewSpaceModalProps {
  onClose: () => void
}

export function NewSpaceModal({ onClose }: NewSpaceModalProps) {
  const [mode, setMode] = useState<Mode | null>(null)
  const [step, setStep] = useState(0)
  const [dir, setDir] = useState('')
  const [grid, setGrid] = useState<GridTemplate>('2x2')
  const [paneAgents, setPaneAgents] = useState<
    { agentType: AgentType; customCmd?: string; customAgentId?: string }[]
  >([])

  // Swarm
  const [goal, setGoal] = useState('')
  const [slots, setSlots] = useState<AgentSlot[]>([
    { role: 'coordinator', agentType: 'claude' },
    { role: 'builder', agentType: 'claude' },
    { role: 'builder', agentType: 'claude' },
  ])

  const spaces = useWorkspaceStore((s) => s.spaces)
  const addSpace = useWorkspaceStore((s) => s.addSpace)
  const { setActivePanel, theme } = useUIStore()
  const { setSwarm } = useSwarmStore()
  const customAgents = useAthenaStore((s) => s.customAgents)

  // 1-Row Native Terminal Hooks
  const termRef = useRef<HTMLDivElement>(null)
  const cliPtyId = useRef(`newspace-cli-${nanoid()}`)
  const [termInstance, setTermInstance] = useState<any>(null)
  const [isPtyReady, setIsPtyReady] = useState(false)

  // Prefer User's Default Env > Last Used
  useEffect(() => {
    Promise.all([
      window.athena.store.get('athena-defaultWorkspaceDir'),
      window.athena.store.get('lastUsedDir'),
    ]).then(([defaultDir, cachedDir]) => {
      if (defaultDir && typeof defaultDir === 'string' && defaultDir.trim() !== '') {
        setDir(defaultDir.trim())
      } else if (cachedDir && typeof cachedDir === 'string') {
        setDir(cachedDir)
      }
    })
  }, [])

  // Mount logic for ad-hoc xterm component when Step 1 loads
  useEffect(() => {
    if (step !== 1 || !termRef.current || termInstance) return

    let fit: any, term: any
    import('@xterm/xterm').then(({ Terminal }) => {
      import('@xterm/addon-fit').then(({ FitAddon }) => {
        // @ts-ignore — CSS side-effect import for xterm styling
        import('@xterm/xterm/css/xterm.css').then(() => {
          const themeColors = themes[theme]?.colors
          term = new Terminal({
            rows: 1,
            fontFamily: 'monospace',
            fontSize: 12,
            theme: themeColors
              ? {
                  background: 'transparent',
                  foreground: themeColors.text || themeColors.terminalFg,
                  cursor: themeColors.accent || themeColors.terminalCursor,
                  selectionBackground: themeColors.terminalSelection,
                }
              : { background: 'transparent' },
            cursorBlink: true,
          })
          fit = new FitAddon()
          term.loadAddon(fit)
          term.open(termRef.current!)
          fit.fit()
          setTermInstance(term)
        })
      })
    })

    return () => {
      if (term) term.dispose()
    }
  }, [step, termInstance])

  // Bridge session backend when the UI connects
  useEffect(() => {
    if (termInstance && dir && !isPtyReady) {
      setIsPtyReady(true)
      window.athena.pty.spawn(cliPtyId.current, dir, '/bin/zsh').then(() => {
        const cleanupData = window.athena.pty.onData(cliPtyId.current, (data: string) => {
          termInstance.write(data)
        })
        const dataHandler = termInstance.onData((data: string) => {
          window.athena.pty.write(cliPtyId.current, data)
        })

        termInstance.__cleanup = () => {
          cleanupData()
          dataHandler.dispose()
        }

        // Auto-focus terminal to allow immediate typing
        setTimeout(() => {
          if (termInstance) {
            termInstance.focus()
          }
        }, 100)
      })
    }
  }, [termInstance, dir, isPtyReady])

  // Kill ephemeral modal session when closed
  useEffect(() => {
    return () => {
      if (termInstance && typeof termInstance.__cleanup === 'function') {
        termInstance.__cleanup()
      }
      window.athena.pty.kill(cliPtyId.current)
    }
  }, [termInstance])

  // Synchronize browse payload with backend
  const handleBrowse = async () => {
    const selected = await window.athena.fs.showOpenDialog()
    if (selected) {
      setDir(selected)
      if (isPtyReady) {
        window.athena.pty.write(cliPtyId.current, `\x03cd "${selected}"\nclear\n`)
      }
    }
  }

  // Pre-flight directory extractor (reads current 'cd' from terminal process)
  const resolveFinalDir = async () => {
    if (isPtyReady) {
      const ptyCwd = await window.athena.pty.getCwd(cliPtyId.current)
      if (ptyCwd) return ptyCwd
    }
    return dir.trim()
  }

  const handleLaunchTerminal = async () => {
    const finalDir = await resolveFinalDir()
    if (!finalDir) return

    const panes: PaneConfig[] = paneAgents.map((pa) => ({
      id: nanoid(),
      agentType: pa.agentType,
      customCmd: pa.customCmd,
      customAgentId: pa.customAgentId,
    }))

    const space: Space = {
      id: nanoid(),
      name: `Space ${spaces.length + 1}`,
      dir: finalDir,
      grid,
      panes,
      color: TAB_COLORS[spaces.length % TAB_COLORS.length],
      createdAt: Date.now(),
      lastOpenedAt: Date.now(),
    }

    addSpace(space)
    window.athena.store.set('lastUsedDir', finalDir)
    onClose()
  }

  const handleLaunchSwarm = async () => {
    const finalDir = await resolveFinalDir()
    if (!finalDir || !goal.trim()) return
    if (coordinatorCount !== 1 || builderCount < 1) return

    const agents: SwarmAgent[] = slots.map((slot) => ({
      id: nanoid(),
      role: slot.role,
      agentType: slot.agentType,
      paneId: `swarm-${nanoid()}`,
      status: 'idle' as const,
      currentTask: null,
      lastAction: 'Spawned',
      lastActionAt: Date.now(),
    }))

    const panes: PaneConfig[] = agents.map((agent) => ({
      id: agent.paneId,
      agentType: agent.agentType,
    }))

    const space: Space = {
      id: nanoid(),
      name: `Mission ${spaces.length + 1}`,
      dir: finalDir,
      grid: '2x2',
      panes,
      color: TAB_COLORS[spaces.length % TAB_COLORS.length],
      createdAt: Date.now(),
      lastOpenedAt: Date.now(),
    }

    addSpace(space)

    const state: SwarmState = {
      id: nanoid(),
      goal: goal.trim(),
      agents,
      tasks: [],
      messages: [],
      status: 'active',
      startedAt: Date.now(),
    }

    await window.athena.swarm.writeState(finalDir, state)
    setSwarm(state)

    for (const agent of agents) {
      const agentCmd = getAgentCommand(agent.agentType)
      const shell = '/bin/zsh'
      await window.athena.pty.spawn(agent.paneId, finalDir, shell, agentCmd)

      const rolePrompt = ROLE_PROMPTS[agent.role]
      const fullPrompt = `${rolePrompt}\n\nGOAL: ${goal.trim()}\n\nStart working now.`

      setTimeout(() => {
        window.athena.pty.write(agent.paneId, fullPrompt + '\n')
      }, 1500)
    }

    setActivePanel('swarm')
    window.athena.store.set('lastUsedDir', finalDir)
    onClose()
  }

  // Helper arrays & selections
  const handleGridSelect = (g: GridTemplate) => setGrid(g)
  const handleModeSelect = (m: Mode) => {
    setMode(m)
    setStep(1)
  }
  const addSlot = () => {
    if (slots.length < 10) setSlots([...slots, { role: 'builder', agentType: 'claude' }])
  }
  const removeSlot = (idx: number) => {
    if (slots.length > 2) setSlots(slots.filter((_, i) => i !== idx))
  }
  const updateSlot = (idx: number, updates: Partial<AgentSlot>) =>
    setSlots(slots.map((s, i) => (i === idx ? { ...s, ...updates } : s)))
  const coordinatorCount = slots.filter((s) => s.role === 'coordinator').length
  const builderCount = slots.filter((s) => s.role === 'builder').length

  const addPaneAgent = (type: AgentType | string) => {
    if (paneAgents.length >= 16) return
    const storeAgent = customAgents.find((a) => a.id === type)
    setPaneAgents([
      ...paneAgents,
      storeAgent
        ? {
            agentType: 'custom' as AgentType,
            customCmd: storeAgent.command,
            customAgentId: storeAgent.id,
          }
        : { agentType: type as AgentType },
    ])
    setGrid(gridForPaneCount(paneAgents.length + 1))
  }

  const removePaneAgent = (type: AgentType | string) => {
    const storeAgent = customAgents.find((a) => a.id === type)
    const idx = [...paneAgents]
      .reverse()
      .findIndex((p) =>
        storeAgent
          ? p.agentType === 'custom' && p.customAgentId === storeAgent.id
          : p.agentType === type,
      )
    if (idx === -1) return
    const realIdx = paneAgents.length - 1 - idx
    const newAgents = paneAgents.filter((_, i) => i !== realIdx)
    setPaneAgents(newAgents)
    setGrid(gridForPaneCount(newAgents.length))
  }

  const canAdvanceStep1 = dir.trim() !== ''

  // Step 1: Using 'display' instead of conditional render so Xterm keeps its DOM node intact between wizard tabs.
  const stepOneRender = (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1.5">
        <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
          Working directory
        </label>
        <div className="flex gap-2">
          <input
            value={dir}
            onChange={(e) => setDir(e.target.value)}
            placeholder="/Users/you/projects/my-app"
            className="flex-1 px-3 py-2 rounded-lg text-sm outline-none transition-colors"
            style={{
              background: 'var(--bg)',
              border: '1px solid var(--border)',
              color: 'var(--text)',
            }}
            onFocus={(e) => (e.target.style.borderColor = 'var(--accent)')}
            onBlur={(e) => (e.target.style.borderColor = 'var(--border)')}
          />
          <button
            onClick={handleBrowse}
            className="px-3 py-2 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-colors"
            style={{
              background: 'var(--bgTertiary)',
              color: 'var(--textMuted)',
              border: '1px solid var(--border)',
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--border)')}
            onMouseLeave={(e) => (e.currentTarget.style.background = 'var(--bgTertiary)')}
          >
            <FolderOpen size={13} />
            Browse
          </button>
        </div>

        {/* Real 1-Row Terminal Component */}
        <div className="relative mt-1 flex flex-col gap-1">
          <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
            Navigate (Shell)
          </label>
          <div
            ref={termRef}
            onClick={() => termInstance?.focus()}
            className="w-full px-2 py-1.5 rounded-lg text-xs cursor-text"
            style={{
              background: 'var(--bgTertiary)',
              border: '1px solid var(--border)',
              minHeight: '34px',
              overflow: 'hidden',
            }}
          >
            {!termInstance && (
              <span style={{ color: 'var(--textMuted)' }}>Initializing shell...</span>
            )}
          </div>
        </div>
      </div>

      {mode === 'swarm' && (
        <div className="flex flex-col gap-1.5">
          <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
            Goal
          </label>
          <textarea
            value={goal}
            onChange={(e) => setGoal(e.target.value)}
            placeholder="Describe what the swarm should accomplish..."
            rows={3}
            className="px-3 py-2 rounded-lg text-sm outline-none resize-none"
            style={{
              background: 'var(--bg)',
              border: '1px solid var(--border)',
              color: 'var(--text)',
            }}
          />
        </div>
      )}
    </div>
  )

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: 'rgba(0,0,0,0.6)', backdropFilter: 'blur(4px)' }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className="rounded-xl shadow-2xl flex flex-col overflow-hidden"
        style={{
          width: 560,
          maxHeight: '85vh',
          background: 'var(--bgSecondary)',
          border: '1px solid var(--border)',
        }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between px-5 py-3.5 border-b"
          style={{ borderColor: 'var(--border)' }}
        >
          <div className="flex items-center gap-3">
            <h2 className="text-sm font-semibold" style={{ color: 'var(--text)' }}>
              {step === 0
                ? 'New Workspace'
                : mode === 'terminal'
                  ? 'Terminal Workspace'
                  : 'Swarm Mission'}
            </h2>
            {step > 0 && (
              <div className="flex items-center gap-1">
                {Array.from({ length: 2 }, (_, i) => (
                  <div
                    key={i}
                    className="w-1.5 h-1.5 rounded-full transition-colors"
                    style={{ background: i + 1 <= step ? 'var(--accent)' : 'var(--bgTertiary)' }}
                  />
                ))}
              </div>
            )}
          </div>
          <button onClick={onClose} className="p-1 rounded hover:bg-white/10 transition-colors">
            <X size={16} style={{ color: 'var(--textMuted)' }} />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-5">
          {/* Step 0: Mode Selection */}
          {step === 0 && (
            <div className="flex flex-col gap-3">
              <p className="text-xs mb-1" style={{ color: 'var(--textDim)' }}>
                Choose workspace type
              </p>
              <button
                onClick={() => handleModeSelect('terminal')}
                className="flex items-center gap-4 p-4 rounded-lg text-left transition-colors"
                style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}
                onMouseEnter={(e) => (e.currentTarget.style.borderColor = 'var(--accent)')}
                onMouseLeave={(e) => (e.currentTarget.style.borderColor = 'var(--border)')}
              >
                <div
                  className="w-10 h-10 rounded-lg flex items-center justify-center shrink-0"
                  style={{ background: 'var(--bgTertiary)' }}
                >
                  <Terminal size={20} style={{ color: 'var(--accent)' }} />
                </div>
                <div>
                  <span className="text-sm font-semibold block" style={{ color: 'var(--text)' }}>
                    Terminal Workspace
                  </span>
                  <span className="text-xs mt-0.5 block" style={{ color: 'var(--textDim)' }}>
                    Launch multiple terminal panes with AI agents
                  </span>
                </div>
              </button>
              <button
                onClick={() => handleModeSelect('swarm')}
                className="flex items-center gap-4 p-4 rounded-lg text-left transition-colors"
                style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}
                onMouseEnter={(e) => (e.currentTarget.style.borderColor = 'var(--accent)')}
                onMouseLeave={(e) => (e.currentTarget.style.borderColor = 'var(--border)')}
              >
                <div
                  className="w-10 h-10 rounded-lg flex items-center justify-center shrink-0"
                  style={{ background: 'var(--bgTertiary)' }}
                >
                  <Users size={20} style={{ color: 'var(--accent)' }} />
                </div>
                <div>
                  <span className="text-sm font-semibold block" style={{ color: 'var(--text)' }}>
                    Swarm Mission
                  </span>
                  <span className="text-xs mt-0.5 block" style={{ color: 'var(--textDim)' }}>
                    Orchestrate a team of agents on a shared goal
                  </span>
                </div>
              </button>
            </div>
          )}

          {/* Combined Step 1 UI via invisible toggle (preventing component teardown of Terminal container) */}
          <div style={{ display: step === 1 ? 'block' : 'none' }}>{stepOneRender}</div>

          {/* Terminal Flow — Step 2: Grid & Agents */}
          {mode === 'terminal' && step === 2 && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-2">
                <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
                  Preset Layout
                </label>
                <GridTemplateSelector selected={grid} onSelect={handleGridSelect} />
              </div>

              <div
                className="flex flex-col gap-2 mt-2 border-t pt-4"
                style={{ borderColor: 'var(--border)' }}
              >
                <div className="flex items-center justify-between mb-2">
                  <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
                    Agents ({paneAgents.length}/16)
                  </label>
                </div>
                {[...AGENT_TYPES, ...customAgents.map((a) => a.id as unknown as AgentType)].map(
                  (type) => {
                    const isCustomStoreAgent = customAgents.some((a) => a.id === (type as string))
                    const storeAgent = customAgents.find((a) => a.id === (type as string))
                    const count = paneAgents.filter(
                      (p) =>
                        p.agentType === type ||
                        (isCustomStoreAgent &&
                          p.agentType === 'custom' &&
                          p.customAgentId === storeAgent?.id),
                    ).length
                    const displayLabel = isCustomStoreAgent
                      ? storeAgent?.name
                      : getAgentLabel(type as AgentType)
                    const displayColor = isCustomStoreAgent
                      ? '#6b7280'
                      : getAgentColor(type as AgentType)

                    return (
                      <div
                        key={type}
                        className="flex items-center justify-between p-2 rounded-md transition-colors"
                        style={{
                          background: count > 0 ? 'var(--bgTertiary)' : 'var(--bg)',
                          border: '1px solid var(--border)',
                        }}
                      >
                        <div className="flex items-center gap-3">
                          <div
                            className="w-2 h-2 rounded-full shrink-0"
                            style={{ background: count > 0 ? displayColor : 'var(--textDim)' }}
                          />
                          <span
                            className="text-[12px] font-medium"
                            style={{ color: count > 0 ? 'var(--text)' : 'var(--textMuted)' }}
                          >
                            {displayLabel}
                          </span>
                        </div>

                        <div className="flex items-center gap-3">
                          <button
                            onClick={() => removePaneAgent(type)}
                            disabled={count === 0}
                            className="w-5 h-5 flex items-center justify-center rounded-full hover:bg-white/10 transition-colors disabled:opacity-30"
                            style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}
                          >
                            <Minus size={10} style={{ color: 'var(--text)' }} />
                          </button>

                          <span
                            className="text-[11.5px] font-mono text-center w-3"
                            style={{ color: 'var(--text)' }}
                          >
                            {count}
                          </span>

                          <button
                            onClick={() => addPaneAgent(type)}
                            disabled={paneAgents.length >= 16}
                            className="w-5 h-5 flex items-center justify-center rounded-full hover:bg-white/10 transition-colors disabled:opacity-30"
                            style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}
                          >
                            <Plus size={10} style={{ color: 'var(--text)' }} />
                          </button>
                        </div>
                      </div>
                    )
                  },
                )}
              </div>
            </div>
          )}

          {/* Swarm Flow — Step 2: Team Config */}
          {mode === 'swarm' && step === 2 && (
            <div className="flex flex-col gap-3">
              <div className="flex items-center justify-between">
                <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
                  Team ({slots.length} agents)
                </label>
                <button
                  onClick={addSlot}
                  disabled={slots.length >= 10}
                  className="flex items-center gap-1 px-2 py-1 rounded text-[11px] transition-colors disabled:opacity-30"
                  style={{ background: 'var(--bgTertiary)', color: 'var(--textMuted)' }}
                >
                  <Plus size={11} /> Add
                </button>
              </div>

              {slots.map((slot, idx) => (
                <div
                  key={idx}
                  className="flex items-center gap-2 p-2 rounded-md"
                  style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}
                >
                  <div
                    className="w-2 h-2 rounded-full shrink-0"
                    style={{ background: ROLE_OPTIONS.find((r) => r.role === slot.role)?.color }}
                  />
                  <select
                    value={slot.role}
                    onChange={(e) => updateSlot(idx, { role: e.target.value as AgentRole })}
                    className="px-1.5 py-0.5 rounded text-[11px] outline-none"
                    style={{
                      background: 'var(--bgTertiary)',
                      color: 'var(--text)',
                      border: '1px solid var(--border)',
                    }}
                  >
                    {ROLE_OPTIONS.map((r) => (
                      <option key={r.role} value={r.role}>
                        {r.label}
                      </option>
                    ))}
                  </select>
                  <select
                    value={slot.agentType}
                    onChange={(e) => updateSlot(idx, { agentType: e.target.value as AgentType })}
                    className="px-1.5 py-0.5 rounded text-[11px] outline-none"
                    style={{
                      background: 'var(--bgTertiary)',
                      color: 'var(--text)',
                      border: '1px solid var(--border)',
                    }}
                  >
                    {AGENT_TYPES.map((t) => (
                      <option key={t} value={t}>
                        {getAgentLabel(t)}
                      </option>
                    ))}
                  </select>
                  <div className="flex-1" />
                  <button
                    onClick={() => removeSlot(idx)}
                    disabled={slots.length <= 2}
                    className="p-1 rounded hover:bg-white/10 transition-colors disabled:opacity-20"
                  >
                    <Minus size={12} style={{ color: 'var(--textDim)' }} />
                  </button>
                </div>
              ))}

              {coordinatorCount !== 1 && (
                <p className="text-[10px]" style={{ color: 'var(--error)' }}>
                  Exactly 1 Coordinator required
                </p>
              )}
              {builderCount < 1 && (
                <p className="text-[10px]" style={{ color: 'var(--error)' }}>
                  At least 1 Builder required
                </p>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        {step > 0 && (
          <div
            className="flex items-center justify-between px-5 py-3 border-t"
            style={{ borderColor: 'var(--border)' }}
          >
            <div className="text-[11px]" style={{ color: 'var(--textDim)' }}>
              Step {step} of 2
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => setStep(step === 1 ? 0 : step - 1)}
                className="px-3 py-1.5 rounded-md text-xs font-medium transition-colors"
                style={{ color: 'var(--textMuted)', background: 'var(--bgTertiary)' }}
              >
                Back
              </button>

              {step === 1 && (
                <button
                  onClick={() => setStep(2)}
                  disabled={!canAdvanceStep1 || (mode === 'swarm' && !goal.trim())}
                  className="px-4 py-1.5 rounded-md text-xs font-medium flex items-center gap-1 transition-colors disabled:opacity-40"
                  style={{ background: 'var(--accent)', color: '#fff' }}
                >
                  Next <ChevronRight size={13} />
                </button>
              )}

              {mode === 'terminal' && step === 2 && (
                <button
                  onClick={handleLaunchTerminal}
                  className="px-4 py-1.5 rounded-md text-xs font-semibold transition-colors"
                  style={{ background: 'var(--accent)', color: '#fff' }}
                >
                  Launch Space
                </button>
              )}

              {mode === 'swarm' && step === 2 && (
                <button
                  onClick={handleLaunchSwarm}
                  disabled={coordinatorCount !== 1 || builderCount < 1}
                  className="px-4 py-1.5 rounded-md text-xs font-semibold transition-colors disabled:opacity-40"
                  style={{ background: 'var(--accent)', color: '#fff' }}
                >
                  Launch Swarm
                </button>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
