import { useState, useEffect, useRef, useCallback } from 'react'
import { X, Brain, Plus, PanelLeftClose, PanelLeftOpen, ImageIcon } from 'lucide-react'
import { useAthena } from './useAthena'
import { AthenaChatMessage } from './AthenaChatMessage'
import { AthenaInput } from './AthenaInput'
import type { AthenaInputHandle } from './AthenaInput'
import { AthenaThinkingIndicator } from './AthenaThinkingIndicator'
import { SessionList } from './SessionList'
import { useAthenaStore } from '../../store/athenaStore'
import type { ImageAttachment } from '../../store/athenaStore'
import { useSessionStore } from '../../store/sessionStore'

export function AthenaPanel() {
  const {
    messages,
    isOpen,
    isPtyReady,
    isStreaming,
    sendMessage,
    spawnAthena,
    setOpen,
    newSession,
  } = useAthena()
  const model = useAthenaStore((s) => s.model)
  const activeSessionId = useSessionStore((s) => s.activeSessionId)
  const isSessionsLoaded = useSessionStore((s) => s.isSessionsLoaded)
  const loadSessions = useSessionStore((s) => s.loadSessions)
  const [showSessionList, setShowSessionList] = useState(false)
  const [statusLog, setStatusLog] = useState<StatusLogItem[]>([])
  const [streamingText, setStreamingText] = useState('')
  const scrollRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<AthenaInputHandle>(null)

  const [isDragOver, setIsDragOver] = useState(false)
  const dragCounterRef = useRef(0)
  const dragTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const panelRef = useRef<HTMLDivElement>(null)

  const resetDragState = useCallback(() => {
    dragCounterRef.current = 0
    setIsDragOver(false)
    if (dragTimeoutRef.current) {
      clearTimeout(dragTimeoutRef.current)
      dragTimeoutRef.current = null
    }
  }, [])

  useEffect(() => {
    if (!isSessionsLoaded) loadSessions()
  }, [isSessionsLoaded, loadSessions])

  useEffect(() => {
    const onDragEnd = () => resetDragState()
    const onWindowDragLeave = () => {
      dragTimeoutRef.current = setTimeout(() => resetDragState(), 150)
    }
    const onWindowDragEnter = () => {
      if (dragTimeoutRef.current) {
        clearTimeout(dragTimeoutRef.current)
        dragTimeoutRef.current = null
      }
    }
    window.addEventListener('dragend', onDragEnd)
    window.addEventListener('dragleave', onWindowDragLeave)
    window.addEventListener('dragenter', onWindowDragEnter)
    return () => {
      window.removeEventListener('dragend', onDragEnd)
      window.removeEventListener('dragleave', onWindowDragLeave)
      window.removeEventListener('dragenter', onWindowDragEnter)
      if (dragTimeoutRef.current) clearTimeout(dragTimeoutRef.current)
    }
  }, [resetDragState])

  useEffect(() => {
    const unsubStatus = window.athena.pty.onAthenaStatus((statusData: StatusLogItem) => {
      if (statusData.status === 'streaming') {
        setStreamingText(statusData.streamedText || '')
      } else if (statusData.status === 'complete' || statusData.status === 'failed') {
        setStreamingText('')
      }
      setStatusLog((prev) => [...prev.slice(-4), statusData])
    })
    return () => {
      unsubStatus()
    }
  }, [])

  useEffect(() => {
    const unsubAsk = window.athena.pty.onAskUser((data: any) => {
      const { addMessage } = useAthenaStore.getState()
      addMessage({
        id: `ask-${data.requestId}`,
        role: 'athena',
        content: '',
        timestamp: Date.now(),
        blocks: [
          {
            type: 'ask_user',
            requestId: data.requestId,
            question: data.question,
            options: data.options,
          },
        ],
      })
    })

    const unsubPlan = window.athena.pty.onPlanUpdate((plan: any) => {
      const { messages } = useAthenaStore.getState()
      const existingIdx = messages.findIndex((m) =>
        m.blocks?.some((b) => b.type === 'plan' && b.planId === plan.id),
      )

      if (existingIdx >= 0) {
        const updated = [...messages]
        const msg = { ...updated[existingIdx] }
        msg.blocks = (msg.blocks || []).map((b) =>
          b.type === 'plan' && b.planId === plan.id
            ? { ...b, steps: plan.steps, status: plan.status }
            : b,
        )
        updated[existingIdx] = msg
        useAthenaStore.getState().setMessages(updated)
      }
    })

    const unsubEval = window.athena.pty.onPlanEvaluated((data: any) => {
      const { addMessage } = useAthenaStore.getState()
      addMessage({
        id: `eval-${Date.now()}`,
        role: 'athena',
        content: '',
        timestamp: Date.now(),
        blocks: [
          {
            type: 'evaluation',
            planId: data.planId,
            overallStatus: data.overallStatus,
            stepEvaluations: data.stepEvaluations,
            nextAction: data.nextAction,
            reasoning: data.reasoning,
          },
        ],
      })
    })

    return () => {
      unsubAsk()
      unsubPlan()
      unsubEval()
    }
  }, [])

  useEffect(() => {
    if (isOpen && !isPtyReady) {
      spawnAthena()
    }
  }, [isOpen, isPtyReady, spawnAthena])

  useEffect(() => {
    if (!isStreaming) {
      const timeout = setTimeout(() => {
        setStatusLog([])
        setStreamingText('')
      }, 1200)
      return () => clearTimeout(timeout)
    }
  }, [isStreaming])

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [messages, isStreaming, statusLog, streamingText])

  const handleDragEnter = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    if (!e.dataTransfer.types.some((t) => t === 'Files')) return
    dragCounterRef.current++
    setIsDragOver(true)
  }, [])

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    const related = e.relatedTarget as Node | null
    if (related && panelRef.current && panelRef.current.contains(related)) return
    dragCounterRef.current--
    if (dragCounterRef.current <= 0) {
      dragCounterRef.current = 0
      setIsDragOver(false)
    }
  }, [])

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault()
  }, [])

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault()
      resetDragState()
      const files = Array.from(e.dataTransfer.files)
      const imageFiles = files.filter((f) => f.type.startsWith('image/'))
      if (imageFiles.length > 0 && inputRef.current) {
        inputRef.current.addDroppedFiles(imageFiles)
      }
    },
    [resetDragState],
  )

  const handleSend = (msg: string, attachments?: ImageAttachment[]) => {
    sendMessage(msg, attachments)
    setStatusLog([])
  }

  const handleNewSession = async () => {
    await newSession()
    setStatusLog([])
  }

  if (!isOpen) return null

  const modelLabels: Record<string, string> = {
    claude: 'Claude Code',
    codex: 'Codex',
    opencode: 'OpenCode',
    gemini: 'Gemini CLI',
    custom: 'Custom',
  }

  return (
    <div
      className="shrink-0 flex border-l"
      style={{
        width: showSessionList ? 500 : 400,
        background: 'var(--bg)',
        borderColor: 'var(--border)',
      }}
    >
      {showSessionList && (
        <div
          className="shrink-0 flex flex-col border-r"
          style={{
            width: 200,
            borderColor: 'var(--border)',
            background: 'var(--bgSecondary)',
          }}
        >
          <SessionList />
        </div>
      )}

      <div
        ref={panelRef}
        className="flex-1 flex flex-col min-w-0 relative"
        onDragEnter={handleDragEnter}
        onDragLeave={handleDragLeave}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
      >
        {isDragOver && (
          <div
            className="absolute inset-0 z-50 flex items-center justify-center"
            style={{
              background: 'rgba(0,0,0,0.5)',
              backdropFilter: 'blur(2px)',
              border: '2px dashed var(--accent)',
              borderRadius: 4,
              pointerEvents: 'none',
              userSelect: 'none',
            }}
          >
            <div className="flex flex-col items-center gap-2">
              <ImageIcon size={28} style={{ color: 'var(--accent)' }} />
              <span className="text-xs font-medium" style={{ color: 'var(--accent)' }}>
                Drop images here
              </span>
            </div>
          </div>
        )}

        <div
          className="flex items-center justify-between px-3 py-1.5 shrink-0"
          style={{
            borderBottom: '1px solid var(--border)',
            background: 'var(--bgSecondary)',
          }}
        >
          <div className="flex items-center gap-1.5">
            <button
              onClick={() => setShowSessionList(!showSessionList)}
              className="p-1 rounded hover:bg-white/10 transition-colors"
              title={showSessionList ? 'Hide sessions' : 'Show sessions'}
            >
              {showSessionList ? (
                <PanelLeftClose size={13} style={{ color: 'var(--textMuted)' }} />
              ) : (
                <PanelLeftOpen size={13} style={{ color: 'var(--textMuted)' }} />
              )}
            </button>
            <Brain size={13} style={{ color: 'var(--accent)' }} />
            <span
              className="text-[11px] font-semibold tracking-wide"
              style={{ color: 'var(--text)' }}
            >
              Athena
            </span>
            <span
              className="text-[8px] px-1.5 py-px rounded-full"
              style={{
                background: 'color-mix(in srgb, var(--accent) 12%, transparent)',
                color: 'var(--accent)',
              }}
            >
              {modelLabels[model] ?? model}
            </span>
          </div>
          <div className="flex items-center gap-0.5">
            <button
              onClick={handleNewSession}
              className="p-1 rounded hover:bg-white/10 transition-colors"
              title="New Session"
            >
              <Plus size={13} style={{ color: 'var(--textDim)' }} />
            </button>
            <button
              onClick={() => setOpen(false)}
              className="p-1 rounded hover:bg-white/10 transition-colors"
              title="Close"
            >
              <X size={13} style={{ color: 'var(--textDim)' }} />
            </button>
          </div>
        </div>

        <div ref={scrollRef} className="flex-1 overflow-y-auto flex flex-col">
          {messages.length === 0 && !isStreaming && (
            <div className="flex-1 flex items-center justify-center px-4">
              <div className="flex flex-col items-center gap-3 text-center">
                <div
                  className="w-10 h-10 rounded-lg flex items-center justify-center"
                  style={{ background: 'color-mix(in srgb, var(--accent) 8%, transparent)' }}
                >
                  <Brain size={20} style={{ color: 'var(--accent)', opacity: 0.6 }} />
                </div>
                <p
                  className="text-[11px] leading-relaxed max-w-[240px]"
                  style={{ color: 'var(--textDim)' }}
                >
                  {activeSessionId
                    ? 'Continue this conversation or start fresh.'
                    : 'Orchestrate agents, run tasks, or explore your codebase.'}
                </p>
              </div>
            </div>
          )}
          {messages.map((msg) => (
            <AthenaChatMessage key={msg.id} message={msg} />
          ))}
          {isStreaming && streamingText && (
            <div
              className="flex gap-2.5 py-2.5 px-3 w-full"
              style={{
                background: 'color-mix(in srgb, var(--accent) 3%, transparent)',
                borderBottom: '1px solid color-mix(in srgb, var(--border) 40%, transparent)',
              }}
            >
              <div className="shrink-0 mt-0.5">
                <div
                  className="w-5 h-5 rounded flex items-center justify-center"
                  style={{ background: 'var(--accent)' }}
                >
                  <Brain size={11} style={{ color: '#fff' }} />
                </div>
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5 mb-0.5">
                  <span className="text-[11px] font-medium" style={{ color: 'var(--accent)' }}>
                    Athena
                  </span>
                  <span
                    className="text-[8px] px-1 py-px rounded-full animate-pulse"
                    style={{
                      background: 'color-mix(in srgb, var(--accent) 15%, transparent)',
                      color: 'var(--accent)',
                    }}
                  >
                    streaming
                  </span>
                </div>
                <div
                  className="text-xs leading-relaxed whitespace-pre-wrap break-words"
                  style={{ color: 'var(--text)' }}
                >
                  {streamingText}
                </div>
              </div>
            </div>
          )}
          {isStreaming && <AthenaThinkingIndicator statusLog={statusLog} />}
        </div>

        <AthenaInput ref={inputRef} onSend={handleSend} disabled={!isPtyReady || isStreaming} />
      </div>
    </div>
  )
}
