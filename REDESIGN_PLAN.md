# Athena's Core — Full UI Redesign Structural Plan

> **Status:** Proposed · **Scope:** Every UI surface, token, icon, and illustration in `frontend/`
> **Goal:** Minimal & sleek, but with *character* — a coherent Greek-mythology identity that
> replaces the current generic / "AI-slop" feel.

---

## 1. Diagnosis — why it currently feels generic

Evidence gathered from a full pass over `frontend/src` + `public/styles.css`:

| Root cause | Where | Effect |
|---|---|---|
| Atmosphere deliberately disabled | `public/styles.css` header; `themes/mod.rs:231-235` sets `--themeGlowOpacity/Noise/Glow` to `0` | Flat, dead surfaces |
| Overused "AI" accent `#8b9eff` (periwinkle) | `styles.css:17`, `obsidian` theme | Reads as default template |
| Generic font pairing (Inter + JetBrains Mono) | `styles.css:33,47` | No identity |
| Text glyphs instead of icons (`×`, `+`, `▶`, `→`, `AG`, `OUT`, `SP`, `DEL`, `SRCH`…) | sidebar, palette, tabs, agents, toasts | Looks unfinished |
| Hardcoded hex (status dots, role/file badges, `#0b0e13`, `#f97316`) | swarm, agents, file_tree_node, modals | Breaks theming, inconsistent |
| Micro-typography 8–11px, no hierarchy | nearly all feature panels | Hard to read, no rhythm |
| Inconsistent radii (0 / 2 / 6 / 9999px) | buttons, inputs, cards, pills | Visual incoherence |
| Missing hover / focus / active states | buttons, cards, rows | Feels inert |
| Stub components | `tooltip.rs`, `context_menu.rs`, `error_boundary.rs`, `resizable_panel.rs` | Native/ugly fallbacks |
| Plain-text empty states everywhere | sidebar, sessions, swarm, kanban, notifications | No delight, no guidance |

---

## 2. Aesthetic North Star — "Obsidian & Gold / The Athenaeum"

**Concept:** A dark temple at night, lit by the lamp of wisdom. Athena = wisdom, strategy,
craft. We lean into *carved, inscribed, antiquarian* details rendered with modern restraint.

**Motifs (used sparingly, as accents — never busy):**
- **Owl of Athena** (Glaukōpis) → primary brand mark + "thinking" indicator
- **Meander / Greek key** → hairline divider + focus-ring motif
- **Doric/Ionic column, laurel, aegis/shield, amphora, helmet, olive branch** → section + empty-state art
- **Black-figure / red-figure pottery line art** → empty-state illustration style

### 2.1 Color identity
- **Signature accent: aged bronze-gold** — `#C9A24B` (base) / `#E0BC6A` (hover) / gold-leaf highlights.
- **Secondary: Aegean teal** — `#3E8E8A` for info/links, balances the warm gold.
- Dark themes = deep obsidian/ink with a *warm* not blue cast; light themes = Pentelic marble.
- Semantic colors retuned to sit in the palette (olive `success`, terracotta `error`, ochre `warning`).

### 2.2 Typography (bundle as local `@font-face`; do not rely on system)
| Role | Font | Why |
|---|---|---|
| Brand / display / H1–H2 | **Cormorant** (and Cormorant SC for inscribed caps) | High-contrast classical serif — unmistakably "Greek", zero AI-slop overlap |
| UI / body / dense labels | **Hanken Grotesk** (fallback: Mona Sans) | Humanist grotesk, warm but legible at 12–13px |
| Mono / terminal / code | **Monaspace Neon** (fallback: JetBrains Mono) | Distinctive, texture-healed, dev-credible |

> Keep the user font-picker (it overrides mono). Add display + UI font slots to the type system.

### 2.3 Motion (reverse the "no animation" rule — tasteful, CSS-only)
- One orchestrated **page-load reveal**: titlebar → sidebar → main panel staggered (`animation-delay`).
- **Lamp-glow pulse** on the owl mark while Athena is thinking.
- 150–200ms ease transitions on hover/active/focus; modal fade+rise; toast slide-in.
- Respect `prefers-reduced-motion`. Avoid JS-driven animation (WKWebView/Dioxus event quirks per CLAUDE.md).

