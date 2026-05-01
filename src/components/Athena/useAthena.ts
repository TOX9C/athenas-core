import { useCallback } from 'react'
import { nanoid } from 'nanoid'
import { useAthenaStore } from '../../store/athenaStore'
import { useSessionStore } from '../../store/sessionStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import type { AthenaMessage, ImageAttachment } from '../../store/athenaStore'

export function useAthena() {
  const messages = useAthenaStore((s) => s.messages)
  const isOpen = useAthenaStore((s) => s.isOpen)
  const addMessage = useAthenaStore((s) => s.addMessage)
  const setOpen = useAthenaStore((s) => s.setOpen)
  const setStreaming = useAthenaStore((s) => s.setStreaming)
  const toggleOpen = useAthenaStore((s) => s.toggleOpen)

  const activeSessionId = useSessionStore((s) => s.activeSessionId)
  const addMessageToActiveSession = useSessionStore((s) => s.addMessageToActiveSession)
  const createSession = useSessionStore((s) => s.createSession)
  const newSession = useSessionStore((s) => s.newSession)

  const activeSpace = useWorkspaceStore((s) => s.spaces.find((sp) => sp.id === s.activeSpaceId))

  const ensureSession = useCallback(async (): Promise<string | null> => {
    if (activeSessionId) return activeSessionId
    try {
      if (!window.athena?.session?.create) {
        console.error('[useAthena] session IPC bridge not available on window.athena')
        return null
      }
      const id = await createSession()
      if (!id) {
        console.error('[useAthena] createSession returned null — IPC may have failed')
      }
      return id
    } catch (err) {
      console.error('[useAthena] Failed to create session:', err)
      return null
    }
  }, [activeSessionId, createSession])

  const sendMessage = useCallback(
    async (text: string, attachments?: ImageAttachment[]) => {
      if (!text.trim() && (!attachments || attachments.length === 0)) return

      const userMsg: AthenaMessage = {
        id: nanoid(),
        role: 'user',
        content: text.trim(),
        timestamp: Date.now(),
        images: attachments && attachments.length > 0 ? attachments : undefined,
      }

      addMessage(userMsg)
      const sessionId = await ensureSession()
      setStreaming(true)
      try {
        let response: string
        const ipcImages = attachments
          ?.filter((img) => img.base64 && img.base64.length > 0)
          .map((img) => ({ base64: img.base64, mediaType: img.mediaType }))

        const hasValidImages = ipcImages && ipcImages.length > 0
        if (hasValidImages && attachments && ipcImages.length < attachments.length) {
          console.warn('[useAthena] Some attachments had empty base64 data and were dropped')
        }

        if (sessionId && hasValidImages && window.athena?.orchestrator?.chatWithImages) {
          response = await window.athena.orchestrator.chatWithImages(
            text.trim(),
            ipcImages,
            sessionId,
          )
        } else if (sessionId && window.athena?.orchestrator?.chatWithSession) {
          response = await window.athena.orchestrator.chatWithSession(text.trim(), sessionId)
        } else if (window.athena?.orchestrator?.chat) {
          response = await window.athena.orchestrator.chat(text.trim())
        } else {
          response = 'Error: No orchestrator available.'
        }

        const athenaMsg: AthenaMessage = {
          id: nanoid(),
          role: 'athena',
          content: response,
          timestamp: Date.now(),
        }

        addMessage(athenaMsg)
        if (sessionId) {
          await addMessageToActiveSession(userMsg)
          await addMessageToActiveSession(athenaMsg)
        }
      } catch (err: unknown) {
        const errorMsg: AthenaMessage = {
          id: nanoid(),
          role: 'athena',
          content: `Error communicating with orchestrator: ${err instanceof Error ? err.message : err}`,
          timestamp: Date.now(),
          isError: true,
        }

        addMessage(errorMsg)
        if (sessionId) {
          await addMessageToActiveSession(userMsg)
          await addMessageToActiveSession(errorMsg)
        }
      } finally {
        setStreaming(false)
      }
    },
    [addMessage, addMessageToActiveSession, ensureSession, setStreaming],
  )

  return {
    messages,
    isOpen,
    isStreaming: useAthenaStore((s) => s.isStreaming),
    isPtyReady: true,
    sendMessage,
    spawnAthena: () => {},
    toggleOpen,
    setOpen,
    newSession,
  }
}
