import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { Search, ChevronRight, Hash } from 'lucide-react'
import { useCommandStore, selectFilteredCommands } from '../../store/commandStore'

function formatShortcut(shortcut: string | undefined): string {
  if (!shortcut) return ''
  return shortcut
    .replace('Mod', '\u2318')
    .replace('Cmd', '\u2318')
    .replace('Ctrl', '\u2303')
    .replace('Shift', '\u21e7')
    .replace('Alt', '\u2325')
    .replace('Enter', '\u23ce')
    .replace('Escape', '\u238b')
    .replace('Backspace', '\u232b')
    .replace('Tab', '\u21e5')
}

export function CommandPalette() {
  const isOpen = useCommandStore((s) => s.isOpen)
  const close = useCommandStore((s) => s.close)
  const executeCommand = useCommandStore((s) => s.executeCommand)

  if (!isOpen) return null

  return (
    <div
      className="fixed inset-0 z-[60] flex justify-center pt-[12vh] command-palette-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) close()
      }}
    >
      <PaletteInner onExecute={executeCommand} onClose={close} />
    </div>
  )
}

function PaletteInner({
  onExecute,
  onClose,
}: {
  onExecute: (id: string) => void
  onClose: () => void
}) {
  const [query, setQuery] = useState('')
  const [selectedIdx, setSelectedIdx] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const commands = useCommandStore((s) => s.commands)
  const recentIds = useCommandStore((s) => s.recentIds)

  const groups = useMemo(
    () => selectFilteredCommands(commands, recentIds, query),
    [commands, recentIds, query],
  )

  const flatCommands = useMemo(() => groups.flatMap((g) => g.commands), [groups])
  const totalCount = commands.length

  useEffect(() => {
    setSelectedIdx(0)
  }, [query])

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => {
    if (!flatCommands[selectedIdx]) return
    const el = listRef.current?.querySelector(
      `[data-command-idx="${selectedIdx}"]`,
    ) as HTMLElement | null
    el?.scrollIntoView({ block: 'nearest' })
  }, [selectedIdx, flatCommands.length])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setSelectedIdx((i) => Math.min(i + 1, flatCommands.length - 1))
      } else if (e.key === 'ArrowUp') {
        e.preventDefault()
        setSelectedIdx((i) => Math.max(i - 1, 0))
      } else if (e.key === 'Enter') {
        e.preventDefault()
        const cmd = flatCommands[selectedIdx]
        if (cmd) onExecute(cmd.id)
      } else if (e.key === 'Escape') {
        e.preventDefault()
        onClose()
      }
    },
    [flatCommands, selectedIdx, onExecute, onClose],
  )

  let runningIdx = 0

  return (
    <div className="command-palette-container" onKeyDown={handleKeyDown}>
      <div className="command-palette-header">
        <Search size={15} style={{ color: 'var(--textDim)', flexShrink: 0 }} />
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Type a command..."
          className="command-palette-input"
          spellCheck={false}
          autoComplete="off"
        />
        <div className="command-palette-header-right">
          {query.trim() && flatCommands.length > 0 && (
            <span className="command-palette-count">{flatCommands.length}</span>
          )}
          <kbd className="command-palette-kbd-sm">esc</kbd>
        </div>
      </div>

      <div ref={listRef} className="command-palette-list">
        {flatCommands.length === 0 ? (
          <div className="command-palette-empty">
            <Hash size={20} style={{ opacity: 0.3 }} />
            <span className="text-xs">
              {query.trim() ? 'No matching commands' : `${totalCount} commands available`}
            </span>
          </div>
        ) : (
          groups.map((group) => {
            const groupStart = runningIdx
            const groupItems = group.commands.map((cmd, gi) => {
              const idx = groupStart + gi
              const isSelected = idx === selectedIdx
              const Icon = cmd.icon
              runningIdx = idx + 1

              return (
                <button
                  key={cmd.id}
                  data-command-idx={idx}
                  onClick={() => onExecute(cmd.id)}
                  onMouseEnter={() => setSelectedIdx(idx)}
                  className="command-palette-item"
                  style={{
                    background: isSelected ? 'var(--bgTertiary)' : 'transparent',
                  }}
                >
                  <span className="command-palette-item-icon">
                    {Icon ? (
                      <Icon size={14} style={{ color: 'var(--textDim)' }} />
                    ) : (
                      <ChevronRight size={12} style={{ color: 'var(--textDim)' }} />
                    )}
                  </span>
                  <span className="command-palette-item-label">{cmd.label}</span>
                  {cmd.shortcut && (
                    <kbd
                      className="command-palette-item-kbd"
                      style={{
                        background: isSelected ? 'var(--bgSecondary)' : 'var(--bgTertiary)',
                      }}
                    >
                      {formatShortcut(cmd.shortcut)}
                    </kbd>
                  )}
                </button>
              )
            })

            return (
              <div key={group.label} className="command-palette-group">
                <div className="command-palette-group-label">{group.label}</div>
                {groupItems}
              </div>
            )
          })
        )}
      </div>

      <div className="command-palette-footer">
        <span>
          <kbd className="command-palette-kbd-sm">↑↓</kbd> navigate
        </span>
        <span>
          <kbd className="command-palette-kbd-sm">↵</kbd> execute
        </span>
        <span>
          <kbd className="command-palette-kbd-sm">esc</kbd> close
        </span>
        <span className="ml-auto" style={{ opacity: 0.5 }}>
          {totalCount} commands
        </span>
      </div>
    </div>
  )
}
