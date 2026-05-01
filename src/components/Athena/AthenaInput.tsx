import { useState, useRef, KeyboardEvent, useEffect, useCallback } from 'react'
import { ArrowUp, ImagePlus, X } from 'lucide-react'
import { nanoid } from 'nanoid'
import type { ImageAttachment } from '../../store/athenaStore'

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'])
const MAX_ATTACHMENTS = 5
const VALID_MEDIA_TYPES: ImageAttachment['mediaType'][] = [
  'image/jpeg',
  'image/png',
  'image/gif',
  'image/webp',
]

interface AthenaInputProps {
  onSend: (text: string, attachments?: ImageAttachment[]) => void
  disabled?: boolean
}

function isImageFile(file: File): boolean {
  const ext = file.name.split('.').pop()?.toLowerCase() || ''
  return IMAGE_EXTENSIONS.has(ext) || file.type.startsWith('image/')
}

function getMediaType(file: File): ImageAttachment['mediaType'] {
  if (VALID_MEDIA_TYPES.includes(file.type as ImageAttachment['mediaType']))
    return file.type as ImageAttachment['mediaType']
  const ext = file.name.split('.').pop()?.toLowerCase() || 'png'
  const map: Record<string, ImageAttachment['mediaType']> = {
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    webp: 'image/webp',
  }
  return map[ext] || 'image/png'
}

function fileToAttachment(file: File): Promise<ImageAttachment | null> {
  return new Promise((resolve) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = reader.result as string
      const base64 = result.split(',')[1] || ''
      if (!base64) {
        console.error('[AthenaInput] FileReader produced empty base64 for', file.name)
        resolve(null)
        return
      }
      resolve({ id: nanoid(), base64, mediaType: getMediaType(file), name: file.name })
    }
    reader.onerror = () => {
      console.error('[AthenaInput] FileReader failed for', file.name)
      resolve(null)
    }
    reader.readAsDataURL(file)
  })
}

function previewUrl(att: ImageAttachment): string {
  return `data:${att.mediaType};base64,${att.base64}`
}

