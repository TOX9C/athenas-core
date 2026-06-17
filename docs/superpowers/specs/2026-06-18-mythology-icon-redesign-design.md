# Greek-Mythology Icon & SVG Redesign

**Date:** 2026-06-18
**Status:** Approved (design phase)
**Scope:** Full sweep of `frontend/src/components/shared/icon.rs`; light touch-up of `illustration.rs`.

---

## Context

Athena's Core already carries a Greek-mythology identity ("Obsidian & Gold / Athenaeum"):
bronze-gold `--accent` (`#C9A24B`), Cormorant/Hanken Grotesk/Monaspace fonts, themes named
Nyx/Aegis/Erebus/Pentelic/Olive/Sky, all color driven from CSS tokens. A partial mythology
motif set exists in `shared/icon.rs` (`IconOwl`, `IconLaurel`, `IconColumn`, `IconAegis`,
`IconAmphora`, `IconHelmet`, `IconScroll`, `IconMeander`) and `shared/illustration.rs` holds
themed empty-state art.

The gap: most **semantic and feature icons** still use generic Lucide-style line/polyline
stubs (Folder, File, Grid, Settings gear, Zap, Swarm, Globe, Edit, Send, Bell, Plugins) that
break the mythology family. The existing motif icons are also rough — sharp line/polyline
geometry rather than smooth curves.

## Goal

Redesign **every** icon in `icon.rs` so the whole set reads as one Greek-mythology family,
is smooth (Bézier curves, not jagged polylines), and stays legible at 16–18px where most
render. Brand mark and illustrations get the richest detail.

## Non-Goals

- No raster images / no pulled-from-web PNGs. Everything stays inline SVG so the theme-recolor
  system (currentColor / `--accent`) keeps working and icons stay crisp at all sizes.
- No downstream churn: component **names and props stay identical** (`IconX(size, color)`,
  `IconOwl(size, color)`, `EmptyArt` enum, `OwlMark(size)`). Only the SVG path data changes.
- No layout / token / theme-palette changes.
- No new call sites added (out of scope); existing call sites keep working verbatim.

## Design Decisions

### Medium: inline SVG (chosen over raster)
- Inherits theme color via `currentColor` / `--accent`; recolors across all 6 themes.
- Crisp on retina at 16–48px; zero binary bloat; instant load in WASM.
- Raster would fight recolor and blur. SVG *is* the smooth option.

### Visual fidelity: stylized symbolic (tier B)
- Smooth Bézier `path` curves replace jagged `line`/`polyline` stubs — primary "smooth" lever.
- Single 1.5px stroke, round caps/joins, one coherent family.
- Each semantic icon clearly *is* the mythological object (recognizable silhouette) without
  engraving-density detail that would muddy at 16px.

### Tiered treatment
1. **Action icons** (×, +, −, chevrons, arrows, check, refresh, copy, trash, menu, play,
   fullscreen, minimize) → smooth geometry, stay universal. Pure form reads faster than motif
   for these; forcing a motif onto "close" hurts usability.
2. **Semantic / section / feature icons** → full mythology treatment (see mapping below).
3. **Brand owl + empty-state illustrations** → richest detail, optional low-opacity fill
   under stroke for depth.

### Stroke language (unchanged structurally)
- `inline_svg`: viewBox `0 0 24 24`, `fill: none`, `stroke: color`, `stroke_width: "1.5"`,
  round caps/joins. (Keep current wrapper; only swap inner paths.)
- `empty_svg` (empty-state icons in `icon.rs`): keep 1.4 stroke.
- `illustration.rs` `illo`: keep viewBox `0 0 120 96`, two-tone (`--textDim` + `--accent`).

## Mythology Mapping (icon.rs)

