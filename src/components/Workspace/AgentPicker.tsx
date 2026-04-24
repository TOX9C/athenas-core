import type { AgentType, GridTemplate } from '../../types/workspace'
import { getAgentLabel, getAgentColor } from '../../utils/agentCommands'

const AGENT_OPTIONS: { type: AgentType; label: string }[] = [
  { type: 'shell', label: 'Shell only' },
  { type: 'claude', label: 'Claude Code' },
  { type: 'codex', label: 'Codex' },
  { type: 'opencode', label: 'OpenCode' },
  { type: 'gemini', label: 'Gemini CLI' },
  { type: 'custom', label: 'Custom...' },
]

interface PaneAgent {
  agentType: AgentType
  customCmd?: string
}

interface AgentPickerProps {
  grid: GridTemplate
  paneAgents: PaneAgent[]
  onChange: (agents: PaneAgent[]) => void
}

const GRID_COLS: Record<GridTemplate, number> = {
  '1x1': 1, '1x2': 2, '2x2': 2, '2x3': 3, '3x3': 3, '3x4': 4, '4x4': 4,
}

export function AgentPicker({ grid, paneAgents, onChange }: AgentPickerProps) {
  const cols = GRID_COLS[grid]

  const updateAgent = (index: number, agentType: AgentType) => {
    const next = paneAgents.map((pa, i) =>
      i === index ? { ...pa, agentType, customCmd: agentType === 'custom' ? pa.customCmd : undefined } : pa
    )
    onChange(next)
  }

  const updateCustomCmd = (index: number, cmd: string) => {
    const next = paneAgents.map((pa, i) =>
      i === index ? { ...pa, customCmd: cmd } : pa
    )
    onChange(next)
  }

  return (
    <div className="flex flex-col gap-3">
      <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
        Assign agents to each pane
      </label>
      <div
        className="grid gap-2"
        style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}
      >
        {paneAgents.map((pa, idx) => (
          <div
            key={idx}
            className="p-2.5 rounded-lg flex flex-col gap-1.5"
            style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center gap-1.5 mb-1">
              <div
                className="w-2 h-2 rounded-full"
                style={{ background: getAgentColor(pa.agentType) }}
              />
              <span className="text-[10px] font-medium" style={{ color: 'var(--textDim)' }}>
                Pane {idx + 1}
              </span>
            </div>
            <select
              value={pa.agentType}
              onChange={(e) => updateAgent(idx, e.target.value as AgentType)}
              className="w-full px-2 py-1 rounded text-[11px] outline-none"
              style={{
                background: 'var(--bgTertiary)',
                color: 'var(--text)',
                border: '1px solid var(--border)',
              }}
            >
              {AGENT_OPTIONS.map((opt) => (
                <option key={opt.type} value={opt.type}>{opt.label}</option>
              ))}
            </select>
            {pa.agentType === 'custom' && (
              <input
                value={pa.customCmd ?? ''}
                onChange={(e) => updateCustomCmd(idx, e.target.value)}
                placeholder="e.g. aider"
                className="w-full px-2 py-1 rounded text-[11px] outline-none"
                style={{
                  background: 'var(--bgTertiary)',
                  color: 'var(--text)',
                  border: '1px solid var(--border)',
                }}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  )
}
