import { useCallback, useEffect } from 'react'
import { nanoid } from 'nanoid'
import { useAthenaStore } from '../../store/athenaStore'
import { useWorkspaceStore } from '../../store/workspaceStore'

export function useAthena() {
  const {
    messages, isOpen, isPtyReady, model, bypassMode, customAgents,
    addMessage, setPtyReady, toggleOpen, setOpen,
  } = useAthenaStore()

  const activeSpace = useWorkspaceStore((s) => {
    return s.spaces.find((sp) => sp.id === s.activeSpaceId)
  })

  // We can just set it to ready since we don't spawn a PTY anymore
  useEffect(() => {
    if (!isPtyReady) {
      setPtyReady(true)
    }
  }, [isPtyReady, setPtyReady])

  const spawnAthena = useCallback(async () => {
    // Keep the signature but it doesn't need to do anything with PTY now
    setPtyReady(true)
  }, [setPtyReady])

  const sendMessage = useCallback(
    async (text: string) => {
      if (!text.trim()) return

      addMessage({
        id: nanoid(),
        role: 'user',
        content: text.trim(),
        timestamp: Date.now(),
      })

      try {
        const response = await window.athena.orchestrator.chat(text.trim(), activeSpace?.id)
        addMessage({
          id: nanoid(),
          role: 'athena',
          content: response,
          timestamp: Date.now(),
        })
      } catch (err: any) {
        addMessage({
          id: nanoid(),
          role: 'athena',
          content: `Error communicating with orchestrator: ${err?.message || err}`,
          timestamp: Date.now(),
        })
      }
    },
    [addMessage, activeSpace?.id]
  )

  return {
    messages,
    isOpen,
    isPtyReady: true,
    sendMessage,
    spawnAthena,
    toggleOpen,
    setOpen,
  }
}
