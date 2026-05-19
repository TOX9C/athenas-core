# Athena's Core — Icon System & Aesthetic Redesign

**Date:** 2026-05-19
**Status:** Approved (mockup reviewed)
**Scope:** Dioxus/WASM frontend only (`frontend/src/`)

---

## 1. Problem

The Dioxus frontend uses 2-letter text abbreviations ("SP", "FL", "AG", "PL", "AI", "SW", "RS", "SET") and Unicode symbols where the original React/Electron app uses Lucide SVG icons. This makes the UI feel unpolished, harder to scan, and inconsistent with the app's identity as a precision development tool. Additionally, the current font stack (IBM Plex Mono + Instrument Sans) reads as robotic and generic for a modern IDE.

## 2. Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Aesthetic direction | Utilitarian-Industrial | Precision tool feel — dark, dense, information-rich. No decoration, every pixel earns its place. |
| Icon style | Adaptive (outline inactive → filled active) | Interactive feedback without extra animation. Matches industrial "state indicator" paradigm. |
| Icon delivery | Rust SVG components (fn() -> String) | Zero new deps, no build changes, works natively in Dioxus rsx!. Full CSS variable color control. |
| Font stack | Geist (UI) + Geist Mono (code) | Clean geometric sans, refined but not robotic. Smooth monospace for terminals. |
| Color palette | Deeper darks than current | Pushed backgrounds darker for contrast and depth. Accent-dim reduced for subtlety. |
| Approach | Icon-first incremental | Lowest risk, each PR visibly improves. Matches Electron Lucide set for consistency. |

## 3. Color Palette Changes

Current → Redesigned:

| Variable | Current | New | Change |
|----------|---------|-----|--------|
| `--bg` | `#0b0e13` | `#070810` | Darker |
| `--bgSecondary` | `#141820` | `#0c0e16` | Darker |
| `--bgTertiary` | `#1e232e` | `#131620` | Darker |
| `--bgHover` | `#1c2128` | `#161a26` | Darker |
| `--border` | `#2a303e` | `#1e2232` | Darker |
| `--borderActive` | `rgba(255,255,255,0.12)` | `#2e3450` | Explicit, darker |
| `--text` | `#e0e4ee` | `#b8bdd0` | Softer, less glaring |
| `--textMuted` | `#8890a4` | `#586074` | Darker |
| `--textDim` | `#525a6e` | `#2e3446` | Darker |
| `--accent` | `#38bdf8` | `#38bdf8` | Unchanged |
| `--accentSubtle` | `rgba(56,189,248,0.1)` | `rgba(56,189,248,0.08)` | More subtle |
| `--success` | `#22c55e` | `#22c55e` | Unchanged |
| `--error` | `#ef4444` | `#ef4444` | Unchanged |
| `--warning` | `#f59e0b` | `#f59e0b` | Unchanged |

These values go into `ThemeColors` struct in `frontend/src/themes/mod.rs` for all 20 dark themes (light themes get proportionally adjusted). The `:root` defaults in `frontend/styles.css` and `frontend/public/styles.css` are also updated.

## 4. Font Changes

| Context | Current | New |
|---------|---------|-----|
| UI font | `Instrument Sans`, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif | `Geist`, -apple-system, BlinkMacSystemFont, sans-serif |
| Mono font | `JetBrains Mono`, Fira Code, monospace | `Geist Mono`, `JetBrains Mono`, monospace |

Implementation: Update `--fontFamily` default in `styles.css`, add Geist + Geist Mono to the font list in `themes/mod.rs`, and add Google Fonts `<link>` tags to `frontend/index.html` (currently no web fonts loaded in the Tauri build). The `font_family` field in `UIState` and the `apply_font_to_dom()` function remain unchanged.

## 5. Icon System Architecture

### 5.1 Module Structure

```
frontend/src/components/icons/
├── mod.rs           — pub mod re-exports, IconStyle enum
├── terminal.rs      — terminal, chevron_right/left/down, cursor_line
├── navigation.rs    — arrow_left/right, back, forward, refresh, external_link
├── panels.rs        — grid_split, kanban_columns, swarm_nodes, layout_preview
├── actions.rs       — plus, close, close_small, settings, settings_filled, search, bell, bell_filled, minimize, maximize, restore
├── athena.rs        — helmet, helmet_filled, spark, tool_use
├── status.rs        — check, check_circle, warning, error, info, loading_spinner
├── sidebar.rs       — spaces, spaces_filled, folder, folder_filled, agent, agent_filled, plugin, plugin_filled, sidebar_toggle, sidebar_toggle_filled
├── right_panel.rs   — inspect, inspect_filled, globe, globe_filled, pulse, pulse_filled
└── file_type.rs     — rust_crab, ts_diamond, python_snake, js_circle, json_braces, md_memo, css_palette, html_globe, image_frame, config_gear, shell_terminal, go_circle, ruby_gem, lock_icon, git_eye, generic_file
```

