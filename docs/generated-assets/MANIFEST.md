# Athena's Core — Generated Asset Library

Greek-mythology themed illustrations and motifs generated via Higgsfield AI.
All assets use the Nyx palette: **warm gold (#C9A24B) on near-black (#0B0B0C)**.

**Status: Mixed / legacy.** Hero and motif assets remain available under
`frontend/public/art/`. Empty states use inline, theme-aware SVG motifs instead
of raster art, so generated PNG canvases are retained as archived source
material rather than rendered in the UI.

---

## Available generated asset set (`frontend/public/art/`)

The generated empty-state PNGs below are legacy assets and are no longer used
by `EmptyState`. They remain in the asset directory for comparison or future
art direction work; the live empty-state implementation is the inline SVG set
in `frontend/src/components/shared/illustration_art.rs`.

Source originals (2048×2048 or 2048×1152) live in `docs/generated-assets/`;
deployed copies in `frontend/public/art/` are downscaled for web serving
(empty-states → 512×512, hero/motif → 1024×576).

| Deployed file             | Source                                | Aspect | Deployed size | Used in                                                      |
| ------------------------- | ------------------------------------- | ------ | ------------- | ------------------------------------------------------------ |
| `empty-workspace.png`     | `empty-workspace-v1-temple.png`       | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `empty-sessions.png`      | `empty-sessions-v2-leaves.png`        | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `empty-kanban.png`        | `empty-kanban-v2-tablets.png`         | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `empty-swarm.png`         | `empty-swarm-v2-council.png`          | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `empty-notifications.png` | `empty-notifications-v2-caduceus.png` | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `empty-plugins.png`       | `empty-plugins-v2-cogs.png`           | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `empty-files.png`         | `empty-files-v2-scroll.png`           | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `empty-agents.png`        | `empty-agents-v1-helmets.png`         | 1:1    | 512×512       | Legacy archive — not rendered by `EmptyState`                |
| `hero-athena.png`         | `hero-v3-oracle-delphi.png`           | 16:9   | 1024×576      | Athena chat panel welcome (`athena_panel.rs:566`)            |
| `hero-athena-alt.png`     | `hero-v4-owl-parthenon-bench.png`     | 16:9   | 1024×576      | Backup hero (not wired; ready to swap)                       |
| `motif-laurel.png`        | `motif-v2-laurel.png`                 | 16:9   | 1024×576      | Legacy archive — welcome screen now uses inline `IconLaurel` |

---

## App icon (v8 hexagonal core)

Flat geometric mark: pointy-topped hexagonal frame with a quiet inner ring and
a solid gold diamond core — Apple/Google-grade flat vector aesthetic, not
illustration. Source: `frontend/public/icons/athena.svg` (192 viewBox; rendered
to 1024×1024 and passed through `cargo tauri icon`). Replaces the v7
bezant-ring / triangle-shield marks.

Deployed as Tauri icon set in `src-tauri/icons/`:

- `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png`, `256x256.png`,
  `512x512.png`, `1024x1024.png` (via `icon.png`)
- `icon.icns` (macOS), `icon.ico` (Windows)
- Registered in `src-tauri/tauri.conf.json` `bundle.icon` array
- Also copied to `frontend/public/icons/athena.png` and `favicon-32.png`

---

## Integration points (all verified via `dx serve` browser smoke test)

| Component               | File                                                   | Change                                                                                                                                             |
| ----------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Empty-state art**     | `frontend/src/components/shared/illustration.rs`       | `art_for()` renders inline SVG motifs from `illustration_art.rs`; motifs inherit `--bg`, `--textDim`, and `--accent` and introduce no image canvas |
| **Hero image**          | `frontend/src/components/athena/athena_panel.rs:566`   | Replaced `CoreMark { size: Some(40) }` with `<img src="./art/hero-athena.png">` sized `min(320px, 70%)`, 16:9, rounded, shadowed                   |
| **Laurel motif**        | `frontend/src/lib.rs`                                  | Renders inline `IconLaurel` with theme-aware strokes; no raster banner or background canvas                                                        |
| **App icon**            | `src-tauri/tauri.conf.json`                            | `bundle.icon` array points to the v8 PNG/ICNS/ICO set                                                                                              |
| **Art serving**         | `frontend/build-dist.sh` + `tauri.conf.json` resources | `cp -r public/art dist/art` step; `"../frontend/dist/art/": "./art/"` resource mapping                                                             |
| **Empty-state styling** | `frontend/public/styles.css`                           | `.empty-state-art` provides a small, quiet, theme-native SVG treatment with no raster frame or image-specific override                             |
| **CSP**                 | `src-tauri/tauri.conf.json` (line 28)                  | `img-src 'self' data: blob:` already permits `./art/*.png`                                                                                         |

---

## Smoke test results (2026-08-11)

Run via `dx serve` on `localhost:8080`, verified with headless Chromium:

- ✅ **Welcome screen** — inline laurel motif renders centered above "New Workspace" without an opaque rectangle
- ✅ **Athena chat panel** — `hero-athena.png` (Oracle of Delphi) renders at 318px width with alt text
- ✅ **Empty-state surfaces** — native inline SVG motifs render without opaque raster backgrounds
- ✅ **Theme inheritance** — motif strokes follow the active surface and accent tokens
- ✅ **Build** — `bash build-dist.sh --debug` exits 0; only pre-existing warnings (`unused variable: now`, `unused_mut`) remain
- ℹ️ Keychain error in Athena panel — **pre-existing**, unrelated to art integration. Tauri `__TAURI__` global unavailable in `dx serve` browser dev mode; resolves inside native Tauri window.

---

## Generation provenance

- **Tool**: Higgsfield AI CLI (`/Users/apollo/.hermes/node/bin/higgsfield`)
- **Account**: `abdulrahim.muhammad@ntu.edu.iq` (free plan, workspace `69eca3b4-…`)
- **Models used**: Z Image (0.15 cr/ea, ~26 option-sketches) + Nano Banana 2 (2 cr, hero only)
- **Credit usage**: ~7.90 of 10.00 (2.10 remaining) — Nano Banana 2 hero + Z Image breadth pass
- **Constraint**: GPT Image 2 and Recraft V4.1 require Basic plan — unavailable on free tier; a new Recraft attempt was plan-gated (`job_minimum_basic_plan_required`)
- **Authored**: 2026-08-11
