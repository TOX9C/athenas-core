import { useState } from 'react'
import { X, FolderOpen, ChevronRight, Terminal, Users, Plus, Minus } from 'lucide-react'
import { nanoid } from 'nanoid'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { useUIStore } from '../../store/uiStore'
import { useSwarmStore } from '../../store/swarmStore'
import { GridTemplateSelector } from './GridTemplateSelector'
import { AgentPicker } from './AgentPicker'
import type { AgentType, GridTemplate, PaneConfig, Space } from '../../types/workspace'
import type { AgentRole, SwarmState, SwarmAgent } from '../../types/swarm'
import { getAgentLabel, getAgentCommand } from '../../utils/agentCommands'

const GRID_CELL_COUNT: Record<GridTemplate, number> = {
  '1x1': 1, '1x2': 2, '2x2': 4, '2x3': 6, '3x3': 9, '3x4': 12, '4x4': 16,
}

const TAB_COLORS = ['#6366f1', '#22c55e', '#f59e0b', '#ef4444', '#06b6d4', '#a855f7', '#f97316', '#64748b']

const ROLE_OPTIONS: { role: AgentRole; label: string; color: string }[] = [
  { role: 'coordinator', label: 'Coordinator', color: '#6366f1' },
  { role: 'builder', label: 'Builder', color: '#22c55e' },
  { role: 'scout', label: 'Scout', color: '#f59e0b' },
  { role: 'reviewer', label: 'Reviewer', color: '#a855f7' },
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
  const [paneAgents, setPaneAgents] = useState<{ agentType: AgentType; customCmd?: string }[]>([])

  // Swarm-specific state
  const [goal, setGoal] = useState('')
  const [slots, setSlots] = useState<AgentSlot[]>([
    { role: 'coordinator', agentType: 'claude' },
    { role: 'builder', agentType: 'claude' },
    { role: 'builder', agentType: 'claude' },
  ])

  const spaces = useWorkspaceStore((s) => s.spaces)
  const addSpace = useWorkspaceStore((s) => s.addSpace)
  const { setActivePanel } = useUIStore()
  const { setSwarm } = useSwarmStore()

  const handleBrowse = async () => {
    const selected = await window.athena.fs.showOpenDialog()
    if (selected) setDir(selected)
  }

  const handleGridSelect = (g: GridTemplate) => {
    setGrid(g)
    const count = GRID_CELL_COUNT[g]
    setPaneAgents(Array.from({ length: count }, () => ({ agentType: 'shell' as AgentType })))
  }

  const handleModeSelect = (m: Mode) => {
    setMode(m)
    setStep(1)
  }

  const addSlot = () => {
    if (slots.length >= 10) return
    setSlots([...slots, { role: 'builder', agentType: 'claude' }])
  }

  const removeSlot = (idx: number) => {
    if (slots.length <= 2) return
    setSlots(slots.filter((_, i) => i !== idx))
  }

  const updateSlot = (idx: number, updates: Partial<AgentSlot>) => {
    setSlots(slots.map((s, i) => (i === idx ? { ...s, ...updates } : s)))
  }

  const coordinatorCount = slots.filter((s) => s.role === 'coordinator').length
  const builderCount = slots.filter((s) => s.role === 'builder').length

  const totalSteps = mode === 'terminal' ? 3 : 2

  const handleLaunchTerminal = () => {
    if (!dir.trim()) return

    const panes: PaneConfig[] = paneAgents.map((pa) => ({
      id: nanoid(),
      agentType: pa.agentType,
      customCmd: pa.customCmd,
    }))

    const space: Space = {
      id: nanoid(),
      name: `Space ${spaces.length + 1}`,
      dir: dir.trim(),
      grid,
      panes,
      color: TAB_COLORS[spaces.length % TAB_COLORS.length],
      createdAt: Date.now(),
      lastOpenedAt: Date.now(),
    }

    addSpace(space)
    onClose()
  }

  const handleLaunchSwarm = async () => {
    if (!dir.trim() || !goal.trim()) return
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
      dir: dir.trim(),
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

    await window.athena.swarm.writeState(dir, state)
    setSwarm(state)

    for (const agent of agents) {
      const agentCmd = getAgentCommand(agent.agentType)
      const shell = '/bin/zsh'
      await window.athena.pty.spawn(agent.paneId, dir, shell, agentCmd)

      const rolePrompt = ROLE_PROMPTS[agent.role]
      const fullPrompt = `${rolePrompt}\n\nGOAL: ${goal.trim()}\n\nStart working now.`

      setTimeout(() => {
        window.athena.pty.write(agent.paneId, fullPrompt + '\n')
      }, 1500)
    }

    setActivePanel('swarm')
    onClose()
  }

  const canAdvanceStep1 = dir.trim() !== ''

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: 'rgba(0,0,0,0.6)', backdropFilter: 'blur(4px)' }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose() }}
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
        <div className="flex items-center justify-between px-5 py-3.5 border-b" style={{ borderColor: 'var(--border)' }}>
          <div className="flex items-center gap-3">
            <h2 className="text-sm font-semibold" style={{ color: 'var(--text)' }}>
              {step === 0 ? 'New Workspace' : mode === 'terminal' ? 'Terminal Workspace' : 'Swarm Mission'}
            </h2>
            {step > 0 && (
              <div className="flex items-center gap-1">
                {Array.from({ length: totalSteps }, (_, i) => (
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
              <p className="text-xs mb-1" style={{ color: 'var(--textDim)' }}>Choose workspace type</p>
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

          {/* Terminal Flow — Step 1: Name + Dir */}
          {mode === 'terminal' && step === 1 && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-1.5">
                <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>Space name</label>
                <input
                  autoFocus
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="My Project"
                  className="px-3 py-2 rounded-lg text-sm outline-none transition-colors"
                  style={{ background: 'var(--bg)', border: '1px solid var(--border)', color: 'var(--text)' }}
                  onFocus={(e) => (e.target.style.borderColor = 'var(--accent)')}
                  onBlur={(e) => (e.target.style.borderColor = 'var(--border)')}
                  onKeyDown={(e) => { if (e.key === 'Enter' && canAdvanceStep1) setStep(2) }}
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>Working directory</label>
                <div className="flex gap-2">
                  <input
                    value={dir}
                    onChange={(e) => setDir(e.target.value)}
                    placeholder="/Users/you/projects/my-app"
                    className="flex-1 px-3 py-2 rounded-lg text-sm outline-none transition-colors"
                    style={{ background: 'var(--bg)', border: '1px solid var(--border)', color: 'var(--text)' }}
                    onFocus={(e) => (e.target.style.borderColor = 'var(--accent)')}
                    onBlur={(e) => (e.target.style.borderColor = 'var(--border)')}
                  />
                  <button
                    onClick={handleBrowse}
                    className="px-3 py-2 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-colors"
                    style={{ background: 'var(--bgTertiary)', color: 'var(--textMuted)', border: '1px solid var(--border)' }}
                    onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--border)')}
                    onMouseLeave={(e) => (e.currentTarget.style.background = 'var(--bgTertiary)')}
                  >
                    <FolderOpen size={13} />
                    Browse
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Terminal Flow — Step 2: Grid */}
          {mode === 'terminal' && step === 2 && (
            <GridTemplateSelector selected={grid} onSelect={handleGridSelect} />
          )}

          {/* Terminal Flow — Step 3: Agents */}
          {mode === 'terminal' && step === 3 && (
            <AgentPicker grid={grid} paneAgents={paneAgents} onChange={setPaneAgents} />
          )}

          {/* Swarm Flow — Step 1: Name + Dir + Goal */}
          {mode === 'swarm' && step === 1 && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-1.5">
                <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>Mission name</label>
                <input
                  autoFocus
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Auth Refactor Sprint"
                  className="px-3 py-2 rounded-lg text-sm outline-none transition-colors"
                  style={{ background: 'var(--bg)', border: '1px solid var(--border)', color: 'var(--text)' }}
                  onFocus={(e) => (e.target.style.borderColor = 'var(--accent)')}
                  onBlur={(e) => (e.target.style.borderColor = 'var(--border)')}
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>Working directory</label>
                <div className="flex gap-2">
                  <input
                    value={dir}
                    onChange={(e) => setDir(e.target.value)}
                    placeholder="/Users/you/projects/my-app"
                    className="flex-1 px-3 py-2 rounded-lg text-sm outline-none transition-colors"
                    style={{ background: 'var(--bg)', border: '1px solid var(--border)', color: 'var(--text)' }}
                    onFocus={(e) => (e.target.style.borderColor = 'var(--accent)')}
                    onBlur={(e) => (e.target.style.borderColor = 'var(--border)')}
                  />
                  <button
                    onClick={handleBrowse}
                    className="px-3 py-2 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-colors"
                    style={{ background: 'var(--bgTertiary)', color: 'var(--textMuted)', border: '1px solid var(--border)' }}
                    onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--border)')}
                    onMouseLeave={(e) => (e.currentTarget.style.background = 'var(--bgTertiary)')}
                  >
                    <FolderOpen size={13} />
                    Browse
                  </button>
                </div>
              </div>
              <div className="flex flex-col gap-1.5">
                <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>Goal</label>
                <textarea
                  value={goal}
                  onChange={(e) => setGoal(e.target.value)}
                  placeholder="Describe what the swarm should accomplish..."
                  rows={3}
                  className="px-3 py-2 rounded-lg text-sm outline-none resize-none"
                  style={{ background: 'var(--bg)', border: '1px solid var(--border)', color: 'var(--text)' }}
                />
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
                    style={{ background: 'var(--bgTertiary)', color: 'var(--text)', border: '1px solid var(--border)' }}
                  >
                    {ROLE_OPTIONS.map((r) => (
                      <option key={r.role} value={r.role}>{r.label}</option>
                    ))}
                  </select>
                  <select
                    value={slot.agentType}
                    onChange={(e) => updateSlot(idx, { agentType: e.target.value as AgentType })}
                    className="px-1.5 py-0.5 rounded text-[11px] outline-none"
                    style={{ background: 'var(--bgTertiary)', color: 'var(--text)', border: '1px solid var(--border)' }}
                  >
                    {AGENT_TYPES.map((t) => (
                      <option key={t} value={t}>{getAgentLabel(t)}</option>
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
                <p className="text-[10px]" style={{ color: 'var(--error)' }}>Exactly 1 Coordinator required</p>
              )}
              {builderCount < 1 && (
                <p className="text-[10px]" style={{ color: 'var(--error)' }}>At least 1 Builder required</p>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        {step > 0 && (
          <div className="flex items-center justify-between px-5 py-3 border-t" style={{ borderColor: 'var(--border)' }}>
            <div className="text-[11px]" style={{ color: 'var(--textDim)' }}>
              Step {step} of {totalSteps}
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => setStep(step === 1 ? 0 : step - 1)}
                className="px-3 py-1.5 rounded-md text-xs font-medium transition-colors"
                style={{ color: 'var(--textMuted)', background: 'var(--bgTertiary)' }}
              >
                Back
              </button>

              {mode === 'terminal' && step < 3 && (
                <button
                  onClick={() => {
                    if (step === 1 && !canAdvanceStep1) return
                    if (step === 1 && paneAgents.length === 0) handleGridSelect(grid)
                    setStep(step + 1)
                  }}
                  disabled={step === 1 && !canAdvanceStep1}
                  className="px-4 py-1.5 rounded-md text-xs font-medium flex items-center gap-1 transition-colors disabled:opacity-40"
                  style={{ background: 'var(--accent)', color: '#fff' }}
                >
                  Next
                  <ChevronRight size={13} />
                </button>
              )}

              {mode === 'terminal' && step === 3 && (
                <button
                  onClick={handleLaunchTerminal}
                  className="px-4 py-1.5 rounded-md text-xs font-semibold transition-colors"
                  style={{ background: 'var(--accent)', color: '#fff' }}
                  onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--accentHover)')}
                  onMouseLeave={(e) => (e.currentTarget.style.background = 'var(--accent)')}
                >
                  Launch Space
                </button>
              )}

              {mode === 'swarm' && step === 1 && (
                <button
                  onClick={() => {
                    if (!canAdvanceStep1 || !goal.trim()) return
                    setStep(2)
                  }}
                  disabled={!canAdvanceStep1 || !goal.trim()}
                  className="px-4 py-1.5 rounded-md text-xs font-medium flex items-center gap-1 transition-colors disabled:opacity-40"
                  style={{ background: 'var(--accent)', color: '#fff' }}
                >
                  Next
                  <ChevronRight size={13} />
                </button>
              )}

              {mode === 'swarm' && step === 2 && (
                <button
                  onClick={handleLaunchSwarm}
                  disabled={coordinatorCount !== 1 || builderCount < 1}
                  className="px-4 py-1.5 rounded-md text-xs font-semibold transition-colors disabled:opacity-40"
                  style={{ background: 'var(--accent)', color: '#fff' }}
                  onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--accentHover)')}
                  onMouseLeave={(e) => (e.currentTarget.style.background = 'var(--accent)')}
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