### 5.2 Icon Function Pattern

Every icon has two constants: outline (default) and filled (active/hover state).

```rust
/// Outline variant — used for inactive/default state
pub const SETTINGS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" width="100%" height="100%"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#;

/// Filled variant — used for active/selected state
pub const SETTINGS_FILLED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" width="100%" height="100%"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#;
```

### 5.3 Helper for Adaptive State

```rust
/// Returns the appropriate icon variant based on active state.
/// Use with icon enum variants for compile-time safety:
pub fn icon(outline: &str, filled: &str, active: bool) -> &str {
    if active { filled } else { outline }
}

// Usage in rsx — store SVG strings as constants:
const SETTINGS_OUTLINE: &str = r#"<svg ...>...</svg>"#;
const SETTINGS_FILLED: &str = r#"<svg ...>...</svg>"#;

// Then in component:
div {
    style: "width: 16px; height: 16px; color: var(--accent);",
    dangerous_inner_html: icons::icon(SETTINGS_OUTLINE, SETTINGS_FILLED, is_active),
}
```

All icon functions return `&'static str` (not `String`) to enable `const` definitions and zero-allocation usage in the render loop.

### 5.4 Usage in Dioxus Components

```rust
// Before (text abbreviation):
div { style: "font-size: 9px; color: var(--text-dim);", "SET" }

// After (SVG icon — static):
div {
    style: "width: 16px; height: 16px; color: var(--text-dim);",
    dangerous_inner_html: icons::SETTINGS,
}

// After (adaptive — outline when inactive, filled when active):
div {
    style: "width: 16px; height: 16px; color: var(--accent);",
    dangerous_inner_html: icons::icon(icons::SETTINGS, icons::SETTINGS_FILLED, is_active),
}
```

Icons inherit color from their parent's `color` CSS property via `stroke="currentColor"` / `fill="currentColor"`. This means they respect theme variables automatically.

### 5.5 Sizing Convention

Icons use a 24x24 viewBox but are sized via their container:

| Context | Container size | stroke-width |
|---------|---------------|--------------|
| Titlebar buttons | 16x16px | 1.75 |
| Sidebar rail buttons | 16x16px | 1.75 |
| Sidebar section tabs | 15x15px | 1.75 |
| Panel switcher | 14x14px | 1.75 |
| Right sidebar tabs | 12x12px | 1.75 |
| Status bar | 10x10px | 1.75 |
| File type icons | 14x14px | 1.5 (slightly lighter for small size) |
| Inline (next to text) | 12x12px | 1.75 |

## 6. Complete Icon Inventory

### 6.1 Titlebar Icons (replacing text abbreviations in lib.rs)

| Current Text | Icon Name | Outline | Filled |
|-------------|-----------|---------|--------|
| "AI" | `athena_helmet` | Brain/helmet outline | Solid helmet |
| "SW" | `swarm_launch` | Connected nodes outline | Solid connected nodes |
| "RS" | `sidebar_toggle` | Panel with left arrow outline | Solid panel with arrow |
| "SET" | `settings` | Gear outline | Solid gear |
| Bell | `bell` | Bell outline | Bell with dot |
| `\u{2715}` | `close` | X outline | X solid |
| `\u{25a1}` | `maximize` | Square outline | Solid square |
| `\u{29c9}` | `restore` | Dual-window outline | Dual-window solid |
| `\u{2013}` | `minimize` | Horizontal line | Horizontal line |

### 6.2 Panel Switcher Icons (replacing text labels in lib.rs)

| Current Text | Icon Name | Visual |
|-------------|-----------|--------|
| "terminals" | `terminal` | Chevron-prompt (>_ ) |
| "panels" | `panels` | Split grid (3 panes) |
| "kanban" | `kanban` | Three columns of varying height |
| "swarm" | `swarm` | Connected nodes with center hub |

### 6.3 Sidebar Section Icons (replacing "SP", "FL", "AG", "PL" in sidebar.rs)

