import { continueRender, delayRender, staticFile } from 'remotion'

let loaded: Promise<void> | null = null

// Deterministic font loading: fetch local woff2 once, register via FontFace
// so every render frame sees identical metrics. Variable fonts cover all
// weights used in the film.
export const loadFonts = (): Promise<void> => {
  if (!loaded) {
    // Block frame capture until fonts are registered, else early frames bake
    // in fallback metrics.
    const handle = delayRender('loading fonts')
    loaded = (async () => {
      try {
        const entries: Array<[string, string]> = [
          ['Hanken Grotesk', 'fonts/HankenGrotesk.woff2'],
          ['JetBrains Mono', 'fonts/JetBrainsMono.woff2'],
        ]
        await Promise.all(
          entries.map(async ([family, file]) => {
            const res = await fetch(staticFile(file))
            if (!res.ok) throw new Error(`font fetch failed ${res.status}: ${file}`)
            const buf = await res.arrayBuffer()
            const face = new FontFace(family, buf)
            await face.load()
            document.fonts.add(face)
          }),
        )
        await document.fonts.ready
      } finally {
        continueRender(handle)
      }
    })()
  }
  return loaded
}
