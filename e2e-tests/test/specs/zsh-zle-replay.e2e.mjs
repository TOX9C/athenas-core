// Regression spec for the zsh zle redraw "ghosting" bug: typing "ca",
// pressing ArrowUp (history recall through zsh-syntax-highlighting), deleting
// and recalling again left stale glyphs painted in the embedded terminal
// (overlap like "cacargo tauri dev\").
//
// The exact byte stream zsh produced during that interaction is checked in as
// fixtures/zsh-zle-replay.b64. This spec replays it through the production
// IPC pipeline (pty_spawn → pty_write → PTY output → pty:raw events → xterm
// write) into a spec-owned xterm.js instance and asserts the final buffer
// holds exactly what a correct terminal (Ghostty) shows: "z ath" alone, with
// no stale fragments ("ca", "cargo tauri …", "omp") surviving on the row.
//
// The assertion is renderer-agnostic: it reads term.buffer directly, so it
// holds under both the DOM renderer and the WebGL renderer.

const PANE_ID = 'e2e-zsh-zle-replay-pane'

describe('zsh zle history-recall replay (ghosting regression)', () => {
  it('leaves no stale glyphs after syntax-highlighted history recall + delete', async () => {
    await browser.waitUntil(
      () => browser.execute(() => Boolean(window.__TAURI__)),
      { timeout: 15000, timeoutMsg: 'Tauri IPC never became available' }
    )

    // Spec-owned xterm consuming the pane's raw stream — mirrors the
    // production write path (base64 event → bytes → term.write).
    await browser.execute((paneId) => {
      const host = document.createElement('div')
      host.id = 'zsh-replay-host'
      document.body.appendChild(host)
      const term = new Terminal({
        cols: 120,
        rows: 30,
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 14,
        scrollback: 1000,
        allowProposedApi: true,
      })
      term.open(host)
      window.__replayTerm = term
      window.__replayUnlisten = null
      window.__TAURI__.event
        .listen(`pty:raw:${paneId}`, (event) => {
          const payload =
            typeof event.payload === 'string'
              ? JSON.parse(event.payload)
              : event.payload
          const bytes = Uint8Array.from(atob(payload.data), (c) => c.charCodeAt(0))
          window.__replayTerm.write(bytes)
        })
        .then((unlisten) => {
          window.__replayUnlisten = unlisten
        })
    }, PANE_ID)

    // Spawn a real interactive zsh in the repo root (sandbox-clean cwd), then
    // have it base64-decode the fixture so the exact captured zle stream
    // comes out of a real PTY through the production event pipeline.
    await browser.execute((paneId) => {
      return window.__TAURI__.core.invoke('pty_spawn', {
        id: paneId,
        cwd: '/Users/apollo/Documents/athenas-core',
        shell: '/bin/zsh',
        cols: 120,
        rows: 30,
        agent: null,
        command: '',
      })
    }, PANE_ID)
    // Let zsh finish its startup prompt before the replay begins.
    await browser.pause(1500)
    await browser.execute((paneId) => {
      return window.__TAURI__.core.invoke('pty_write', {
        id: paneId,
        data: 'base64 -D -i e2e-tests/test/fixtures/zsh-zle-replay.b64\r',
      })
    }, PANE_ID)
    // Give the fixture output time to flush through the 8 ms coalescing tick.
    await browser.pause(1500)

    const report = await browser.execute(() => {
      const term = window.__replayTerm
      const buf = term.buffer.active
      const lines = []
      for (let y = 0; y < buf.length; y++) {
        const line = buf.getLine(y)
        if (line) lines.push(line.translateToString(true))
      }
      const joined = lines.join('\n')
      const athRows = lines.filter((l) => l.includes('z ath'))
      return { tail: lines.slice(-6), hasZath: joined.includes('z ath'), athRows }
    })

    // The replayed stream ends mid-edit with the buffer holding "z ath".
    expect(report.hasZath).toBe(true)
    expect(report.athRows.length).toBe(1)
    // No stale fragments from earlier frames may survive into the final line.
    const finalRow = report.athRows[0]
    expect(finalRow).not.toMatch(/cargo|tauri|omp/)
    expect(finalRow).not.toContain('ca')
    expect(finalRow).not.toMatch(/\\/)

    await browser.execute(async (paneId) => {
      if (window.__replayUnlisten) window.__replayUnlisten()
      try {
        await window.__TAURI__.core.invoke('pty_kill', { id: paneId })
      } catch {}
      document.getElementById('zsh-replay-host')?.remove()
      delete window.__replayTerm
    }, PANE_ID)
  })
})
