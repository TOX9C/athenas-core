# Targeted Icon Legibility Fixes + Browser Quick-Links Dropdown

**Date:** 2026-06-23
**Status:** Approved (design phase)
**Scope:** 7 named icons in `frontend/src/components/shared/icon.rs`; quick-access/localhost
footer in `frontend/src/components/right_sidebar/browser_surface.rs`; the titlebar Athena
toggle call site in `frontend/src/lib.rs`.

---

## Context

Athena's Core carries a Greek-mythology identity ("Obsidian & Gold / Athenaeum"):
bronze-gold `--accent`, Cormorant/Hanke Grotesk/Monaspace fonts, themes Nyx/Aegis/Erebus/
Pentelic/Olive/Sky. The icon family in `shared/icon.rs` is the output of the
2026-06-18 mythology-first redesign (see
`docs/superpowers/specs/2026-06-18-mythology-icon-redesign-design.md`): smooth Bézier
paths, 24×24 grid, 1.5px stroke, round caps, one family.

That redesign deliberately thematized several **functional** icons as mythology motifs —
Settings as a "spoked celestial wheel", Refresh as "ouroboros-curved arrows", Fullscreen/
Minimize as corner brackets. The user reports these now read wrong at their actual render
sizes (12–18px):

- **Settings** reads as a **sun/asterisk**, not a gear.
- **Browser reload** reads as a **moon**, not a refresh.
- **Minimize** corner brackets curl into a **pinwheel**.
- **Athena titlebar toggle** shows a `>` terminal glyph for a button labeled "Athena".
- **Launch Swarm** (triangle + star + 2 dots) is **busy** at 16px.
- **Agents** Corinthian helmet cheek-guard curls get **muddy** at 18px.

Separately, the browser panel's **Quick Access + Localhost footer** renders 11 buttons across
two always-visible rows (browser_surface.rs:377), consuming viewport height.

## Goal

Fix the legibility of the 7 named icons by adopting a **Hybrid** direction: universal,
instantly-readable shapes for the functional chrome (settings gear, single circular reload
arrow, clean expand/collapse brackets) while keeping and refining the Greek-mythology
identity for brand/feature icons (Athena glyph, refined swarm constellation, refined helmet).
Collapse the browser quick-links footer into a compact dropdown menu.

## Non-Goals

- No raster images. Inline SVG only — the theme-recolor system (`currentColor` / `--accent`)
  must keep working and icons stay crisp at all sizes.
- No change to the icon API surface: component **names and props stay identical**
  (`IconX(size, color)`). Only SVG path data changes (except where a new icon is added).
- No sweep of the whole icon family — the other mythology icons (Owl, Laurel, Column, Aegis,
  Amphora, Scroll, Meander) are untouched.
- No layout/token/theme-palette changes beyond the new dropdown menu's markup + a small CSS
  class for it.
- No new call sites except swapping the titlebar Athena toggle from `IconTerminal` to the new
  `IconAthena`.

## Design Decisions

### Direction: Hybrid (universal chrome + refined mythology brand icons)
Functional chrome that must be recognized at a glance → universal, unambiguous geometry
(toothed gear, single circular arrow, mirror corner brackets). Brand/feature icons → keep the
mythology identity but simplify for legibility (Athena owl glyph, hub-and-spoke swarm, cleaner
helmet). This deliberately **reverses** the 2026-06-18 decision to thematize Settings as a
celestial wheel and Refresh as an ouroboros — those motifs lost their affordance at small
sizes. The mythology family is preserved everywhere else.

### Stroke language (unchanged)
`inline_svg`: viewBox `0 0 24 24`, `fill: none`, `stroke: color`, `stroke_width: "1.5"`,
round caps/joins. Keep the existing wrapper; only swap inner paths.

### Affordance priority
At 12–18px render sizes, **recognition beats richness**. Each functional icon must read as its
universal meaning in under a glance. Brand/feature icons prioritize a recognizable silhouette
over engraved detail.

## Icon Mapping (icon.rs)