---

## 3. Design-token foundation (do this first — everything depends on it)

**File:** `public/styles.css` + `frontend/src/themes/mod.rs` + `frontend/src/types/theme.rs`

Introduce a full token layer (CSS custom properties):

- **Color:** existing vars + `--accentGold`, `--accentGoldHover`, `--accentTeal`, `--gold-leaf`,
  `--ring` (focus), plus re-enable `--bgAtmosphere`, `--themeGlowColor`, `--themeGlowOpacity`, `--themeNoiseOpacity`.
- **Type scale:** `--font-display`, `--font-ui`, `--font-mono`; sizes `--text-xs:11 / sm:12 / base:13 / md:15 / lg:18 / xl:24 / 2xl:32`; weights; line-heights. **Raise floor to 12px.**
- **Spacing scale:** keep 4-based, add tokens `--space-1…8`.
- **Radius:** standardize → `--radius-sm:4px`, `--radius-md:8px`, `--radius-pill:999px`. Kill the 0/2/6 chaos.
- **Elevation:** `--shadow-sm/md/lg` (soft, low-opacity warm-black) + `--inset-hairline`.
- **Motion:** `--ease`, `--dur-fast:140ms`, `--dur:200ms`.
- **Atmosphere:** reusable `.atmosphere` layer (radial lamp glow + faint noise/marble texture), `.meander-rule` hairline.

Deliverable: a documented token reference comment block at top of `styles.css` replacing the
"No Animations, No Shadows, No Effects" banner.

---

## 4. Theme system overhaul

**Files:** `themes/mod.rs`, `types/theme.rs`, `stores/ui.rs`, `components/settings/theme_picker.rs`

- Rename/retune themes to the mythology palette (keep `System`):
  - **Dark:** `Nyx` (obsidian/ink + gold), `Aegis` (deep Aegean blue-black + bronze), `Erebus` (true black + gold-leaf).
  - **Light:** `Pentelic` (marble white + ink + terracotta), `Olive` (warm parchment + olive + bronze), `Sky` (cool marble + teal).
- Extend `ThemeColors` with `accent_gold`, `accent_teal`, `glow_color`, `glow_opacity`, `noise_opacity` so each theme drives its own atmosphere (stop zeroing it in `apply_theme_to_dom`).
- Add display/UI font family to font application (`apply_font_to_dom`).
- **theme_picker redesign:** replace 28px swatch squares with proper preview *cards* (mini app
  chrome + 3-swatch palette + theme name in Cormorant), hover lift, selected = gold ring.

---

## 5. Icon & illustration system (replaces all glyphs)

**File:** `components/shared/icon.rs` (+ new `components/shared/illustration.rs`)

1. **Audit & replace every text glyph** across the app with SVG icon components:
   `×`→`IconClose`, `+`→`IconPlus`, `▶`→`IconChevron`, `→/←/↻`→nav icons, `☰`, `·`, etc.
2. **Unify icon style:** one stroke width, `currentColor`, 24 viewBox, optical sizes. Replace
   `dangerous_inner_html` SVGs in `terminal_grid.rs` with components.
3. **Add a mythology motif set:** `IconOwl` (brand), `IconLaurel`, `IconColumn`, `IconAegis`,
   `IconAmphora`, `IconHelmet`, `IconOlive`, `IconMeander`, `IconScroll`, `IconLyre`.
4. **Brand mark:** `OwlMark` (used in titlebar, empty state, about, thinking indicator).
5. **Empty-state illustrations** (line-art, pottery style), replacing current geometric stubs:
   - Workspace → owl perched on a branch
   - Sessions/chat → unrolled scroll / amphora
   - Kanban → three columns of a temple façade
   - Swarm → constellation network (owl-eye hub)
   - Notifications → sleeping owl
   - Plugins → interlocking laurel pieces
6. **Replace cryptic abbreviations** with icon + label: sidebar rail (`SP/FL/AG/PL` → icons),
   agent tabs (`OUT/STS/ALT`), status bar (`AG`, `RUN/WAIT/OK/ERR`), file-type badges.

---

## 6. Shared primitives (propagate everywhere — do before feature panels)

