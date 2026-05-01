cat << 'INNER_EOF' > /tmp/AthenaInput.tsx
import { useState, useRef, KeyboardEvent, useEffect } from 'react'
import { ArrowUp } from 'lucide-react'

interface AthenaInputProps {
  onSend: (text: string) => void
  disabled?: boolean
}

export function AthenaInput({ onSend, disabled }: AthenaInputProps) {
  const [text, setText] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  // History state
  const [history, setHistory] = useState<string[]>([])
  const [historyIndex, setHistoryIndex] = useState<number | null>(null)
  const [draft, setDraft] = useState<string>('')

  // Load history from localStorage on mount
  useEffect(() => {
    try {
      const stored = localStorage.getItem('athena-input-history')
      if (stored) {
        setHistory(JSON.parse(stored))
      }
    } catch (err) {
      console.error('Failed to load history', err)
    }
  }, [])

  // Persist history to localStorage
  const saveHistory = (newHistory: string[]) => {
    setHistory(newHistory)
    try {
      localStorage.setItem('athena-input-history', JSON.stringify(newHistory))
    } catch (err) {
      console.error('Failed to save history', err)
    }
  }

  const handleSend = () => {
    if (!text.trim() || disabled) return
    onSend(text)
    
    // Append to history
    const newHistory = [...history, text]
    // Cap at 100
    if (newHistory.length > 100) {
      newHistory.shift()
    }
    saveHistory(newHistory)
    
    // Reset state
    setText('')
    setHistoryIndex(null)
    setDraft('')
    
    if (textareaRef.current) textareaRef.current.style.height = 'auto'
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    } else if (e.key === 'ArrowUp') {
      if (history.length === 0) return
      
      e.preventDefault()
      if (historyIndex === null) {
        // First time querying history
        setDraft(text)
        const newIndex = history.length - 1
        setHistoryIndex(newIndex)
        setText(history[newIndex])
      } else if (historyIndex > 0) {
        // Go older
        const newIndex = historyIndex - 1
        setHistoryIndex(newIndex)
        setText(history[newIndex])
      }
    } else if (e.key === 'ArrowDown') {
      if (historyIndex === null) return
      
      e.preventDefault()
      if (historyIndex + 1 >= history.length) {
        // Reached the present
        setHistoryIndex(null)
        setText(draft)
      } else {
        // Go newer
        const newIndex = historyIndex + 1
        setHistoryIndex(newIndex)
        setText(history[newIndex])
      }
    }
  }

  const handleInput = () => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = Math.min(el.scrollHeight, 120) + 'px'
  }

  // When manually typing, reset history index
  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value)
    if (historyIndex !== null) {
      setHistoryIndex(null) // Exit history mode
      setDraft('') // Clear draft since they are typing something new
    }
    handleInput()
  }

  return (
    <div
      className="flex items-end gap-2 p-3 border-t"
      style={{ borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
    >
      <textarea
        ref={textareaRef}
        value={text}
        onChange={handleChange}
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
INNER_EOF
mv /tmp/AthenaInput.tsx src/components/Athena/AthenaInput.tsx
