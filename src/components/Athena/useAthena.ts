import { useEffect, useRef, useCallback } from 'react'
import { nanoid } from 'nanoid'
import { useAthenaStore } from '../../store/athenaStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { stripAnsi } from '../../utils/ansi'

const ATHENA_PTY_ID = '__athena__'

export function useAthena() {
  const {
    messages, isOpen, isPtyReady, model, bypassMode, customCommand,
    addMessage, setPtyReady, toggleOpen, setOpen,
  } = useAthenaStore()

  const activeSpace = useWorkspaceStore((s) => {
    return s.spaces.find((sp) => sp.id === s.activeSpaceId)
  })

  const bufferRef = useRef('')
  const collectingRef = useRef(false)
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const getCommand = useCallback(() => {
    switch (model) {
      case 'claude':
        return bypassMode ? 'claude --dangerously-skip-permissions' : 'claude'
      case 'codex':
        return 'codex'
      case 'opencode':
        return 'opencode'
      case 'gemini':
        return 'gemini'
      case 'custom':
        return customCommand || ''
      default:
        return 'claude'
    }
  }, [model, bypassMode, customCommand])

  const spawnAthena = useCallback(async () => {
    if (!activeSpace) return

    const shell = '/bin/zsh'
    const agentCmd = getCommand()
    await window.athena.pty.spawn(ATHENA_PTY_ID, activeSpace.dir, shell, agentCmd || undefined)
    setPtyReady(true)
  }, [activeSpace, getCommand, setPtyReady])

  useEffect(() => {
    if (!activeSpace) return

    const unsub = window.athena.pty.onData(ATHENA_PTY_ID, (data) => {
      if (!collectingRef.current) return

      const clean = stripAnsi(data)
      if (clean) {
        bufferRef.current += clean
      }

      if (timeoutRef.current) clearTimeout(timeoutRef.current)
      timeoutRef.current = setTimeout(() => {
        const content = bufferRef.current.trim()
        const isPrompt = content.endsWith('%') || content.endsWith('$') || content.endsWith('#')
        if (content && !isPrompt) {
          addMessage({
            id: nanoid(),
            role: 'athena',
            content,
            timestamp: Date.now(),
          })
        }
        bufferRef.current = ''
        collectingRef.current = false
      }, 2000)
    })

    return () => {
      unsub()
      if (timeoutRef.current) clearTimeout(timeoutRef.current)
    }
  }, [activeSpace, addMessage])

  const sendMessage = useCallback(
    (text: string) => {
      if (!text.trim() || !isPtyReady) return

      addMessage({
        id: nanoid(),
        role: 'user',
        content: text.trim(),
        timestamp: Date.now(),
      })

      bufferRef.current = ''
      collectingRef.current = true
      window.athena.pty.write(ATHENA_PTY_ID, text.trim() + '\n')
    },
    [isPtyReady, addMessage]
  )

  return {
    messages,
    isOpen,
    isPtyReady,
    sendMessage,
    spawnAthena,
    toggleOpen,
    setOpen,
  }
}
