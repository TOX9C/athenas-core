import { useRef, useEffect, useCallback } from 'react'
import { useAgentOutputStore } from '../../store/agentOutputStore'
import { AgentOutputLine } from './AgentOutputLine'
import { AgentSelector } from './AgentSelector'
import { Trash2, ArrowDown } from 'lucide-react'

export function AgentOutputPanel() {
  const selectedPaneId = useAgentOutputStore((s) => s.selectedPaneId)
  const buffers = useAgentOutputStore((s) => s.buffers)
  const autoScroll = useAgentOutputStore((s) => s.autoScroll)
  const setAutoScroll = useAgentOutputStore((s) => s.setAutoScroll)
  const clearBuffer = useAgentOutputStore((s) => s.clearBuffer)
  const scrollRef = useRef<HTMLDivElement>(null)
  const lines = selectedPaneId ? (buffers[selectedPaneId] ?? []) : []

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [lines.length, autoScroll])

  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current
    const atBottom = scrollHeight - scrollTop - clientHeight < 40
    if (atBottom !== autoScroll) setAutoScroll(atBottom)
  }, [autoScroll, setAutoScroll])

  if (!selectedPaneId) {
    return (
      <div className="flex flex-col h-full">
        <div className="border-b px-2 py-1.5" style={{ borderColor: 'var(--border)' }}>
          <AgentSelector />
        </div>
        <div
          className="flex-1 flex items-center justify-center"
          style={{ color: 'var(--textDim)' }}
        >
          <span className="text-[10px]">Select an agent to view output</span>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <div
        className="flex items-center gap-1 border-b px-2 py-1.5 shrink-0"
        style={{ borderColor: 'var(--border)' }}
      >
        <div className="flex-1 min-w-0">
          <AgentSelector />
        </div>
        <button
          onClick={() => clearBuffer(selectedPaneId)}
          className="p-1 rounded hover:bg-white/10 transition-colors"
          title="Clear output"
        >
          <Trash2 size={11} style={{ color: 'var(--textDim)' }} />
        </button>
        {!autoScroll && (
          <button
            onClick={() => {
              setAutoScroll(true)
            }}
            className="p-1 rounded hover:bg-white/10 transition-colors"
            title="Scroll to bottom"
          >
            <ArrowDown size={11} style={{ color: 'var(--accent)' }} />
          </button>
        )}
      </div>

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto overflow-x-hidden"
        style={{ background: 'var(--bg)' }}
      >
        {lines.length === 0 ? (
          <div
            className="flex items-center justify-center h-full"
            style={{ color: 'var(--textDim)' }}
          >
            <span className="text-[10px]">No output captured yet</span>
          </div>
        ) : (
          lines.map((line) => (
            <AgentOutputLine key={`${line.paneId}-${line.lineNum}`} line={line} showLineNumbers />
          ))
        )}
      </div>

      <div
        className="flex items-center justify-between px-2 py-0.5 border-t shrink-0 text-[9px]"
        style={{
          borderColor: 'var(--border)',
          background: 'var(--bgSecondary)',
          color: 'var(--textDim)',
        }}
      >
        <span>{lines.length} lines</span>
        <span>{selectedPaneId.slice(0, 16)}</span>
      </div>
    </div>
  )
}
