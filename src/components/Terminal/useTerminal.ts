import { useEffect, useRef, useCallback } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import '@xterm/xterm/css/xterm.css'
import { useUIStore } from '../../store/uiStore'
import { useTerminalStore } from '../../store/terminalStore'
import { themes } from '../../themes/themes'

interface UseTerminalOptions {
  paneId: string
  cwd: string
  agentCmd?: string
}

export function useTerminal(
  { paneId, cwd, agentCmd }: UseTerminalOptions,
  containerRef: React.RefObject<HTMLDivElement | null>,
) {
  const termRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const spawnedRef = useRef(false)
  const lastSizeRef = useRef<{ w: number; h: number }>({ w: 0, h: 0 })
  const webglAddonRef = useRef<WebglAddon | null>(null)
  const theme = useUIStore((s) => s.theme)
  const fontSize = useUIStore((s) => s.fontSize)
  const fontFamily = useUIStore((s) => s.fontFamily)
  const handleShellEvent = useTerminalStore((s) => s.handleShellIntegrationEvent)

  const fit = useCallback(() => {
    if (fitAddonRef.current && termRef.current) {
      // Xterm.js hack: check if the DOM is actually attached before trying to fit
      // to avoid Uncaught TypeError on 'dimensions'
      if (!termRef.current.element || !termRef.current.element.parentElement) return
      try {
        fitAddonRef.current.fit()
        const { cols, rows } = termRef.current
        window.athena.pty.resize(paneId, cols, rows)
      } catch {
        // ignore fit errors during teardown
      }
    }
  }, [paneId])

  useEffect(() => {
    if (!containerRef.current) return

    const themeColors = themes[theme]?.colors
    const term = new Terminal({
      fontSize,
      fontFamily,
      cursorBlink: true,
      cursorStyle: 'bar',
      allowProposedApi: true,
      theme: themeColors
        ? {
            background: themeColors.terminalBg,
            foreground: themeColors.terminalFg,
            cursor: themeColors.terminalCursor,
            selectionBackground: themeColors.terminalSelection,
          }
        : undefined,
    })

    const fitAddon = new FitAddon()
    term.loadAddon(fitAddon)

    term.open(containerRef.current)

    function tryLoadWebgl(t: Terminal): void {
      try {
        const addon = new WebglAddon()
        addon.onContextLoss(() => {
          addon.dispose()
          webglAddonRef.current = null
          // Attempt recovery after a short delay
          setTimeout(() => {
            if (termRef.current) {
              tryLoadWebgl(termRef.current)
            }
          }, 1000)
        })
        t.loadAddon(addon)
        webglAddonRef.current = addon
      } catch {
        // WebGL not available or all contexts exhausted; xterm falls back to canvas renderer
        webglAddonRef.current = null
      }
    }
    tryLoadWebgl(term)

    fitAddon.fit()

    termRef.current = term
    fitAddonRef.current = fitAddon

    window.athena.pty.getHistory(paneId).then((hist) => {
      if (hist) term.write(hist)
    })

    const unsubData = window.athena.pty.onData(paneId, (data) => {
      term.write(data)
    })

    const unsubShell = window.athena.pty.onShellIntegration(paneId, (event) => {
      handleShellEvent(event)
    })

    const unsubCwd = window.athena.pty.onCwdChanged((data) => {
      if (data.paneId === paneId) {
        handleShellEvent({
          type: 'cwd',
          paneId: data.paneId,
          cwd: data.cwd,
          timestamp: data.timestamp,
        })
      }
    })

    const unsubCmdStart = window.athena.pty.onCommandStarted((data) => {
      if (data.paneId === paneId) {
        handleShellEvent({
          type: 'commandStart',
          paneId: data.paneId,
          command: data.command,
          cwd: data.cwd,
          timestamp: data.timestamp,
        })
      }
    })

    const unsubCmdExit = window.athena.pty.onCommandExited((data) => {
      if (data.paneId === paneId) {
        handleShellEvent({
          type: 'commandFinished',
          paneId: data.paneId,
          command: data.command,
          exitCode: data.exitCode,
          cwd: data.cwd,
          duration: data.duration,
          timestamp: data.timestamp,
        })
      }
    })

    term.onData((data) => {
      window.athena.pty.write(paneId, data)
    })

    if (!spawnedRef.current) {
      spawnedRef.current = true
      window.athena.pty
        .hasSession(paneId)
        .then((exists) => {
          if (!exists) {
            window.athena.pty
              .spawn(paneId, cwd, '', agentCmd || undefined)
              .then((res) => {
                if (res && !res.success) {
                  spawnedRef.current = false
                }
              })
              .catch(() => {
                spawnedRef.current = false
              })
          }
        })
        .catch(() => {
          spawnedRef.current = false
        })
    }

    let timeout: ReturnType<typeof setTimeout> | null = null
    const ro = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (!entry) return
      const { width, height } = entry.contentRect
      const prev = lastSizeRef.current
      // Use < 1 on both independently so that if *either* is large it triggers,
      // but if *both* are < 1 difference, it doesn't.
      if (Math.abs(width - prev.w) < 1 && Math.abs(height - prev.h) < 1) return
      console.log(
        `[ResizeObserver] Resizing pane ${paneId} from ${prev.w}x${prev.h} to ${width}x${height}`,
      )
      lastSizeRef.current = { w: width, h: height }
      if (timeout) clearTimeout(timeout)
      timeout = setTimeout(() => {
        fit()
      }, 150)
    })
    ro.observe(containerRef.current)

    return () => {
      ro.disconnect()
      if (timeout) clearTimeout(timeout)
      lastSizeRef.current = { w: 0, h: 0 }
      unsubData()
      unsubShell()
      unsubCwd()
      unsubCmdStart()
      unsubCmdExit()
      if (webglAddonRef.current) {
        webglAddonRef.current.dispose()
        webglAddonRef.current = null
      }
      term.dispose()
      termRef.current = null
      fitAddonRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paneId, cwd, agentCmd, containerRef, fit, handleShellEvent])

  // Update theme and font without recreating the terminal
  useEffect(() => {
    if (!termRef.current) return

    const themeColors = themes[theme]?.colors
    termRef.current.options.fontSize = fontSize
    termRef.current.options.fontFamily = fontFamily

    if (themeColors) {
      termRef.current.options.theme = {
        background: themeColors.terminalBg,
        foreground: themeColors.terminalFg,
        cursor: themeColors.terminalCursor,
        selectionBackground: themeColors.terminalSelection,
      }
    }

    // Fit again in case font size caused geometry changes
    fit()
  }, [theme, fontSize, fontFamily, fit])

  // Listen for theme-change custom events from the document to sync
  // the xterm background with CSS variable overrides dynamically.
  useEffect(() => {
    const handleThemeChange = (event: Event) => {
      const customEvent = event as CustomEvent<{ background?: string }>
      const newBg =
        customEvent.detail?.background ??
        getComputedStyle(document.documentElement).getPropertyValue('--panel-bg').trim()

      if (newBg && termRef.current) {
        termRef.current.options.theme = {
          ...termRef.current.options.theme,
          background: newBg,
        }
      }
    }

    document.addEventListener('theme-change', handleThemeChange)
    return () => document.removeEventListener('theme-change', handleThemeChange)
  }, [])

  return { terminal: termRef, fit }
}
