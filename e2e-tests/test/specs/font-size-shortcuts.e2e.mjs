describe('Shared font-size shortcuts', () => {
  const dispatchShortcut = async (key, shift = false) => browser.execute(
    ({ key, shift }) => {
      const target = document.querySelector('.app-root')
      if (!target) return { ok: false, error: 'app root not found' }

      const event = new KeyboardEvent('keydown', {
        key,
        code: key === '=' ? 'Equal' : key === '+' ? 'Equal' : 'Minus',
        metaKey: true,
        shiftKey: shift,
        bubbles: true,
        cancelable: true,
      })
      target.dispatchEvent(event)
      return {
        ok: true,
        defaultPrevented: event.defaultPrevented,
        fontSize: getComputedStyle(document.documentElement)
          .getPropertyValue('--fontSize')
          .trim(),
      }
    },
    { key, shift },
  )

  it('maps Command equals/plus/minus to shared font-size changes', async () => {
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.app-root')),
      { timeout: 25000, interval: 250, timeoutMsg: 'App root did not mount' },
    )

    const initial = await browser.execute(() => Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
    ))
    expect(initial).toBeGreaterThanOrEqual(10)
    expect(initial).toBeLessThanOrEqual(24)

    const increasedByEquals = await dispatchShortcut('=')
    expect(increasedByEquals.ok).toBe(true)
    expect(increasedByEquals.defaultPrevented).toBe(true)
    expect(Number.parseFloat(increasedByEquals.fontSize)).toBe(Math.min(initial + 1, 24))

    const increasedByPlus = await dispatchShortcut('+', true)
    expect(increasedByPlus.defaultPrevented).toBe(true)
    expect(Number.parseFloat(increasedByPlus.fontSize)).toBe(Math.min(initial + 2, 24))

    const decreasedByMinus = await dispatchShortcut('-')
    expect(decreasedByMinus.defaultPrevented).toBe(true)
    expect(Number.parseFloat(decreasedByMinus.fontSize)).toBe(Math.min(initial + 1, 24))

    // Restore the persisted preference so this spec does not affect later
    // specs that share the local app data directory.
    await browser.execute(({ initial }) => {
      const target = document.querySelector('.app-root')
      if (!target) return
      const key = initial < Number.parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
      ) ? '-' : '='
      const steps = Math.abs(initial - Number.parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
      ))
      for (let i = 0; i < steps; i += 1) {
        target.dispatchEvent(new KeyboardEvent('keydown', {
          key,
          code: key === '=' ? 'Equal' : 'Minus',
          metaKey: true,
          bubbles: true,
          cancelable: true,
        }))
      }
    }, { initial })
  })
})
