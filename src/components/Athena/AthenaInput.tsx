import { useState, useRef, KeyboardEvent } from 'react'
import { ArrowUp } from 'lucide-react'

interface AthenaInputProps {
  onSend: (text: string) => void
  disabled?: boolean
}

export function AthenaInput({ onSend, disabled }: AthenaInputProps) {
  const [text, setText] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const handleSend = () => {
    if (!text.trim() || disabled) return
    onSend(text)
    setText('')
    if (textareaRef.current) textareaRef.current.style.height = 'auto'
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const handleInput = () => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = Math.min(el.scrollHeight, 120) + 'px'
  }

  return (
    <div
      className="flex items-end gap-2 p-3 border-t"
      style={{ borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
    >
      <textarea
        ref={textareaRef}
        value={text}
        onChange={(e) => { setText(e.target.value); handleInput() }}
        onKeyDown={handleKeyDown}
        placeholder={disabled ? 'Athena is starting...' : 'Ask Athena...'}
        disabled={disabled}
        rows={1}
        className="flex-1 bg-transparent text-xs outline-none resize-none leading-relaxed disabled:opacity-40"
        style={{ color: 'var(--text)', maxHeight: 120 }}
      />
      <button
        onClick={handleSend}
        disabled={disabled || !text.trim()}
        className="p-1.5 rounded-md transition-colors disabled:opacity-30 shrink-0"
        style={{ background: 'var(--accent)', color: '#fff' }}
      >
        <ArrowUp size={14} />
      </button>
    </div>
  )
}
