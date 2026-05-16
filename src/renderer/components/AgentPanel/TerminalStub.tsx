import { useRef } from 'react'
import { useTerminal } from '../../../components/Terminal/useTerminal'

export interface TerminalStubProps {
  paneId: string
  cwd: string
  agentCmd?: string
}

export default function TerminalStub({ paneId, cwd, agentCmd }: TerminalStubProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  useTerminal({ paneId, cwd, agentCmd }, containerRef)

  return (
    <div
      ref={containerRef}
      className="agent-panel-terminal-stub"
      style={{ width: '100%', height: '100%' }}
    />
  )
}
