import { remote } from 'webdriverio'
import path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const projectRoot = path.resolve(__dirname, '..')
const binaryPath = path.join(projectRoot, 'target', 'debug', 'athenas-core')

const browser = await remote({
    hostname: '127.0.0.1',
    port: 4444,
    capabilities: {
        browserName: 'safari',
        'tauri:options': {
            binary: binaryPath
        }
    }
})

let failed = false
const fail = (msg, ctx) => {
    failed = true
    console.error('❌', msg)
    if (ctx !== undefined) console.error('   ctx:', JSON.stringify(ctx, null, 2))
}

try {
    // Wait for app + Dioxus mount
    await browser.pause(5000)

    // Pre-arm: skip modal validation
    await browser.execute(() => { window.__athenaE2E = true })

    // 1) Open modal: textContent of empty-state button is "+New Workspace"
    const r1 = await browser.execute(() => {
        for (const btn of document.querySelectorAll('button')) {
            if (btn.textContent.includes('New Workspace')) {
                btn.click()
                return { ok: true, text: btn.textContent.trim() }
            }
        }
        return { ok: false }
    })
    if (r1.ok) console.log('  clicked:', JSON.stringify(r1.text))
    else fail('No "New Workspace" button found')
    await browser.pause(1500)

    // 2) Pick Terminal Workspace card (avoid Launch Space button)
    const r2 = await browser.execute(() => {
        for (const btn of document.querySelectorAll('button')) {
            if (btn.textContent.includes('Terminal Workspace') && !btn.textContent.includes('Launch')) {
                btn.click()
                return { ok: true, text: btn.textContent.trim() }
            }
        }
        return { ok: false }
    })
    if (r2.ok) console.log('  clicked:', JSON.stringify(r2.text))
    else fail('No "Terminal Workspace" card found')
    await browser.pause(1500)

    // 3) Next > (validation bypassed)
    const r3 = await browser.execute(() => {
        for (const btn of document.querySelectorAll('button')) {
            if (btn.textContent.trim() === 'Next >') {
                btn.click()
                return { ok: true, text: btn.textContent.trim() }
            }
        }
        return { ok: false }
    })
    if (r3.ok) console.log('  clicked:', JSON.stringify(r3.text))
    else fail('No "Next >" button found')
    await browser.pause(1500)

    // 4) Add 3 Shell panes via #add-shell
    const r4 = await browser.execute(() => {
        const btn = document.getElementById('add-shell')
        if (!btn) return { ok: false, error: 'no #add-shell element' }
        for (let i = 0; i < 3; i++) btn.click()
        return { ok: true, count: 3 }
    })
    if (r4.ok) console.log('  added', r4.count, 'Shell panes')
    else fail('Could not add Shell panes', r4)
    await browser.pause(1000)

    // 5) Launch Space
    const r5 = await browser.execute(() => {
        for (const btn of document.querySelectorAll('button')) {
            if (btn.textContent.trim() === 'Launch Space') {
                btn.click()
                return { ok: true, text: btn.textContent.trim() }
            }
        }
        return { ok: false }
    })
    if (r5.ok) console.log('  clicked:', JSON.stringify(r5.text))
    else fail('No "Launch Space" button found')

    // Allow grid + xterm mounts to render
    await browser.pause(3000)

    // Geometry assertions
    const geom = await browser.execute(() => {
        const viewportW = window.innerWidth
        const viewportH = window.innerHeight

        let grid = null
        for (const el of document.querySelectorAll('div')) {
            const s = el.getAttribute('style') || ''
            if (s.includes('display: grid') && s.includes('grid-template-columns')) {
                grid = el
                break
            }
        }
        if (!grid) return { found: false, viewportW, viewportH }

        const gridRect = grid.getBoundingClientRect()
        const gridStyle = grid.getAttribute('style')

        const panes = []
        for (const child of grid.children) {
            const r = child.getBoundingClientRect()
            panes.push({
                w: Math.round(r.width),
                h: Math.round(r.height),
                x: Math.round(r.x),
                y: Math.round(r.y)
            })
        }

        const xterms = []
        for (const el of document.querySelectorAll('.xterm, [class*="xterm"]')) {
            const r = el.getBoundingClientRect()
            if (r.width > 0) {
                xterms.push({ w: Math.round(r.width), h: Math.round(r.height) })
            }
        }

        return {
            found: true,
            viewportW, viewportH,
            gridW: Math.round(gridRect.width),
            gridH: Math.round(gridRect.height),
            gridStyle,
            panes, xterms
        }
    })

    console.log('\nGEOMETRY:', JSON.stringify(geom, null, 2))

    if (!geom.found) {
        fail('Workspace grid element not in DOM after launching a space', geom)
    } else {
        const minW = Math.floor(geom.viewportW * 0.8)
        if (geom.gridW < minW) {
            fail(`Grid did not stretch to viewport (gridW=${geom.gridW}, viewportW=${geom.viewportW}, need >= ${minW})`)
        } else {
            console.log(`✅ Grid stretches to ${geom.gridW}px (viewport ${geom.viewportW}px)`)
        }

        const collapsedPanes = geom.panes.filter(p => p.w < 50 || p.h < 50)
        if (collapsedPanes.length > 0) {
            fail(`${collapsedPanes.length} pane(s) collapsed`, collapsedPanes)
        } else if (geom.panes.length === 0) {
            fail('Grid has no child panes')
        } else {
            console.log(`✅ All ${geom.panes.length} panes have real geometry`)
        }

        if (geom.panes.length === 3) {
            const sorted = [...geom.panes].sort((a, b) => a.y - b.y)
            if (sorted[0].x !== sorted[1].x) {
                fail('2x2 grid: expected top-row panes to share an x coordinate', sorted)
            } else {
                const row1MaxX = Math.max(sorted[0].x + sorted[0].w, sorted[1].x + sorted[1].w)
                const row2MaxX = sorted[2].x + sorted[2].w
                if (Math.abs(row1MaxX - row2MaxX) > 2) {
                    fail(`2x2 grid: bottom pane should match top-row right edge (row1=${row1MaxX}, row2=${row2MaxX})`, sorted)
                } else {
                    console.log(`✅ 2x2 grid layout: bottom pane spans full row width (${row2MaxX}px)`)
                }
            }
        }

        if (geom.xterms.length === 0) {
            fail('No xterm mounts found inside grid')
        } else {
            const collapsedXterms = geom.xterms.filter(x => x.w < 100)
            if (collapsedXterms.length > 0) {
                fail(`${collapsedXterms.length} xterm mount(s) collapsed (<100px wide)`, collapsedXterms)
            } else {
                console.log(`✅ All ${geom.xterms.length} xterm mount(s) have real width`)
            }
        }
    }

} finally {
    await browser.deleteSession()
}

if (failed) {
    console.error('\n❌ test_workspace_resize_geometry FAILED')
    process.exit(1)
} else {
    console.log('\n✅ test_workspace_resize_geometry PASSED')
}