**Dir:** `components/shared/`

| Component | Work |
|---|---|
| `button.rs` | Variants (primary-gold / secondary / ghost / danger), sizes, **focus ring (meander)**, hover, active-press, loading spinner, disabled treatment. Drive `.btn-*` from tokens. |
| `modal.rs` | Backdrop blur + scrim, soft shadow, fade+rise animation, Cormorant title, icon close, footer hidden when empty, ESC/scrim-close affordance. |
| `badge.rs` | Real variants (status/role/count/info) from tokens — kill ad-hoc color props. Optional leading icon. |
| `toast.rs` | SVG type icons (info/success/warn/error), gold left-rule, slide-in + auto-dismiss progress bar, consistent close icon. |
| `tooltip.rs` | **Implement** — themed floating tooltip (positioned, hairline border, fade), replace native `title`. |
| `context_menu.rs` | **Implement** — themed right-click menu (items, separators, icons, keyboard nav). |
| `resizable_panel.rs` | **Implement** a visible drag handle (hairline → gold on hover) shared by sidebar + grid + right sidebar. |
| `error_boundary.rs` | Real fallback UI (owl + "The oracle is silent…" + reset) instead of pass-through. |
| New: `input.rs`, `card.rs`, `segmented.rs` | Standard field (focus ring, `--radius-md`), card (bg + hairline + hover lift), segmented control to replace bare `<select>`s and toggle rows. |

---

## 7. App chrome

| Surface | File | Work |
|---|---|---|
| Titlebar | `lib.rs:388-523` | Owl brand mark + "Athena's Core" wordmark (Cormorant), gold-tinted active states, icon-only toolbar with tooltips, refined `+` and panel switcher, atmosphere layer behind. |
| Sidebar + rail | `sidebar.rs`, `lib.rs:534-575` | Replace `SP/FL/AG/PL` with icon+label; section header gets icon + Cormorant label; meander hairline dividers; gold active indicator (left bar, not underline). |
| Workspace list | `sidebar_dir/workspace_list.rs` | Tokenized status badges (no hardcoded rgba), hover row, count chips with labels, illustrated empty state. |
| File explorer / tree | `sidebar_dir/file_explorer.rs`, `file_tree*.rs` | Icon refresh button, SVG folder/file icons, tokenized file-type colors via a small palette map, hover bg, illustrated empty state. |
| Workspace tabs | `workspace/workspace_tabs.rs`, `workspace_tab.rs` | Icon add button, tokenized status dot, activity chip, hover/active, overflow fade indicator. |
| Status bar | `lib.rs:733-743` | Icon segments, gold theme-name, subtle separators (meander dot), live agent count. |

---

## 8. Core panels

| Surface | File | Work |
|---|---|---|
| Terminal grid + pane chrome | `workspace/terminal_grid.rs`, `grid_template.rs` | Visible resize handles, pane header refined (icon + label, fullscreen/close as icon components), Monaspace terminal font, illustrated empty pane, grid-template selector with clearer previews. |
| Athena chat panel | `athena/athena_panel.rs` | Owl avatar (not "A" text), Cormorant section titles, atmosphere behind, refined header icons, scroll affordances. |
| Chat input | `athena/athena_input.rs` | Real field styling, focus ring, send **icon** button, disabled + sending states, attach/context affordances. |
| Chat message | `athena/chat_message.rs` | Owl/user avatars, role label styling, timestamp format, message hover actions, refined block frames. |
| Blocks | `athena/{thinking,plan_block,ask_user_block,eval_block,content_block}.rs` | Owl lamp-glow thinking indicator (drop hardcoded `#38bdf8`), SVG step icons + checks, replace `❓` emoji, tokenized status, hierarchy + spacing. |
| Session list | `athena/session_list.rs` | Session type icons, icon refresh, row hover, illustrated empty state. |
| Right sidebar | `right_sidebar/{panel,browser_panel,editor_panel,skills_panel}.rs` | Tab icons + gold active, SVG nav icons (drop `←→↻` + `🌍`), real code highlighting frame, copy-feedback, illustrated empties. |

---

## 9. Feature surfaces

