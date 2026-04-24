import { useState, useEffect, useRef } from 'react'
import { BrowserToolbar } from './BrowserToolbar'
import { useUIStore } from '../../store/uiStore'

export function BrowserPanel() {
  const [url, setUrl] = useState('https://www.google.com')
  const [title, setTitle] = useState('')
  const containerRef = useRef<HTMLDivElement>(null)
  const { toggleBrowser } = useUIStore()

  useEffect(() => {
    const unsubUrl = window.athena.browser.onUrlChange(setUrl)
    const unsubTitle = window.athena.browser.onTitleChange(setTitle)
    return () => { unsubUrl(); unsubTitle() }
  }, [])

  useEffect(() => {
    if (!containerRef.current) return

    const updateBounds = () => {
      if (!containerRef.current) return
      const rect = containerRef.current.getBoundingClientRect()
      window.athena.browser.show({
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      })
    }

    updateBounds()

    const ro = new ResizeObserver(updateBounds)
    ro.observe(containerRef.current)

    return () => {
      ro.disconnect()
      window.athena.browser.hide()
    }
  }, [])

  return (
    <div className="flex flex-col h-full border-l" style={{ borderColor: 'var(--border)' }}>
      <BrowserToolbar
        url={url}
        title={title}
        onNavigate={(u) => window.athena.browser.navigate(u)}
        onBack={() => window.athena.browser.back()}
        onForward={() => window.athena.browser.forward()}
        onReload={() => window.athena.browser.reload()}
        onClose={toggleBrowser}
      />
      <div ref={containerRef} className="flex-1 min-h-0" style={{ background: '#fff' }} />
    </div>
  )
}
