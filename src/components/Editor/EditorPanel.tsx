import { useRef, useEffect, useCallback } from 'react'
import Editor, { type OnMount } from '@monaco-editor/react'
import type { editor } from 'monaco-editor'
import { useEditorStore } from '../../store/editorStore'
import { useUIStore } from '../../store/uiStore'
import { EditorTabs } from './EditorTabs'
import { themes } from '../../themes/themes'
import { FileText } from 'lucide-react'

export function EditorPanel() {
  const { openFiles, activeFilePath, updateFile, closeFile } = useEditorStore()
  const { fontSize, fontFamily, theme } = useUIStore()
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  const activeFile = openFiles.find((f) => f.path === activeFilePath)
  const themeColors = themes[theme]
  const isDark = themeColors?.type === 'dark'

  const handleChange = useCallback(
    (value: string | undefined) => {
      if (!activeFilePath || value === undefined) return
      updateFile(activeFilePath, { content: value, isDirty: true })

      if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
      saveTimerRef.current = setTimeout(async () => {
        try {
          await window.athena.fs.writeFile(activeFilePath, value)
          updateFile(activeFilePath, { isDirty: false })
        } catch {
          // auto-save failed
        }
      }, 1000)
    },
    [activeFilePath, updateFile],
  )

  const handleMount: OnMount = (editor) => {
    editorRef.current = editor
    editor.layout()
  }

  useEffect(() => {
    const container = containerRef.current
    const ed = editorRef.current
    if (!container || !ed) return

    let timeout: ReturnType<typeof setTimeout>
    const ro = new ResizeObserver(() => {
      clearTimeout(timeout)
      timeout = setTimeout(() => {
        if (editorRef.current) {
          editorRef.current.layout()
        }
      }, 50)
    })
    ro.observe(container)
    return () => {
      clearTimeout(timeout)
      ro.disconnect()
    }
  }, [activeFilePath])

  useEffect(() => {
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
    }
  }, [])

  return (
    <div
      className="flex flex-col h-full border-l overflow-hidden"
      style={{ borderColor: 'var(--border)', background: 'var(--bg)' }}
    >
      <EditorTabs onClose={closeFile} />

      {activeFile ? (
        <div ref={containerRef} className="flex-1 min-h-0 relative overflow-hidden">
          <Editor
            key={activeFile.path}
            language={activeFile.language}
            value={activeFile.content}
            onChange={handleChange}
            onMount={handleMount}
            theme={isDark ? 'vs-dark' : 'vs'}
            loading={null}
            options={{
              fontSize,
              fontFamily,
              readOnly: true,
              minimap: { enabled: false },
              lineNumbers: 'on',
              scrollBeyondLastLine: false,
              wordWrap: 'on',
              automaticLayout: false,
              padding: { top: 8 },
              renderLineHighlight: 'none',
              scrollbar: { verticalScrollbarSize: 4, horizontalScrollbarSize: 4 },
              overviewRulerLanes: 0,
              hideCursorInOverviewRuler: true,
              overviewRulerBorder: false,
              domReadOnly: true,
              cursorStyle: 'line-thin',
            }}
          />
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <div className="flex flex-col items-center gap-2" style={{ color: 'var(--textDim)' }}>
            <FileText size={32} style={{ opacity: 0.3 }} />
            <span className="text-xs">Open a file to edit</span>
          </div>
        </div>
      )}
    </div>
  )
}
