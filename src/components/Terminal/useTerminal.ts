import { useEffect, useRef, useCallback } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import '@xterm/xterm/css/xterm.css'
import { useUIStore } from '../../store/uiStore'
import { themes } from '../../themes/themes'

interface UseTerminalOptions {
  paneId: string
  cwd: string
  agentCmd?: string
}

export function useTerminal({ paneId, cwd, agentCmd }: UseTerminalOptions, containerRef: React.RefObject<HTMLDivElement | null>) {
  const termRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const spawnedRef = useRef(false)
  const theme = useUIStore((s) => s.theme)
  const fontSize = useUIStore((s) => s.fontSize)
  const fontFamily = useUIStore((s) => s.fontFamily)

  const fit = useCallback(() => {
    if (fitAddonRef.current && termRef.current) {
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

    try {
      const webglAddon = new WebglAddon()
      webglAddon.onContextLoss(() => webglAddon.dispose())
      term.loadAddon(webglAddon)
    } catch {
      // WebGL not available, fall back to canvas
    }

    fitAddon.fit()

    termRef.current = term
    fitAddonRef.current = fitAddon

    window.athena.pty.getHistory(paneId).then((hist) => {
      if (hist) term.write(hist)
    })

    const unsubData = window.athena.pty.onData(paneId, (data) => {
      term.write(data)
    })

    term.onData((data) => {
      window.athena.pty.write(paneId, data)
    })

    if (!spawnedRef.current) {
      spawnedRef.current = true
      window.athena.pty.hasSession(paneId).then((exists) => {
        if (!exists) {
          window.athena.pty.spawn(paneId, cwd, '', agentCmd || undefined)
            .then((res) => {
              if (res && !res.success) {
                spawnedRef.current = false
              }
            })
            .catch(() => {
              spawnedRef.current = false
            })
        }
      }).catch(() => {
        spawnedRef.current = false
      })
    }

    const ro = new ResizeObserver(() => {
      requestAnimationFrame(() => fit())
    })
    ro.observe(containerRef.current)

    return () => {
      ro.disconnect()
      unsubData()
      term.dispose()
      termRef.current = null
      fitAddonRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paneId, cwd, agentCmd, containerRef, fit])

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

  return { terminal: termRef, fit }
}
