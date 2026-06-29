# athenas-core Frontend Redesign Plan

## Executive Summary

This plan documents a comprehensive redesign of the athenas-core Dioxus frontend. The redesign targets every panel, component, store, and UI surface to achieve a premium, professional aesthetic inspired by Linear and Warp Terminal aesthetics. Dark theme default with teal accent (#00c2b5), no emojis, no chat-bubble borders, no left-border accent lines, and a class-based CSS system to replace ~8,228 inline `style` string attributes.

## Theme

| Token | Current Value | Proposed Value | Notes |
|---|---|---|---|
| `--bg` | `#0f1115` | `#080a0e` | Darker, more ink-like |
| `--bgSecondary` | `#161922` | `#0d0f14` | Reduced luminance |
| `--bgTertiary` | `#1c1f2b` | `#14161d` | Subtle lift |
| `--bgElevated` | `#232735` | `#1a1c24` | Cards, modals |
| `--surface` | `#2b3040` | `#22262f` | Buttons, inputs |
| `--surfaceHover` | `#3a4055` | `#2a2f3a` |
| `--border` | `#2a3040` | `#1e212a` | Softer divider |
| `--accent` | `#ff5a5a` | `#00c2b5` | Teal accent |
| `--accentHover` | `#ff7b7b` | `#00e0d5` | Brighter on hover |
| `--text` | `#f0f4f8` | `#e8edf3` | Slightly cooler |
| `--textMuted` | `#9ca3b0` | `#7a8290` | Reduced contrast |
| `--textDim` | `#5a6070` | `#4a5060` |
| `--textInverse` | `#0f1115` | `#080a0e` |
| `--badgeBg` | `#1c3d3a` | `#0a2e2b` | Muted teal badge |
| `--badgeText` | `#00e0d5` | `#7fe8e0` |
| `--success` | `#2ecc71` | `#00c2b5` | Align with accent |
| `--warning` | `#f1c40f` | `#e5a53d` |
| `--error` | `#ff5555` | `#e05252` |
| `--info` | `#3498db` | `#4dabf7` | Only place blue appears |
| `--link` | `#38bdf8` | `#00c2b5` | Replace sky-400 |
| `--codeBg` | `#1e222a` | `#12151a` |
| `--codeBorder` | `#2a303d` | `#1e212a` |
| `--shadow` | `#000000` | `#000000` |
| `--shadowLight` | `rgba(0,0,0,0.2)` | `rgba(0,0,0,0.15)` |

## CSS Architecture

### Problem: Inline Style Abuse
The frontend currently uses ~8,228 inline `style: "..."` attributes across 64 source files. This prevents:
- Theme consistency (values must be updated in every inline string)
- Responsiveness (no media queries possible)
- Maintainability (finding all uses of a design token requires grep)
- Zero CSS class reuse

### Solution: CSS Custom Property + Class Token System

1. **Retain the 21 CSS custom properties** in `frontend/public/styles.css`
2. **Add semantic utility classes** mapped to those properties (e.g., `.bg-surface`, `.text-accent`, `.border-subtle`)
3. **Dioxus components use `class:` attribute + `class` string**, NOT inline `style:`
4. **Dynamic values (widths, heights, positions)** continue using inline styles, but ALL color/typography/spacing use classes

### CSS Class Structure

```css
/* Base (frontend/public/styles.css) */
.bg-base         { background: var(--bg); }
.bg-secondary    { background: var(--bgSecondary); }
.bg-tertiary     { background: var(--bgTertiary); }
.bg-elevated     { background: var(--bgElevated); }
.bg-surface      { background: var(--surface); }
.bg-accent       { background: var(--accent); }
.text-primary    { color: var(--text); }
.text-muted      { color: var(--textMuted); }
.text-dim        { color: var(--textDim); }
.text-accent     { color: var(--accent); }
.border-subtle   { border-color: var(--border); }
.border-accent   { border-color: var(--accent); }
.font-ui         { font-family: 'Instrument Sans', ...; }
.font-mono       { font-family: 'JetBrains Mono', ...; }
/* ... and semantic component classes */
.panel-base      { background: var(--bg); border: 1px solid var(--border); }
.pill            { border-radius: 9999px; padding: 2px 8px; font-size: 11px; ... }
.badge           { background: var(--badgeBg); color: var(--badgeText); padding: 2px 6px; border-radius: 4px; }
```

### Migration Strategy
- Phase 1: All new components use `class:` only
- Phase 2: Refactor each existing component file, replacing inline styles with class references
- Phase 3: Remove one-off inline style values that can be class-driven

## Complete Component Inventory

### lib.rs (Root App)
- **Responsibility**: Global layout, titlebar, keybindings, store provision, panel routing
- **Lines**: 498
- **Inline styles**: ~120 (titlebar, content area, sidebar rail, empty state, status bar, buttons)
- **Issues**: Emojis in titlebar buttons (🧠, 👥, ⚙), no class usage, hardcoded pixel values
- **Redesign**: Replace emoji buttons with SVG icons, apply panel classes, extract layout constants

### Sidebar (components/sidebar)
- **Files**: `sidebar.rs`, `sidebar_dir/file_tree.rs`, `sidebar_dir/mod.rs`
- **Responsibility**: Left sidebar with Spaces/Files/Agents/Plugins sections, collapsed rail mode, file tree
- **Inline styles**: ~45
- **Issues**: Left-border accent removed per user feedback — now uses background color for active state. File tree uses inline `padding-left` for indentation.
- **Redesign**: CSS class-based depth indentation, smooth section transitions, refined active-state styling

### Terminal Grid (components/workspace/terminal_grid.rs)
- **Responsibility**: Multi-pane terminal workspace, drag splits, pane management
- **Inline styles**: ~60
- **Issues**: Pane borders hardcoded, no visual distinction between focused/unfocused panes, grid lines too prominent
- **Redesign**: Subtle pane borders, focused pane glow (box-shadow), floating pill headers, no chat-bubble borders

### Right Sidebar (components/right_sidebar/panel.rs)
- **Responsibility**: Right panel (Browser/Athena/Editor tabs)
- **Inline styles**: ~30
- **Issues**: Tab styling basic, no visual hierarchy
- **Redesign**: Clean tabs, subtle active indicator, proper content area padding

### Athena Panel (components/athena/athena_panel.rs)
- **Responsibility**: Bottom overlay chat panel (35vh)
- **Inline styles**: ~50
- **Issues**: Chat bubble borders (REMOVE per user), hardcoded `#38bdf8` sky-400, emoji "A" dot
- **Redesign**: Borderless chat messages, message backgrounds only, no border-radius on chat containers, proper avatar with initials

### Chat Message (components/athena/chat_message.rs)
- **Inline styles**: ~20
- **Issues**: Avatar hardcoded #38bdf8, bubble borders present
- **Redesign**: Borderless, background-only message blocks. User messages right-aligned with subtle background. Athena messages left-aligned with different background. No border-radius on side that touches edge.

### Kanban Board (components/kanban/kanban_board.rs)
- **Inline styles**: ~35
- **Issues**: Card styling basic, no drag visual feedback
- **Redesign**: Refined card styling, column headers with pill counts, drag ghost styling

### Swarm Board (components/swarm/swarm_board.rs)
- **Inline styles**: ~40
- **Issues**: Agent status dots, basic card layout
- **Redesign**: Status indicators with subtle glow, refined card layout

### Command Palette (components/command_palette/command_palette_inner.rs)
- **Inline styles**: ~25
- **Issues**: Basic overlay, no keyboard shortcut display
- **Redesign**: Refined overlay, keyboard shortcut badges, search highlight

### Modals (components/shared/modal.rs, new_space_modal.rs, swarm_modal.rs, settings_modal.rs)
- **Inline styles**: ~50 per modal
- **Issues**: Inconsistent modal sizes, basic backdrop
- **Redesign**: Consistent modal sizing, refined backdrop blur, header styling

### Notification System (components/notifications/notification_bell.rs, notification_toast.rs)
- **Inline styles**: ~30
- **Issues**: Toast styling basic, no progress indication
- **Redesign**: Refined toast cards, proper iconography, progress/success states

### Toast Container (components/shared/toast.rs)
- **Inline styles**: ~20
- **Issues**: Basic positioning, no animation classes
- **Redesign**: Slide-in animation, proper stacking

### Agent Inspector (components/agents/agent_inspector.rs)
- **Inline styles**: ~25
- **Issues**: Basic agent detail panel
- **Redesign**: Refined detail cards, status indicators

### Settings Modal (components/settings/settings_modal.rs)
- **Inline styles**: ~40
- **Issues**: Basic form styling
- **Redesign**: Consistent form controls, theme picker grid

### Plugin Event Bus / Input Request Modal (components/plugin/)
- **Inline styles**: ~30
- **Issues**: Input modal basic, bus invisible
- **Redesign**: Refined input modal, invisible bus unchanged

## Panel System

The `Panel` enum defines the center content area:

```rust
pub enum Panel {
    Workspace,    // Terminal Grid
    Editor,       // Code Editor (placeholder)
    Kanban,       // Kanban Board
    Swarm,        // Swarm Board
    Chat,         // Chat (legacy, likely unused)
    Settings,     // Settings (accessed via modal)
    Browser,      // Browser (right sidebar)
    Plugin,       // Plugin panel
    Notifications,// Notifications panel
    Agents,       // Agents panel
}
```

Switching is done via keyboard (1-4) or UI buttons. The panel system needs:
- Consistent panel padding/margins (all panels must have same inner spacing)
- Panel-specific header treatment (if any)
- No panel border — content fills the space organically

## Store Architecture (15 Stores)

| Store | File | Purpose |
|---|---|---|
| UI Store | `stores/ui.rs` | Panel, sidebar, theme, modals |
| Workspace Store | `stores/workspace.rs` | Spaces, panes, terminals |
| Terminal Store | `stores/terminal.rs` | PTY sessions, cell data, cursor |
| Athena Store | `stores/athena.rs` | Chat messages, state |
| Notification Store | `stores/notification.rs` | Notification list |
| Editor Store | `stores/editor.rs` | Editor state |
| Layout Store | `stores/layout.rs` | Layout configuration |
| Session Store | `stores/session.rs` | Backend sessions |
| Swarm Store | `stores/swarm.rs` | Swarm data |
| Task Store | `stores/task.rs` | Task state |
| Command Store | `stores/command.rs` | Command palette |
| Agent Output Store | `stores/agent_output.rs` | Agent output tracking |
| Agent Status Store | `stores/agent_status.rs` | Agent connection status |
| Panel Manager Store | `stores/panel_manager.rs` | Exclusive/Right panel |
| Toast Store | `components/shared/toast.rs` | Toast messages |

All stores use Dioxus `use_signal` / `use_context_provider` pattern. No redesign needed for store logic — only the UI they render.

## Tauri IPC Bridge

69 typed command wrappers in `tauri_bridge.rs` covering: Window, FS, Store, Session, OutputBuffer, Notifications, Plan, AgentComms, Search, MCP, Swarm, Shell, PTY, Tools, Athena, Browser, Plugin.

Events listened to by frontend (stringly-typed, no constants):
- `agent:status`, `terminal:exit`, `terminal:prompt`, `terminal:data`
- `agents:connected`, `agents:disconnected`, `agents:statusUpdate`, `agents:inputRequested`
- `output-capture:*`, `athena:*`, `notifications:*`, `plugin:*`, `swarm:*`, `fs:change:*`

No redesign needed for the bridge itself, but the component event handling should be reviewed for cleanup.

## Z-Index Layering

| Layer | Z-Index | Components |
|---|---|---|
| Base modals | 50 | NewSpaceModal, SwarmModal, SettingsModal |
| Command palette | 60 | CommandPalette |
| Toast/Athena | 100 | ToastContainer, AthenaPanel |

Current layering is functional. No changes needed.

## Theme System (25 Themes)

Located in `frontend/src/themes/mod.rs` (315 lines). Themes are named (e.g., Nord, Dracula, Tokyo Night) and applied via `apply_theme_to_dom()` which injects CSS custom properties via `web_sys`.

Fonts available: JetBrains Mono, Fira Code, Cascadia Code, etc.

## Terminal (Custom Cell Grid)

NOT xterm.js. The terminal is a 100% custom cell-grid renderer:
- Backend sends `CellDeltaEvent` with cell deltas and cursor position
- Frontend renders individual `<span>` DOM elements per cell
- Input path: keyboard events → `TerminalStore::send_input()` → `tauri_bridge::pty_write()`
- Each pane header shows: shell type, CWD, running process (if any)

## Icon Strategy

REMOVE all emojis. Replace with:
- SVG icons inline (as Dioxus `svg` elements)
- Or a minimal icon font (e.g., Phosphor Icons, Feather Icons)
- Titlebar: search icon, settings gear, athena brain, swarm users
- Sidebar: spaces, files, agents, plugins icons
- Status indicators: dot indicators with color only

## Implementation Priority

1. **CSS Foundation** (`styles.css` class system + theme tokens)
2. **Root App** (`lib.rs` — layout classes, remove emojis, SVG icons)
3. **Sidebar** (refined active states, icon integration)
4. **Terminal Grid** (pane styling, borders, headers)
5. **Athena Panel** (borderless chat, avatar fix, message styling)
6. **Right Sidebar** (tab styling, content padding)
7. **Modals** (consistent sizing, backdrop, form controls)
8. **Kanban & Swarm** (card styling, status indicators)
9. **Command Palette** (search highlight, keyboard badges)
10. **Notification/Toast** (refined cards, animations)
11. **Settings** (form controls, theme picker)
12. **Agent Inspector** (detail cards)
13. **Final polish** (spacing audit, color consistency, shadow/glow refinement)

## Open Questions / Blockers

1. Will the terminal keyboard input regression be fixed before redesign? (Currently broken — PTY write path non-functional)
2. Should we introduce a proper icon library (Phosphor/Feather) or inline SVGs?
3. Do we need a design token doc outside of CSS? (e.g. `tokens.rs` for Rust-side values)
4. Are there any panels from the original React app that still need porting to Dioxus?

## Appendix: File Inventory

| File | Lines | Responsibility |
|---|---|---|
| `frontend/src/lib.rs` | 498 | Root App, layout, keybindings |
| `frontend/src/tauri_bridge.rs` | 794 | Tauri IPC bridge (69 commands) |
| `frontend/src/stores/ui.rs` | 172 | UI state, Panel enum, themes |
| `frontend/src/stores/workspace.rs` | ~200 | Spaces, panes, terminals |
| `frontend/src/stores/terminal.rs` | ~300 | PTY sessions, cell grid |
| `frontend/src/stores/athena.rs` | ~150 | Chat messages |
| `frontend/src/themes/mod.rs` | 315 | 25 theme definitions |
| `frontend/src/components/sidebar.rs` | ~150 | Left sidebar |
| `frontend/src/components/sidebar_dir/file_tree.rs` | ~200 | File tree |
| `frontend/src/components/workspace/terminal_grid.rs` | ~400 | Terminal grid, panes |
| `frontend/src/components/athena/athena_panel.rs` | ~300 | Bottom chat panel |
| `frontend/src/components/athena/chat_message.rs` | ~100 | Chat message rendering |
| `frontend/src/components/kanban/kanban_board.rs` | ~300 | Kanban |
| `frontend/src/components/swarm/swarm_board.rs` | ~350 | Swarm board |
| `frontend/src/components/command_palette/command_palette_inner.rs` | ~250 | Command palette |
| `frontend/src/components/shared/modal.rs` | ~100 | Base modal |
| `frontend/src/components/shared/toast.rs` | ~150 | Toast container |
| `frontend/src/components/notifications/notification_bell.rs` | ~200 | Notification bell |
| `frontend/src/components/notifications/notification_toast.rs` | ~150 | Toast notifications |
| `frontend/src/components/agents/agent_inspector.rs` | ~200 | Agent inspector |
| `frontend/src/components/settings/settings_modal.rs` | ~250 | Settings modal |
| `frontend/src/components/plugin/input_request_modal.rs` | ~150 | Plugin input |
| `frontend/public/styles.css` | ~300 | Base CSS, 21 tokens |
| `frontend/styles.css` | ~300 | Duplicate of above |
| `docs/redesign-mockup-v3.html` | ~1200 | Full HTML/CSS mockup |