| Current | Icon Name | Visual |
|---------|-----------|--------|
| "SP" | `spaces` | 2x2 grid of rounded squares |
| "FL" | `folder` | Folder with tab |
| "AG" | `agent` | Chip/circuit with face dots |
| "PL" | `plugin` | Cross/puzzle piece |

### 6.4 Right Sidebar Tab Icons (adding icons to text labels in right_sidebar/panel.rs)

| Current | Icon Name | Visual |
|---------|-----------|--------|
| "DETAILS" | `inspect` | Crosshair/target |
| "BROWSER" | `globe` | Globe with meridians |
| "OUTPUT" | `pulse` | EKG/pulse line |
| "ASSISTANT" | `athena_helmet` (reused) | Same as titlebar AI |

### 6.5 Status Bar Icons (adding icon prefixes in lib.rs)

| Current | Icon Name |
|---------|-----------|
| workspace name text | `spaces` (small) |
| pane count text | `panels` (small) |
| panel name text | current panel icon (small) |
| theme name text | no icon (keep text-only) |

### 6.6 Unicode Symbol Replacements (across all components)

| Unicode | Location | New Icon |
|---------|----------|----------|
| `\u{203a}` / `\u{2039}` | sidebar.rs collapse | `chevron_right` / `chevron_left` |
| `\u{25be}` | various dropdowns | `chevron_down` |
| `\u{25b6}` | expand indicators | `chevron_right` |
| `\u{2190}` / `\u{2192}` | browser_panel.rs | `arrow_left` / `arrow_right` |
| `\u{21bb}` | browser_panel.rs | `refresh` |
| `\u{2197}` | various external links | `external_link` |
| `\u{00d7}` | workspace_tabs.rs, pane_header.rs | `close_small` |
| `\u{270f}` | various edit buttons | `pencil` |
| `\u{2318}` | settings_modal.rs shortcut labels | Keep as-is (macOS convention) |

### 6.7 File Type Icons (replacing emoji in file_icons.rs)

Emoji are replaced with small SVG icons using the same file-type-to-icon mapping. The `get_file_icon()` function changes return type from `&'static str` (emoji) to `&'static str` (SVG markup — still static, just different content). File type icons use a 1.5 stroke-width at 14px container size for clarity at small scale.

| Extension | Current Emoji | New SVG Icon |
|-----------|--------------|--------------|
| .rs | 🦀 crab | `rust_crab` — outlined crab shape |
| .ts | 💎 diamond | `ts_diamond` — diamond with "TS" |
| .tsx/.jsx | ⚛ atom | `tsx_atom` — atom symbol |
| .js | 🟡 yellow circle | `js_circle` — circle with "JS" |
| .json | 📋 clipboard | `json_braces` — curly braces |
| .md | 📝 memo | `md_memo` — document with lines |
| .css/.scss | 🎨 palette | `css_palette` — palette/brush |
| .html | 🌐 globe | `html_globe` — globe with angle brackets |
| .svg/.png/.jpg | 🖼 picture frame | `image_frame` — image icon |
| .yml/.yaml/.toml | ⚙ gear | `config_gear` — small gear |
| .sh/.bash/.zsh | 🐚 shell | `shell_terminal` — terminal prompt |
| .py | 🐍 snake | `python_snake` — two snakes |
| .go | 🔵 blue circle | `go_circle` — circle with arrow |
| .rb | 💎 gem | `ruby_gem` — gem shape |
| .lock | 🔒 lock | `lock_icon` — padlock |
| .gitignore | 👁 eye | `git_eye` — eye with branch |
| default | 📄 page | `generic_file` — document |

## 7. Spacing & Layout Refinements

Minor adjustments aligned with the utilitarian-industrial aesthetic. These are small tweaks, not a full restructure:

| Element | Current | Change | Rationale |
|---------|---------|--------|-----------|
| Sidebar rail width | 28px | 40px | Icons need more breathing room than 2-letter text |
| Sidebar section tab height | 28px | 32px | Taller hit target for icon buttons |
| Titlebar icon button size | text-based | 26x26px container | Consistent touch target |
| Titlebar icon gap | 1px | 2px | Slightly more air between icon buttons |
| Right sidebar tab style | uppercase text | icon + lowercase text | Less shouty, more scannable |
| Status bar icon size | none | 10x10px | Minimal, informational |

No changes to the overall app shell layout (titlebar 38px, sidebar 240px, right sidebar 400px, status bar 22px). Grid, flexbox, and panel structure remain the same.

## 8. Files Modified

