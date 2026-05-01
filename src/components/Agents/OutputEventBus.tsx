import { useEffect } from 'react'
import { useAgentOutputStore } from '../../store/agentOutputStore'

export function OutputEventBus() {
  const setAgents = useAgentOutputStore((s) => s.setAgents)
  const setLines = useAgentOutputStore((s) => s.setLines)
  const clearBuffer = useAgentOutputStore((s) => s.clearBuffer)

  useEffect(() => {
    const unsubs: Array<() => void> = []

    unsubs.push(
      window.athena.outputCapture.onPaneRegistered(() => {
        window.athena.outputCapture
          .listAgents()
          .then(setAgents)
          .catch(() => {})
      }),
    )

    unsubs.push(
      window.athena.outputCapture.onPaneUnregistered((data) => {
        clearBuffer(data.paneId)
        window.athena.outputCapture
          .listAgents()
          .then(setAgents)
          .catch(() => {})
      }),
    )

    window.athena.outputCapture
      .listAgents()
      .then(setAgents)
      .catch(() => {})

    return () => {
      unsubs.forEach((u) => u())
    }
  }, [setAgents, setLines, clearBuffer])

  return null
}
