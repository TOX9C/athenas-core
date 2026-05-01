import { useState } from 'react'
import { X, Users, Plus, Minus } from 'lucide-react'
import { nanoid } from 'nanoid'
import type { AgentType } from '../../types/workspace'
import type { AgentRole, SwarmState, SwarmAgent } from '../../types/swarm'
import { useSwarmStore } from '../../store/swarmStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { getAgentLabel, getAgentColor, getAgentCommand } from '../../utils/agentCommands'

const ROLE_OPTIONS: { role: AgentRole; label: string; color: string }[] = [
  { role: 'coordinator', label: 'Coordinator', color: '#0ea5e9' },
  { role: 'builder', label: 'Builder', color: '#22c55e' },
  { role: 'scout', label: 'Scout', color: '#f59e0b' },
  { role: 'reviewer', label: 'Reviewer', color: '#06b6d4' },
]

const AGENT_TYPES: AgentType[] = ['claude', 'codex', 'opencode', 'gemini', 'shell']

interface SwarmModalProps {
  onClose: () => void
}

interface AgentSlot {
  role: AgentRole
  agentType: AgentType
}

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

export function SwarmModal({ onClose }: SwarmModalProps) {
  const [goal, setGoal] = useState('')
  const [slots, setSlots] = useState<AgentSlot[]>([
    { role: 'coordinator', agentType: 'claude' },
    { role: 'builder', agentType: 'claude' },
    { role: 'builder', agentType: 'claude' },
  ])

  const { setSwarm } = useSwarmStore()
  const activeSpace = useWorkspaceStore((s) => {
    return s.spaces.find((sp) => sp.id === s.activeSpaceId)
  })

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
  const canLaunch = goal.trim() && coordinatorCount === 1 && builderCount >= 1 && activeSpace

  const handleLaunch = async () => {
    if (!canLaunch || !activeSpace) return

    const agents: SwarmAgent[] = slots.map((slot, idx) => ({
      id: nanoid(),
      role: slot.role,
      agentType: slot.agentType,
      paneId: `swarm-${nanoid()}`,
      status: 'idle' as const,
      currentTask: null,
      lastAction: 'Spawned',
      lastActionAt: Date.now(),
    }))

    const state: SwarmState = {
      id: nanoid(),
      goal: goal.trim(),
      agents,
      tasks: [],
      messages: [],
      status: 'active',
      startedAt: Date.now(),
    }

    await window.athena.swarm.writeState(activeSpace.dir, state)
    setSwarm(state)

    for (const agent of agents) {
      const agentCmd = getAgentCommand(agent.agentType)
      const shell = '/bin/zsh'
      await window.athena.pty.spawn(agent.paneId, activeSpace.dir, shell, agentCmd)

      const rolePrompt = ROLE_PROMPTS[agent.role]
      const fullPrompt = `${rolePrompt}\n\nGOAL: ${goal.trim()}\n\nStart working now.`

      setTimeout(() => {
        window.athena.pty.write(agent.paneId, fullPrompt + '\n')
      }, 1500)
    }

    onClose()
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
        className="rounded-xl shadow-2xl flex flex-col overflow-hidden"
        style={{
          width: 560,
          maxHeight: '80vh',
          background: 'var(--bgSecondary)',
          border: '1px solid var(--border)',
        }}
      >
        <div
          className="flex items-center justify-between px-5 py-3.5 border-b"
          style={{ borderColor: 'var(--border)' }}
        >
          <div className="flex items-center gap-2">
            <Users size={16} style={{ color: 'var(--accent)' }} />
            <h2 className="text-sm font-semibold" style={{ color: 'var(--text)' }}>
              Launch Swarm
            </h2>
          </div>
          <button onClick={onClose} className="p-1 rounded hover:bg-white/10 transition-colors">
            <X size={16} style={{ color: 'var(--textMuted)' }} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-5 flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
              Goal
            </label>
            <textarea
              autoFocus
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

          <div className="flex flex-col gap-2">
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
        </div>

        <div
          className="flex items-center justify-end px-5 py-3 border-t gap-2"
          style={{ borderColor: 'var(--border)' }}
        >
          <button
            onClick={onClose}
            className="px-3 py-1.5 rounded-md text-xs font-medium"
            style={{ color: 'var(--textMuted)', background: 'var(--bgTertiary)' }}
          >
            Cancel
          </button>
          <button
            onClick={handleLaunch}
            disabled={!canLaunch}
            className="px-4 py-1.5 rounded-md text-xs font-semibold transition-colors disabled:opacity-40"
            style={{ background: 'var(--accent)', color: '#fff' }}
          >
            Launch Swarm
          </button>
        </div>
      </div>
    </div>
  )
}