export function AthenaInput({ onSend, disabled }: AthenaInputProps) {
  const [text, setText] = useState('')
  const [attachments, setAttachments] = useState<ImageAttachment[]>([])
  const [isDragOver, setIsDragOver] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const dragCounterRef = useRef(0)
  const [history, setHistory] = useState<string[]>([])
  const [historyIndex, setHistoryIndex] = useState<number | null>(null)
  const [draft, setDraft] = useState<string>('')

  useEffect(() => {
    try {
      const stored = localStorage.getItem('athena-input-history')
      if (stored) setHistory(JSON.parse(stored))
    } catch {}
  }, [])

  useEffect(() => {
    const onDragEnd = () => {
      dragCounterRef.current = 0
      setIsDragOver(false)
    }
    window.addEventListener('dragend', onDragEnd)
    return () => window.removeEventListener('dragend', onDragEnd)
  }, [])

  const saveHistory = (newHistory: string[]) => {
    setHistory(newHistory)
    try {
      localStorage.setItem('athena-input-history', JSON.stringify(newHistory))
    } catch {}
  }

  const addFiles = useCallback(
    async (files: File[]) => {
      const imageFiles = files.filter(isImageFile)
      if (imageFiles.length === 0) return
      const remaining = MAX_ATTACHMENTS - attachments.length
      const toAdd = imageFiles.slice(0, remaining)
      if (toAdd.length === 0) return
      const results = await Promise.all(toAdd.map(fileToAttachment))
      const newAtts = results.filter((a): a is ImageAttachment => a !== null)
      if (newAtts.length === 0) return
      setAttachments((prev) => [...prev, ...newAtts])
    },
    [attachments.length],
  )

  const removeAttachment = useCallback((id: string) => {
    setAttachments((prev) => prev.filter((a) => a.id !== id))
  }, [])

  const handleSend = () => {
    const hasText = text.trim().length > 0
    const hasAttachments = attachments.length > 0
    if ((!hasText && !hasAttachments) || disabled) return
    onSend(text.trim(), hasAttachments ? attachments : undefined)
    const newHistory = [...history, text]
    if (newHistory.length > 100) newHistory.shift()
    saveHistory(newHistory)
    setText('')
    setAttachments([])
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
        setDraft(text)
        const newIndex = history.length - 1
        setHistoryIndex(newIndex)
        setText(history[newIndex])
      } else if (historyIndex > 0) {
        const newIndex = historyIndex - 1
        setHistoryIndex(newIndex)
        setText(history[newIndex])
      }
    } else if (e.key === 'ArrowDown') {
      if (historyIndex === null) return
      e.preventDefault()
      if (historyIndex + 1 >= history.length) {
        setHistoryIndex(null)
        setText(draft)
      } else {
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

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value)
    if (historyIndex !== null) {
      setHistoryIndex(null)
      setDraft('')
    }
    handleInput()
  }

  const handleImagePick = async () => {
    if (disabled) return
    try {
      const filePaths = await window.athena.fs.showImageDialog()
      if (!filePaths || filePaths.length === 0) return
      const remaining = MAX_ATTACHMENTS - attachments.length
      const toProcess = filePaths.slice(0, remaining)
      if (toProcess.length === 0) return
      const newAtts: ImageAttachment[] = (
        await Promise.all(
          toProcess.map(async (filePath) => {
            const result = await window.athena.fs.readFileAsBase64(filePath)
            if (!result.data) return null
            const name = filePath.split('/').pop() || filePath
            const mediaType = (
              VALID_MEDIA_TYPES.includes(result.mediaType as ImageAttachment['mediaType'])
                ? result.mediaType
                : 'image/png'
            ) as ImageAttachment['mediaType']
            return { id: nanoid(), base64: result.data, mediaType, name }
          }),
        )
      ).filter((a): a is ImageAttachment => a !== null)
      if (newAtts.length > 0) setAttachments((prev) => [...prev, ...newAtts])
    } catch (err) {
      console.error('Image pick failed:', err)
    }
  }

  const handlePaste = useCallback(
    async (e: React.ClipboardEvent) => {
      const items = e.clipboardData?.items
      if (!items) return
      const imageFiles: File[] = []
      for (let i = 0; i < items.length; i++) {
        const item = items[i]
        if (item.type.startsWith('image/')) {
          const file = item.getAsFile()
          if (file) imageFiles.push(file)
        }
      }
      if (imageFiles.length > 0) {
        e.preventDefault()
        await addFiles(imageFiles)
      }
    },
    [addFiles],
  )

  const handleDragEnter = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounterRef.current++
    if (dragCounterRef.current === 1) {
      setIsDragOver(true)
    }
  }, [])

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounterRef.current--
    if (dragCounterRef.current <= 0) {
      dragCounterRef.current = 0
      setIsDragOver(false)
    }
  }, [])

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    e.dataTransfer.dropEffect = 'copy'
  }, [])

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      dragCounterRef.current = 0
      setIsDragOver(false)
      const files = Array.from(e.dataTransfer.files)
      if (files.length > 0) await addFiles(files)
    },
    [addFiles],
  )

  const canSend = !disabled && (text.trim().length > 0 || attachments.length > 0)

  return (
    <div
      className="flex flex-col relative"
      style={{ borderTop: '1px solid var(--border)', background: 'var(--bgSecondary)' }}
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
            pointerEvents: 'auto',
          }}
          onDragEnter={(e) => {
            e.preventDefault()
            e.stopPropagation()
          }}
          onDragLeave={(e) => {
            e.preventDefault()
            e.stopPropagation()
          }}
          onDragOver={(e) => {
            e.preventDefault()
            e.stopPropagation()
            e.dataTransfer.dropEffect = 'copy'
          }}
          onDrop={handleDrop}
        >
          <div className="flex items-center gap-2 pointer-events-none">
            <ImagePlus size={20} style={{ color: 'var(--accent)' }} />
            <span className="text-xs font-medium" style={{ color: 'var(--accent)' }}>
              Drop images here
            </span>
          </div>
        </div>
      )}

      {attachments.length > 0 && (
        <div
          className="flex gap-1.5 p-2 overflow-x-auto"
          style={{ borderBottom: '1px solid color-mix(in srgb, var(--border) 50%, transparent)' }}
        >
          {attachments.map((att) => (
            <div
              key={att.id}
              className="shrink-0 relative group rounded overflow-hidden"
              style={{
                width: 56,
                height: 56,
                background: 'var(--bgTertiary)',
                border: '1px solid var(--border)',
              }}
            >
              {att.base64 ? (
                <img
                  src={previewUrl(att)}
                  alt={att.name || 'Image'}
                  className="w-full h-full object-cover"
                  draggable={false}
                />
              ) : (
                <div className="w-full h-full flex items-center justify-center">
                  <ImagePlus size={14} style={{ color: 'var(--textDim)' }} />
                </div>
              )}
              <button
                onClick={() => removeAttachment(att.id)}
                className="absolute top-0.5 right-0.5 p-0.5 rounded-full opacity-0 group-hover:opacity-100 transition-opacity"
                style={{ background: 'rgba(0,0,0,0.75)', color: '#fff', lineHeight: 0 }}
              >
                <X size={9} />
              </button>
              {att.name && (
                <div
                  className="absolute bottom-0 left-0 right-0 px-1 py-px truncate text-[7px]"
                  style={{ background: 'rgba(0,0,0,0.65)', color: '#c4c4c7' }}
                >
                  {att.name}
                </div>
              )}
            </div>
          ))}
          {attachments.length < MAX_ATTACHMENTS && (
            <button
              onClick={handleImagePick}
              disabled={disabled}
              className="shrink-0 flex items-center justify-center rounded transition-colors disabled:opacity-30"
              style={{
                width: 56,
                height: 56,
                border: '1px dashed var(--border)',
                background: 'transparent',
              }}
            >
              <ImagePlus size={14} style={{ color: 'var(--textDim)' }} />
            </button>
          )}
        </div>
      )}

      <div className="flex items-end gap-1.5 px-3 py-2">
        <textarea
          ref={textareaRef}
          value={text}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder={disabled ? 'Athena is busy...' : 'Ask Athena...'}
          disabled={disabled}
          rows={1}
          className="flex-1 bg-transparent text-xs outline-none resize-none leading-relaxed disabled:opacity-40 placeholder:opacity-40"
          style={{ color: 'var(--text)', maxHeight: 120 }}
        />
        <button
          onClick={handleImagePick}
          disabled={disabled || attachments.length >= MAX_ATTACHMENTS}
          className="p-1.5 rounded-md transition-colors disabled:opacity-20 shrink-0 hover:bg-white/5"
          style={{ color: 'var(--textDim)' }}
          title="Attach image"
        >
          <ImagePlus size={13} />
        </button>
        <button
          onClick={handleSend}
          disabled={!canSend}
          className="p-1.5 rounded-md transition-all disabled:opacity-20 shrink-0"
          style={{ background: canSend ? 'var(--accent)' : 'var(--bgTertiary)', color: '#fff' }}
        >
          <ArrowUp size={13} />
        </button>
      </div>
    </div>
  )
}