| File | Changes |
|------|---------|
| `frontend/src/components/icons/mod.rs` | **NEW** — module re-exports, `icon()` helper |
| `frontend/src/components/icons/terminal.rs` | **NEW** — terminal, chevron icons |
| `frontend/src/components/icons/navigation.rs` | **NEW** — arrow, back, forward, refresh, external |
| `frontend/src/components/icons/panels.rs` | **NEW** — grid, kanban, swarm, layout icons |
| `frontend/src/components/icons/actions.rs` | **NEW** — plus, close, settings, search, bell, window controls |
| `frontend/src/components/icons/athena.rs` | **NEW** — helmet, spark, tool icons |
| `frontend/src/components/icons/status.rs` | **NEW** — check, warning, error, info, loading |
| `frontend/src/components/icons/sidebar.rs` | **NEW** — spaces, folder, agent, plugin, sidebar_toggle |
| `frontend/src/components/icons/right_panel.rs` | **NEW** — inspect, globe, pulse |
| `frontend/src/components/icons/file_type.rs` | **NEW** — all file type SVG icons |
| `frontend/src/lib.rs` | Replace text abbreviations with icons in titlebar, panel switcher, status bar |
| `frontend/src/components/sidebar.rs` | Replace "SP"/"FL"/"AG"/"PL" with icons, rail width 28→40px |
| `frontend/src/components/right_sidebar/panel.rs` | Add icons to tab labels |
| `frontend/src/components/workspace_tabs.rs` | Replace `\u{00d7}` with close_small icon |
| `frontend/src/components/terminal/pane_header.rs` | Replace Unicode with icons |
| `frontend/src/components/browser/browser_panel.rs` | Replace Unicode arrows/refresh with icons |
| `frontend/src/components/notification_bell.rs` | Use bell/bell_filled from icon system |
| `frontend/src/components/settings_modal.rs` | Add icons to section headers |
| `frontend/src/utils/file_icons.rs` | Return SVG strings instead of emoji |
| `frontend/src/themes/mod.rs` | Update dark theme defaults to deeper palette, add Geist fonts |
| `frontend/public/styles.css` | Update `:root` variables to darker palette, change font-family |
| `frontend/styles.css` | Same updates as public/styles.css |
| `frontend/index.html` | Add Google Fonts `<link>` for Geist + Geist Mono |

## 9. Implementation Order

Phase 1 — Icon module + titlebar (highest visibility, immediate impact):
1. Create `icons/` module with all ~80 icon functions
2. Replace titlebar text abbreviations in `lib.rs`
3. Replace panel switcher text in `lib.rs`

Phase 2 — Sidebar:
4. Replace sidebar rail "SP"/"FL"/"AG"/"PL" with icons, widen rail to 40px
5. Replace sidebar section tab icons
6. Replace sidebar header dots with icons

Phase 3 — Right sidebar + status bar:
7. Add icons to right sidebar tab labels
8. Add icon prefixes to status bar

Phase 4 — Remaining components:
9. Replace Unicode symbols in terminal pane header, browser panel, workspace tabs
10. Replace emoji file icons with SVG in `file_icons.rs`

Phase 5 — Color palette + fonts:
11. Update theme defaults to deeper darks in `themes/mod.rs`
12. Update `styles.css` and `public/styles.css` `:root` variables
13. Add Geist + Geist Mono font loading to `index.html`
14. Update font list in `themes/mod.rs`

Each phase is independently shippable. The app should build and run correctly after each phase.

## 10. Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| `dangerous_inner_html` XSS | All SVG strings are hardcoded literals in Rust source — no user input flows into them. Dioxus requires `dangerous_inner_html` for SVG; this is the standard pattern. |
| SVG rendering performance | ~40 SVGs on screen at once is negligible for WKWebView. Each icon is <500 bytes. No animation overhead. |
| Font loading failure | Geist has same fallback chain as current fonts. If Google Fonts CDN fails, falls back to system sans/mono. |
| Theme variable gap | Current `--bgHover`, `--borderActive`, `--accentSubtle`, `--shadow` are not in `ThemeColors` struct. This spec's palette changes only affect `:root` defaults and the struct fields that exist. A separate spec should add the missing variables to the theme system. |
| WASM binary size increase | ~80 SVG string literals add ~30KB to the WASM binary. Negligible relative to current size. |
| Emoji file icons in xterm | The file_icons.rs emoji are used in the sidebar file tree, not in the terminal. Terminal rendering is untouched. |
