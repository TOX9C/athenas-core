import { useEffect, useRef, useCallback } from 'react'
import { nanoid } from 'nanoid'
import { useAthenaStore } from '../../store/athenaStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { stripAnsi } from '../../utils/ansi'

const ATHENA_PTY_ID = '__athena__'

export function useAthena() {
  const {
    messages, isOpen, isPtyReady, model, bypassMode, customAgents,
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
      default: {
        const custom = customAgents.find(a => a.id === model)
        if (custom) return custom.command
        return 'claude'
      }
    }
  }, [model, bypassMode, customAgents])

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

  // Re-spawn the underlying agent PTY if the user modifies CLI parameters while it's running
  useEffect(() => {
    if (isPtyReady && activeSpace) {
      // The backend ptyManager.ts explicitly kills older sessions sharing the same ID during spawn
      spawnAthena()
    }
    // We intentionally only listen to configuration dependencies
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [model, bypassMode, customAgents])

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