| Component | Problem | Redesign |
|---|---|---|
| `IconSettings` | circle + 8 spokes reads as sun | **Toothed gear** — single path with 8 trapezoidal teeth around the rim, center hub circle, inner hole. Unambiguous settings affordance at 12px. |
| `IconRefresh` | two overlapping arcs merge into a crescent → moon | **Single circular arrow** — one clockwise ≈270° arc with a solid arrowhead at the top end. No second arc to confuse the eye. |
| `IconFullscreen` | clean but thin corner brackets | **Refined outward brackets** — 4 corner brackets pointing outward, slightly longer arms + tighter corner radius so they read as a frame. |
| `IconMinimize` | corners curl inward into a pinwheel | **Inward corner brackets** — 4 brackets pointing inward toward center (collapse metaphor, the mirror of fullscreen). |
| `IconSwarm` | triangle + star + 2 dots busy at 16px | **Hub-and-spoke constellation** — central node + 3 satellite nodes connected by lines. Drops the busy star. Reads as a network/swarm. |
| `IconAgents` | cheek-guard curls + nose guard + crest muddy at 18px | **Simplified Corinthian helmet** — keep dome + nose guard + crest; reduce cheek-guard curls to cleaner single strokes. |
| `IconAthena` (NEW) | titlebar "Athena (Cmd+J)" toggle uses `IconTerminal` (`>`) | **Compact owl face** — two eyes + ear tufts + beak (a simplified Owl of Athena). Direct "Athena" brand signal. Default size 16. |

## Call-site change (lib.rs)

The titlebar Athena toggle (lib.rs:586–596) currently renders `IconTerminal`. Swap to the new
`IconAthena` so the glyph matches the "Athena" label and concept. No other call sites change.
`IconTerminal` stays available for the actual Terminal affordance.

## Browser Quick-Links Dropdown (browser_surface.rs)

Replace the two-row **Quick Access + Localhost** footer (browser_surface.rs:377–444) with a
single compact bar containing one dropdown button. Clicking opens a popover menu grouping the
existing entries under two section headers ("Quick Access", "Localhost"). Entry lists and
navigation behavior are unchanged.

### Markup
- A container row (same border-top / `bgSecondary` styling as today) holding:
  - one `button` ("Quick links" + a chevron) that toggles a `show_quick_menu` signal.
- A popover `div` (absolutely positioned, above the bar) rendered when `show_quick_menu` is
  true, containing:
  - a "Quick Access" section header + the 5 quick entries as menu rows,
  - a "Localhost" section header + the 6 localhost entries as menu rows.
- Each menu row is a `button` carrying the same `browser_navigate` onclick as today; clicking
  also closes the menu.
- Click-away to close: a `window` `mousedown` listener (registered while the menu is open, via
  `use_effect` on `show_quick_menu`) that sets `show_quick_menu = false` on the next mousedown
  anywhere outside the popover. A transparent overlay is **not** used — it risks intercepting
  sidebar/window interactions.

### State
- Add `let mut show_quick_menu = use_signal(|| false);`.
- Keep the existing `quick_urls` / `localhost_urls` Vecs unchanged.

### CSS
- Add a small `.quick-menu` class (and `.quick-menu-row`) to `frontend/public/styles.css`:
  popover surface using `--bgSecondary`/`--border`/`--radius-md`, `--shadow-md`, with row
  hover using `--bgTertiary`. No new tokens; reuse existing ones.

## Compatibility Contract

- Every edited `pub fn Icon*` keeps its exact signature `(size: Option<u8>, color: Option<String>) -> Element`.
- New `IconAthena` follows the same signature; default size 16.
- All current call sites compile and render without edit except the single lib.rs swap
  (`IconTerminal` → `IconAthena`).
- The browser panel's public props / behavior unchanged; only the footer internals change.

## Verification

1. `cargo check -p athena-frontend --target wasm32-unknown-unknown` — compiles.
2. `bash frontend/build-dist.sh --debug` then `cargo run --manifest-path src-tauri/Cargo.toml`
   — visual check:
   - Titlebar: Settings reads as a gear; Athena toggle shows the owl glyph; Launch Swarm reads
     as a constellation.
   - Sidebar: Agents reads as a clean helmet.
   - Browser toolbar: reload reads as a circular arrow; expand/dock toggle reads as
     expand↔collapse brackets.
   - Browser footer: single "Quick links" button; clicking opens a popover with Quick Access +
     Localhost sections; entries navigate; click-away closes.
3. Watch for Dioxus RSX "attributes before children" ordering errors (stroke attributes must
   precede child elements inside each SVG primitive).
4. Confirm icons recolor correctly under Nyx (dark) and Pentelic (light) themes.

## Risks

- **16px legibility**: a toothed gear has many small features. Mitigation: 8 teeth (not more),
  bold enough teeth; test at 16px in the titlebar.
- **RSX ordering**: SVG primitive attributes must come before children. Mitigation:
  compile-check after each icon.
- **Dropdown click-away**: the `window` `mousedown` listener must ignore clicks inside the
  popover and must not leak after close. Mitigation: register/deregister it in a `use_effect`
  keyed on `show_quick_menu`; check the click target against the popover element before
  closing. No transparent overlay is used (it would risk intercepting sidebar/window
  interactions).
- **Affordance of new Athena glyph**: a compact owl could read as "eyes". Mitigation: include
  ear tufts + beak so it reads as an owl/brand, not a generic face.
