import { ChevronDown, Circle } from 'lucide-react'
import { useState, useRef, useEffect } from 'react'
import { useAgentOutputStore } from '../../store/agentOutputStore'
import { getAgentColor, getAgentLabel } from '../../utils/agentCommands'
import type { AgentType } from '../../types/workspace'
import type { AgentOutputInfo } from '../../store/agentOutputStore'

function AgentOption({
  agent,
  isSelected,
  onSelect,
}: {
  agent: AgentOutputInfo
  isSelected: boolean
  onSelect: () => void
}) {
  const at = agent.agentType as AgentType
  return (
    <button
      onClick={onSelect}
      className="w-full flex items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-white/[0.06]"
      style={{ background: isSelected ? 'var(--bgTertiary)' : undefined }}
    >
      <Circle size={6} fill={getAgentColor(at)} style={{ color: getAgentColor(at) }} />
      <span className="text-[11px] font-medium flex-1 truncate" style={{ color: 'var(--text)' }}>
        {agent.paneId.slice(0, 12)}
      </span>
      <span
        className="text-[9px] px-1 py-px rounded"
        style={{ background: `${getAgentColor(at)}22`, color: getAgentColor(at) }}
      >
        {getAgentLabel(at)}
      </span>
      <span className="text-[8px]" style={{ color: 'var(--textDim)' }}>
        {agent.lineCount} lines
      </span>
    </button>
  )
}

export function AgentSelector() {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const agents = useAgentOutputStore((s) => s.agents)
  const selectedPaneId = useAgentOutputStore((s) => s.selectedPaneId)
  const selectAgent = useAgentOutputStore((s) => s.selectAgent)

  const selected = agents.find((a) => a.paneId === selectedPaneId)

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    if (open) document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [open])

  if (agents.length === 0) {
    return (
      <div
        className="flex items-center gap-1.5 px-2 py-1 text-[10px]"
        style={{ color: 'var(--textDim)' }}
      >
        <Circle size={5} style={{ opacity: 0.3 }} />
        No agents with output
      </div>
    )
  }

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 px-2 py-1 rounded transition-colors hover:bg-white/[0.06] w-full"
      >
        <Circle
          size={6}
          fill={selected ? getAgentColor(selected.agentType as AgentType) : 'var(--textDim)'}
          style={{
            color: selected ? getAgentColor(selected.agentType as AgentType) : 'var(--textDim)',
          }}
        />
        <span
          className="text-[11px] font-medium flex-1 text-left truncate"
          style={{ color: 'var(--text)' }}
        >
          {selected ? selected.paneId.slice(0, 12) : 'Select agent...'}
        </span>
        <ChevronDown
          size={10}
          style={{ color: 'var(--textDim)' }}
          className={`transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>

      {open && (
        <div
          className="absolute top-full left-0 right-0 z-50 border rounded-md shadow-lg overflow-hidden"
          style={{
            background: 'var(--bgSecondary)',
            borderColor: 'var(--border)',
            maxHeight: 240,
            overflowY: 'auto',
          }}
        >
          {agents.map((agent) => (
            <AgentOption
              key={agent.paneId}
              agent={agent}
              isSelected={agent.paneId === selectedPaneId}
              onSelect={() => {
                selectAgent(agent.paneId)
                setOpen(false)
              }}
            />
          ))}
        </div>
      )}
    </div>
  )
}
