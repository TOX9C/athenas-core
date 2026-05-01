import { useState, useEffect } from 'react'
import { MessageSquare, Send, X } from 'lucide-react'
import { Modal } from '../shared/Modal'
import { Button } from '../shared/Button'
import { useNotificationStore, isEnhanced } from '../../store/notificationStore'
import { getAgentColor, getAgentLabel } from '../../utils/agentCommands'
import type { EnhancedNotification } from '../../store/notificationStore'

export function InputRequestModal() {
  const { notifications, respondToInput, setInputResponding } = useNotificationStore()
  const [activeIdx, setActiveIdx] = useState(0)
  const [freeText, setFreeText] = useState('')

  const pending = useNotificationStore.getState().pendingInputRequests()
  const current = pending[activeIdx]

  useEffect(() => {
    if (pending.length > 0 && activeIdx >= pending.length) {
      setActiveIdx(0)
    }
  }, [pending.length, activeIdx])

  if (!current || pending.length === 0) return null

  const handleRespond = (response: string) => {
    setInputResponding(current.id, true)
    respondToInput(current.id, response)
    window.athena?.plugin?.respondToInput?.(current.inputRequestId ?? current.id, response)
    setFreeText('')
  }

  return (
    <Modal title="Agent Request" onClose={() => {}} width={440}>
      <div className="flex flex-col gap-4">
        <div className="flex items-center gap-2">
          <div
            className="w-2 h-2 rounded-full"
            style={{
              background: current.agentType ? getAgentColor(current.agentType) : 'var(--accent)',
            }}
          />
          <span className="text-[11px] font-semibold" style={{ color: 'var(--text)' }}>
            {current.agentType ? getAgentLabel(current.agentType) : 'Agent'}
          </span>
          <span
            className="text-[9px] px-1.5 py-0.5 rounded-full"
            style={{ background: '#f9731622', color: '#f97316' }}
          >
            Needs Input
          </span>
          {pending.length > 1 && (
            <span className="text-[9px] ml-auto" style={{ color: 'var(--textDim)' }}>
              {activeIdx + 1} of {pending.length}
            </span>
          )}
        </div>

        <div
          className="rounded-lg p-3"
          style={{ background: 'var(--bgTertiary)', border: '1px solid var(--border)' }}
        >
          <div className="flex items-start gap-2">
            <MessageSquare size={14} style={{ color: '#f97316', flexShrink: 0, marginTop: 2 }} />
            <p className="text-[12px] leading-relaxed" style={{ color: 'var(--text)' }}>
              {current.inputRequestPrompt ?? current.message}
            </p>
          </div>
        </div>

        {current.inputRequestOptions && current.inputRequestOptions.length > 0 && (
          <div className="flex flex-col gap-1.5">
            {current.inputRequestOptions.map((option) => (
              <button
                key={option}
                onClick={() => handleRespond(option)}
                disabled={current.inputResponding}
                className="flex items-center gap-2 px-3 py-2 rounded-md text-[11px] font-medium transition-colors text-left"
                style={{
                  background: 'var(--bgTertiary)',
                  border: '1px solid var(--border)',
                  color: 'var(--text)',
                  opacity: current.inputResponding ? 0.5 : 1,
                }}
              >
                <Send size={10} style={{ color: 'var(--accent)' }} />
                {option}
              </button>
            ))}
          </div>
        )}

        <div className="flex items-center gap-2">
          <input
            type="text"
            value={freeText}
            onChange={(e) => setFreeText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && freeText.trim()) {
                handleRespond(freeText.trim())
              }
            }}
            placeholder="Type a response..."
            disabled={current.inputResponding}
            className="flex-1 px-3 py-2 rounded-md text-[11px] outline-none"
            style={{
              background: 'var(--bgTertiary)',
              border: '1px solid var(--border)',
              color: 'var(--text)',
            }}
          />
          <Button
            size="sm"
            onClick={() => handleRespond(freeText.trim())}
            disabled={!freeText.trim() || current.inputResponding}
          >
            <Send size={11} />
            Send
          </Button>
        </div>

        {pending.length > 1 && (
          <div
            className="flex items-center justify-between pt-2 border-t"
            style={{ borderColor: 'var(--border)' }}
          >
            <button
              onClick={() => setActiveIdx(Math.max(0, activeIdx - 1))}
              disabled={activeIdx === 0}
              className="text-[10px] px-2 py-1 rounded transition-colors"
              style={{ color: activeIdx === 0 ? 'var(--textDim)' : 'var(--accent)' }}
            >
              Previous
            </button>
            <span className="text-[9px]" style={{ color: 'var(--textDim)' }}>
              {pending.filter((p) => p.inputResponse).length} resolved
            </span>
            <button
              onClick={() => setActiveIdx(Math.min(pending.length - 1, activeIdx + 1))}
              disabled={activeIdx >= pending.length - 1}
              className="text-[10px] px-2 py-1 rounded transition-colors"
              style={{
                color: activeIdx >= pending.length - 1 ? 'var(--textDim)' : 'var(--accent)',
              }}
            >
              Next
            </button>
          </div>
        )}
      </div>
    </Modal>
  )
}