| Current component | New motif | Notes |
|---|---|---|
| `IconOwl` | **Owl of Athena** — redrawn front-facing, owl-eyes, ear tufts, beak, olive sprig in talons | brand mark; richest |
| `IconFiles` | **Papyrus scroll stack** (two rolled scrolls) | replaces file/document |
| `IconAgents` | **Corinthian helmet** | replaces person-circles |
| `IconPlugins` | **Knot of Heracles** (reef-knot interlock = modularity) | replaces wrench/slash |
| `IconSettings` | **Spoked celestial wheel / astrolabe** (gear→celestial) | keeps "settings" affordance via spokes |
| `IconSearch` | smooth **lens** (kept — affordance) | just smoothed paths |
| `IconZap` | **Keraunos** — Zeus thunderbolt, redrawn forked | replaces lightning bolt |
| `IconSwarm` | **Constellation** of connected stars | replaces node graph |
| `IconGrid` / `IconSpaces` | **2×2 meander-key tiles** | replaces plain squares |
| `IconGlobe` | **Armillary sphere** (celestial rings) | replaces globe |
| `IconEdit` | **Stylus + wax tablet** | replaces pencil |
| `IconSend` | **Herald's dart** (winged arrow) | replaces paper-plane |
| `IconTerminal` | `>` prompt (kept, smoothed) | affordance |
| `IconBell` | **Salpinx** (war-trumpet) hint for alert | replaces bell |
| `IconFolder` | **Amphora** silhouette (storage vessel) | replaces folder |
| `IconEye` / `IconEyeOff` | **Gorgoneion eye** / eye-with-meander-slash | themed view icons |
| `IconMoreHorizontal` | **three olive dots** | trivial motif tint |
| `IconPlay` | smooth triangle (kept, smoothed) | action affordance |
| `IconRefresh` | **Ouroboros**-curved arrows | action; motif tint |
| `IconArrowLeft/Right` | smooth arrows (kept) | action |
| `IconCheck` | smooth check (kept) | action |
| `IconCopy` | **overlapping wax tablets** | action + motif |
| `IconTrash` | **offering brazier / ash vessel** | action + motif |
| `IconMenu` | smooth three-line (kept) | action |
| `IconFile` | **single papyrus** | motif |
| `IconFullscreen` / `IconMinimize` | smooth corner brackets (kept) | action |
| `IconClose` / `IconPlus` / `IconMinus` / `IconChevron*` | smooth geometry (kept) | action |
| `IconLaurel` / `IconColumn` / `IconAegis` / `IconAmphora` / `IconHelmet` / `IconScroll` / `IconMeander` | redrawn smooth | existing motifs, unified |
| Empty-state icons (`IconEmpty*`) | redrawn smooth to match family | keep 1.4 stroke |

## illustration.rs Touch-ups

The illustrations are already mythology-themed and well-drawn. Minor smoothness pass only:
- Verify all curves are Bézier (most already are); replace any remaining straight-stub
  silhouettes with curves where it improves the silhouette.
- No structural/enum changes. `EmptyArt` variants unchanged.

## Compatibility Contract

- Every `pub fn Icon*` keeps its exact signature: `(size: Option<u8|u16>, color: Option<String>) -> Element`.
- `EmptyArt` enum and `EmptyState`/`OwlMark` signatures unchanged.
- Default sizes/colors per component unchanged.
- All current call sites (sidebar, workspace tabs, settings modal, etc.) compile and render
  without edit.

## Verification

1. `cargo check -p athena-frontend --target wasm32-unknown-unknown` — compiles.
2. `bash frontend/build-dist.sh --debug` then `cargo run --manifest-path src-tauri/Cargo.toml`
   — visual check: sidebar icons, empty states, brand owl, settings modal all render themed.
3. Watch for Dioxus RSX "attributes before children" ordering errors (stroke attributes must
   precede child elements inside each SVG primitive).
4. Confirm icons recolor correctly under Nyx (dark) and Pentelic (light) themes.

## Risks

- **16px legibility**: dense motifs (Knot of Heracles, armillary sphere) risk muddiness.
  Mitigation: prefer fewer, bolder strokes at small sizes; test in sidebar at 18px.
- **RSX ordering**: SVG primitive attributes must come before children — easy to slip.
  Mitigation: compile-check after each icon batch.
- **Affordance loss**: over-theming action icons (close, send) can hurt recognition.
  Mitigation: tiered treatment — action icons stay geometric.

## Deliverable

Rewritten `frontend/src/components/shared/icon.rs` (paths only, same API) + minor
`illustration.rs` smoothness pass. Single focused change, verifiable via wasm check + app run.