| Surface | Files | Work |
|---|---|---|
| Swarm | `swarm/{swarm_board,swarm_modal,swarm_launcher,agent_card,role_badge,activity_feed}.rs` | Tokenized role colors (kill hardcoded hex), role icons, status with subtle pulse, Cormorant header + owl, constellation empty state, refined modal (segmented size selector, styled textarea), activity feed with action icons + timestamps. |
| Kanban | `kanban/{kanban_board,kanban_column,kanban_card}.rs` | Column headers with icon + count chip, card as proper `card.rs` (bg + hover lift + drag handle icon), edit/delete as icon buttons, real edit modal (remove "TODO" text), column accent rules. |
| Agents | `agents/{agent_inspector,agent_selector,agent_status_bar,agent_output_panel,agent_output_line}.rs` | Replace `OUT/STS/ALT/AG/DEL/BOT/SRCH` with icons+labels, friendly agent names, tokenized status, progress bar for active, output line type icons, illustrated empty. |
| Notifications | `notifications/{notification_bell,notification_panel,notification_toast}.rs` | Type icons (not bare dots) + labels, timestamps, tab icons, refined badge, illustrated "sleeping owl" empty. |
| Plugins | `plugin/{plugin_dashboard,plugin_card,input_request_modal,agent_status_list}.rs` | Real plugin icon/avatar, tokenized status (drop `#f97316`/`#0b0e13`), capability chips, hover/active, search clear button, icon refresh, segmented filters, illustrated empty. |
| Command palette | `command_palette/command_palette_inner.rs` | Larger legible group headers (Cormorant SC), result icons, gold selected row, keycap styling, meaningful empty state (owl + hint), refined shortcut keycaps. |
| Settings | `settings/{settings_modal,theme_picker,shortcuts_ref}.rs` | Tab icons + gold active, styled inputs/fields, font-picker with live preview, theme preview cards (§4), grouped shortcut table with styled keycaps + category headers, owl About panel. |

---

## 10. Empty-state, motion choreography & QA polish

- Wire every empty state to its `Illustration` (§5.5) with a Cormorant headline + muted hint + primary action.
- Add the orchestrated load reveal + reduced-motion guard.
- Cross-theme QA: verify contrast (WCAG AA) for **all 6 themes** + System; confirm no hardcoded
  colors remain (`grep` for `#` hex in components).
- Verify nothing relies on JS-driven animation that trips WKWebView; build via `bash frontend/build-dist.sh --debug` and run `cargo run`.

---

## 11. Execution order (phased; each phase compiles & is reviewable)

1. **Phase 0 — Tokens & fonts** (§3) — bundle fonts, token layer, atmosphere utilities. *No visual regressions; foundation only.*
2. **Phase 1 — Theme engine** (§4) — palettes, atmosphere wiring, font slots.
3. **Phase 2 — Icons & illustrations** (§5) — icon set, motifs, brand mark, empty-state art.
4. **Phase 3 — Shared primitives** (§6) — button/modal/badge/toast/tooltip/context-menu/input/card.
5. **Phase 4 — Chrome** (§7) — titlebar, sidebar, tabs, status bar.
6. **Phase 5 — Core panels** (§8) — terminal grid, Athena chat + blocks, right sidebar.
7. **Phase 6 — Feature surfaces** (§9) — swarm, kanban, agents, notifications, plugins, palette, settings.
8. **Phase 7 — Empties, motion, QA** (§10).

**Guiding rule for every change:** drive *all* color/spacing/radius/type from tokens; no glyph
labels; one icon language; restraint over decoration — character comes from typography, the gold
accent, the owl, and a few well-placed motifs, not from clutter.

---

## 12. Open choices (reversible — committed to defaults; flag if you disagree)

1. **Default theme:** `Nyx` (obsidian + gold) dark. *(Alt: `Aegis` Aegean-blue.)*
2. **Accent:** bronze-gold `#C9A24B`. *(Alt: keep blue family but deeper/teal.)*
3. **Display font:** Cormorant. *(Alt: a Trajan-style inscribed face / "Fraunces".)*
4. **UI font:** Hanken Grotesk. *(Alt: Mona Sans / Public Sans.)*
5. **Mono font:** Monaspace Neon, user-overridable. *(Alt: keep JetBrains Mono.)*
