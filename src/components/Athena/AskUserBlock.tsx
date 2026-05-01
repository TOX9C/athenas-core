import { useState } from 'react'
import type { AskUserBlock as AskUserBlockType } from '../../store/athenaStore'
import { MessageCircleQuestion } from 'lucide-react'

interface AskUserBlockProps {
  block: AskUserBlockType
}

export function AskUserBlock({ block }: AskUserBlockProps) {
  const [answered, setAnswered] = useState(block.answered ?? false)
  const [selected, setSelected] = useState(block.selectedAnswer ?? '')
  const [customInput, setCustomInput] = useState('')

  const handleSelect = (answer: string) => {
    if (answered) return
    setAnswered(true)
    setSelected(answer)
    block.answered = true
    block.selectedAnswer = answer
    window.athena.pty.answerUser(block.requestId, answer)
  }

  const handleCustomSubmit = () => {
    if (!customInput.trim() || answered) return
    handleSelect(customInput.trim())
  }

  return (
    <div
      className="rounded-md p-3 mt-1.5 mb-1"
      style={{
        background: 'color-mix(in srgb, var(--accent) 5%, transparent)',
        border: '1px solid color-mix(in srgb, var(--accent) 20%, transparent)',
      }}
    >
      <div className="flex items-start gap-2 mb-2">
        <MessageCircleQuestion size={14} style={{ color: 'var(--accent)', marginTop: 1 }} />
        <span className="text-xs font-medium" style={{ color: 'var(--text)' }}>
          {block.question}
        </span>
      </div>

      <div className="flex flex-col gap-1.5 ml-5">
        {block.options.map((opt, i) => {
          const isSelected = answered && selected === opt.label
          return (
            <button
              key={i}
              onClick={() => handleSelect(opt.label)}
              disabled={answered}
              className="text-left px-2.5 py-1.5 rounded transition-all"
              style={{
                background: isSelected
                  ? 'var(--accent)'
                  : 'color-mix(in srgb, var(--bgTertiary) 80%, transparent)',
                color: isSelected ? '#fff' : 'var(--text)',
                border: `1px solid ${isSelected ? 'var(--accent)' : 'color-mix(in srgb, var(--border) 60%, transparent)'}`,
                opacity: answered && !isSelected ? 0.5 : 1,
                cursor: answered ? 'default' : 'pointer',
              }}
            >
              <span className="text-[11px] font-medium block">{opt.label}</span>
              {opt.description && (
                <span
                  className="text-[10px] block mt-0.5"
                  style={{ color: isSelected ? 'rgba(255,255,255,0.8)' : 'var(--textDim)' }}
                >
                  {opt.description}
                </span>
              )}
            </button>
          )
        })}

        {!answered && (
          <div className="flex gap-1.5 mt-1">
            <input
              type="text"
              value={customInput}
              onChange={(e) => setCustomInput(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleCustomSubmit()}
              placeholder="Or type a custom response..."
              className="flex-1 text-[10px] px-2 py-1 rounded"
              style={{
                background: 'var(--bgTertiary)',
                color: 'var(--text)',
                border: '1px solid var(--border)',
                outline: 'none',
              }}
            />
            <button
              onClick={handleCustomSubmit}
              disabled={!customInput.trim()}
              className="text-[10px] px-2 py-1 rounded"
              style={{
                background: customInput.trim() ? 'var(--accent)' : 'var(--bgTertiary)',
                color: customInput.trim() ? '#fff' : 'var(--textDim)',
                border: 'none',
                cursor: customInput.trim() ? 'pointer' : 'default',
              }}
            >
              Send
            </button>
          </div>
        )}

        {answered && (
          <div className="text-[10px] mt-0.5" style={{ color: 'var(--textDim)' }}>
            Answered: {selected}
          </div>
        )}
      </div>
    </div>
  )
}
