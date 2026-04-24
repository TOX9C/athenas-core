import { useRef, useEffect, useCallback } from 'react'
import Editor from '@monaco-editor/react'
import { useEditorStore } from '../../store/editorStore'
import { useUIStore } from '../../store/uiStore'
import { EditorTabs } from './EditorTabs'
import { themes } from '../../themes/themes'
import { FileText } from 'lucide-react'

function detectLanguage(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript',
    json: 'json', md: 'markdown', css: 'css', scss: 'scss', html: 'html',
    py: 'python', go: 'go', rs: 'rust', rb: 'ruby', sh: 'shell',
    yml: 'yaml', yaml: 'yaml', toml: 'toml', sql: 'sql', xml: 'xml',
    svg: 'xml', graphql: 'graphql', dockerfile: 'dockerfile',
  }
  return map[ext] ?? 'plaintext'
}

export function EditorPanel() {
  const { openFiles, activeFilePath, updateFile, closeFile, setActiveFile } = useEditorStore()
  const { fontSize, fontFamily, theme } = useUIStore()
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

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
          // auto-save failed silently
        }
      }, 1000)
    },
    [activeFilePath, updateFile]
  )

  useEffect(() => {
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
    }
  }, [])

  return (
    <div
      className="flex flex-col h-full border-l"
      style={{ borderColor: 'var(--border)', background: 'var(--bg)' }}
    >
      <EditorTabs onClose={closeFile} />

      {activeFile ? (
        <div className="flex-1 min-h-0">
          <Editor
            key={activeFile.path}
            language={activeFile.language}
            value={activeFile.content}
            onChange={handleChange}
            theme={isDark ? 'vs-dark' : 'vs'}
            options={{
              fontSize,
              fontFamily,
              minimap: { enabled: false },
              lineNumbers: 'on',
              scrollBeyondLastLine: false,
              wordWrap: 'on',
              automaticLayout: true,
              padding: { top: 8 },
              renderLineHighlight: 'gutter',
              scrollbar: { verticalScrollbarSize: 4, horizontalScrollbarSize: 4 },
              overviewRulerLanes: 0,
              hideCursorInOverviewRuler: true,
              overviewRulerBorder: false,
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
