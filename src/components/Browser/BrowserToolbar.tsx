import { useState, useEffect, useRef, useCallback } from 'react'
import { ArrowLeft, ArrowRight, RotateCw, ExternalLink, X, Globe } from 'lucide-react'

interface BrowserToolbarProps {
  url: string
  title: string
  onNavigate: (url: string) => void
  onBack: () => void
  onForward: () => void
  onReload: () => void
  onClose: () => void
}

export function BrowserToolbar({
  url,
  title,
  onNavigate,
  onBack,
  onForward,
  onReload,
  onClose,
}: BrowserToolbarProps) {
  const [inputUrl, setInputUrl] = useState(url)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    setInputUrl(url)
  }, [url])

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (inputUrl.trim()) onNavigate(inputUrl.trim())
    inputRef.current?.blur()
  }

  return (
    <div
      className="flex items-center gap-1.5 px-2 shrink-0 border-b"
      style={{ height: 36, borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
    >
      <button onClick={onBack} className="p-1 rounded hover:bg-white/10 transition-colors">
        <ArrowLeft size={14} style={{ color: 'var(--textMuted)' }} />
      </button>
      <button onClick={onForward} className="p-1 rounded hover:bg-white/10 transition-colors">
        <ArrowRight size={14} style={{ color: 'var(--textMuted)' }} />
      </button>
      <button onClick={onReload} className="p-1 rounded hover:bg-white/10 transition-colors">
        <RotateCw size={13} style={{ color: 'var(--textMuted)' }} />
      </button>

      <form onSubmit={handleSubmit} className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md" style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}>
          <Globe size={12} style={{ color: 'var(--textDim)', flexShrink: 0 }} />
          <input
            ref={inputRef}
            value={inputUrl}
            onChange={(e) => setInputUrl(e.target.value)}
            className="flex-1 bg-transparent text-[11px] outline-none min-w-0"
            style={{ color: 'var(--text)' }}
            placeholder="Enter URL..."
            onFocus={(e) => e.target.select()}
          />
        </div>
      </form>

      <button
        onClick={() => {
          if (url) {
            const w = window.open(url, '_blank')
            w?.focus()
          }
        }}
        className="p-1 rounded hover:bg-white/10 transition-colors"
        title="Open in system browser"
      >
        <ExternalLink size={13} style={{ color: 'var(--textMuted)' }} />
      </button>
      <button onClick={onClose} className="p-1 rounded hover:bg-white/10 transition-colors">
        <X size={14} style={{ color: 'var(--textMuted)' }} />
      </button>
    </div>
  )
}
