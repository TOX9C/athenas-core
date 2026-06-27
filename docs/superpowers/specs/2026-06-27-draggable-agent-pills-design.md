# Draggable Agent Pills — Design

**Date:** 2026-06-27
**Status:** Approved (brainstorm)
**Scope:** Make agent "pills" (grid pane-header strips) draggable. Drop on another pill's grid cell = swap pane positions). Drop on Athena chat = add as Agent reference). Persist across restarts. Touch + mouse support.

## Context & Root Causes

The user wants ability to reorder panes in their workspace without resizing them. Two drop targets:
1. **In-grid swap** — drag pill A onto pill B's cell = exchange positions. The grid morphs around them; in a 2x2 with three panes (`[A][B]` / `[C───]`), dragging C onto A means C goes top-left, A goes full-width bottom row. No empty cells, no resize.
2. **Drag-out to Athena** — drag any pill onto Athena chat = existing `DraggableItem::Agent` reference flow. Already coded in `athena_panel.rs:393-426`. Just needs verification + extension for the new payload MIME.

### Why "grid pane-header pills" and not sidebar pills

Deep-dive grep revealed `frontend/src/components/sidebar_dir/agent_panel.rs` is **dead code** — the `AgentPanel` component is defined but never rendered anywhere in `frontend/src`. The DnD wiring inside it is technically correct but never fires. The user-recognizable pills are the grid pane-header strips rendered in `terminal_grid.rs:507-622` (border-radius 999px, the literal "pills").

### Why raw HTML5 DnD (not a library)

Dioxus 0.7.9 (`Cargo.lock`) serializes `draggable="true"` directly via `set_attribute(name="draggable", value="true")` (`dioxus-web-0.7.9/src/mutations.rs:174-213`). No CSS in the project sets `user-select: none` / `-webkit-user-drag: none` on the rows or ancestors (`styles.css` confirmed clean except for `.line-number`). CSP in `tauri.conf.json:33` is content/security — does not block drag. WKWebView on macOS honors HTML5 DnD. **No library needed.**

### User choices captured in brainstorm

| Decision | Choice |
|---|---|
| Target surface | Grid cell (move/swap panes) |
| Swap semantics | Exchange positions; A→B slot, B→A slot |
| Cross-workspace drag | Blocked (same workspace only) |
| Self-target (A onto A) | No-op |
| Empty cells | Concept doesn't exist (terminal_grid.rs always renders `cols` cells; under-filled rows have a spanning cell) |
| Visual feedback | Ghost overlay following cursor |
| Persistence | Save to athena-store (SQLite-backed, same path as other PaneConfig edits) |
| Undo | None (no Cmd+Z) |
| Athena drop | Wire both grid pills and sidebar pills to Athena |
| DnD engine | Raw HTML5 + ghost overlay (no library) |
| Touch | Required (mouse + touch unified) |

## Data Model

Add a stable position field to `PaneConfig`:

```rust
// frontend/src/types/workspace.rs
pub struct PaneConfig {
    pub id: String,
    pub agent_type: AgentType,
    pub custom_cmd: Option<String>,
    pub custom_agent_id: Option<String>,
    pub label: String,
    pub bypass_mode: bool,
    pub project_name: Option<String>,
    pub model_name: Option<String>,
    pub resume_id: Option<String>,
    pub resume_cmd: Option<String>,
    pub resume_dismissed: bool,
    pub slot_index: usize,   // NEW — persists across grid template changes
}
```

Render uses `SlotIndex → (row, col)` derived from `grid: GridTemplate` per `terminal_grid.rs:82-103`. Slot index is the source of truth; `space.panes` ordering becomes implementation detail.

### SlotIndex helper

```rust
// frontend/src/components/workspace/terminal_grid.rs
fn slot_to_row_col(slot: usize, cols: usize) -> (usize, usize) {
    (slot / cols, slot % cols)
}
fn row_col_to_slot(row: usize, col: usize, cols: usize) -> usize {
    row * cols + col
}
```

### Migration

Legacy panes (no `slot_index`) use `Vec::position` as default on first app open. athena-store schema gets one-time migration in `crates/athena-store/src/migrations/` reading each pane row, setting `slot_index = position` if missing.

## DnD Layer

New: `frontend/src/components/agents/drag_layer.rs`.

```rust
pub enum DragPayload {
    GridPane {
        space_id: String,
        source_slot: usize,
        pane_id: String,
        pane_label: String,
    },
    Agent {
        pane_id: String,
        agent_type: String,
        label: String,
    },
}

#[derive(Default)]
pub struct DragLayer {
    pub active: Signal<Option<DragPayload>>,
    pub cursor_xy: Signal<(i32, i32)>,
    pub hovered_cell: Signal<Option<usize>>, // slot_index
    pub active_pointer_drag: Signal<Option<DragPayload>>, // touch fallback
}
```

Two MIMEs in `dataTransfer`:
- `application/x-athena-grid-swap` — JSON-serialized `DragPayload::GridPane`
- `application/x-athena-agent-ref` — JSON-serialized `DragPayload::Agent` (legacy: same as existing `DraggableItem::Agent` JSON in `text/plain`)

### Touch path

Pointer-down on pill with 50ms timer + 5px movement threshold switches into manual drag mode. Updates `cursor_xy` on pointer-move; pointer-up dispatches synthetic drop with empty `dataTransfer`. Reduces to the same `DragLayer::active` payload.

## Grid Cell Drop Targets

Each `PaneItem` div in `terminal_grid.rs:209-260` gains:

```rust
div {
    class: "pane-cell",
    ondragover: move |e| {
        e.prevent_default();
        drag_layer.set_hovered(Some(my_slot_index));
    },
    ondragleave: move |_| drag_layer.set_hovered(None),
    ondrop: move |e| {
        e.prevent_default();
        if let Ok(payload) = serde_json::from_str::<DragPayload>(&e.get_data("application/x-athena-grid-swap")) {
            match payload {
                DragPayload::GridPane { space_id, source_slot, .. } => {
                    if space_id != active_space_id { return reject_cross_workspace(); }
                    workspace.swap_pane_slots(&space_id, source_slot, my_slot_index);
                }
                _ => {}
            }
        }
        drag_layer.clear();
    },
    // ...
}
```

## Workspace Store Mutation

In `frontend/src/stores/workspace.rs`:

```rust
pub fn swap_pane_slots(&mut self, space_id: &str, a: usize, b: usize) -> Result<(), WorkspaceError> {
    if a == b { return Ok(()); }  // self-target no-op
    let Some(idx_a) = self.find_pane_by_slot(space_id, a) else { return Err(...); };
    let Some(idx_b) = self.find_pane_by_slot(space_id, b) else { return Err(...); };
    let space = &mut self.spaces[/* ... */];
    space.panes[idx_a].slot_index = b;
    space.panes[idx_b].slot_index = a;
    self.persist_space(&space.id);
    Ok(())
}
```

Persists via existing `WorkspaceStore::persist_space()` which calls the `save_space` Tauri command (already used for label edits).

## Athena Drop Path

Existing drop at `athena_panel.rs:393-426` already accepts `text/plain` JSON of `DraggableItem::Agent`. Extend the handler to also accept `application/x-athena-grid-swap` (decode, project to `DraggableItem::Agent`) and `application/x-athena-agent-ref` (decode directly).

```rust
ondrop: move |e| {
    e.prevent_default();
    let grid_swap = e.get_data("application/x-athena-grid-swap");
    if let Ok(json) = grid_swap {
        if let Ok(p) = serde_json::from_str::<DragPayload>(&json) {
            match p {
                DragPayload::GridPane { pane_id, pane_label, .. } => push_agent_ref(pane_id, pane_label),
                DragPayload::Agent { pane_id, label, .. } => push_agent_ref(pane_id, label),
            }
        }
    }
    let legacy = e.get_data("text/plain");
    // existing DraggableItem code unchanged
},
```

`push_agent_ref` calls `tauri_bridge::athena_pin_agent(pane_id)` (`src-tauri/src/commands/mod.rs:1288-1303`).

## Cross-Workspace Blocker

Drop zone checks `payload.space_id == active_space_id`. Mismatch → toast via `notification` store. Existing `notification.rs` already provides `enqueue_toast()`.

## Visual Feedback

CSS additions in `frontend/styles.css`:

```css
.pane-cell {
  transition: outline-offset 120ms ease-out, box-shadow 120ms ease-out;
}
.pane-cell.is-drop-target {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
  box-shadow: var(--drag-target-glow);
}
.pane-cell--self {
  opacity: 0.9;
}
.drag-ghost {
  position: fixed;
  pointer-events: none;
  z-index: 9999;
  transform: translate3d(var(--dx), var(--dy), 0);
  opacity: var(--drag-ghost-opacity);
}
@media (prefers-reduced-motion: reduce) {
  .pane-cell, .drag-ghost { transition: none; }
}
```

Ghost rendered by `frontend/src/components/agents/drag_ghost.rs` — a memoized div that follows `drag_layer.cursor_xy` via inline `transform: translate3d()` updates.

## File Impact

Modified:
- `frontend/src/types/workspace.rs` — `slot_index` field on `PaneConfig`
- `frontend/src/stores/workspace.rs` — `swap_pane_slots`, `persist_space`, `find_pane_by_slot`
- `frontend/src/components/workspace/terminal_grid.rs` — drop handlers, render by slot
- `frontend/src/components/athena/athena_panel.rs` — accept new MIMEs
- `crates/athena-store/src/migrations/` — slot_index default migration
- `frontend/styles.css` — drop-target + ghost CSS

New files:
- `frontend/src/components/agents/drag_layer.rs` — DnD state
- `frontend/src/components/agents/drag_ghost.rs` — floating overlay
- `e2e-tests/test/specs/draggable-pill-swap.e2e.mjs` — E2E

## Tests

### Frontend unit (`#[cfg(test)]` in same files)

```rust
#[test]
fn swap_same_slot_is_noop() {
    let mut ws = WorkspaceState::new_test();
    ws.swap_pane_slots("space-1", 2, 2).unwrap();
    assert_eq!(ws.spaces[0].panes.iter().map(|p| p.slot_index).collect::<Vec<_>>(), vec![0,1,2,3]);
}

#[test]
fn swap_exchanges_slot_indices() {
    let mut ws = WorkspaceState::new_test();
    ws.swap_pane_slots("space-1", 1, 3).unwrap();
    let slots: Vec<_> = ws.spaces[0].panes.iter().map(|p| p.slot_index).collect();
    assert_eq!(slots, vec![0,3,2,1]);
}

#[test]
fn swap_calls_persist_space_callback() {
    let mut ws = WorkspaceState::new_test_with_persist_counter();
    ws.swap_pane_slots("space-1", 0, 1).unwrap();
    assert_eq!(ws.persist_call_count(), 1);
}

#[test]
fn migration_defaults_slot_index_to_vec_position() {
    let legacy = PaneConfig { slot_index: 0, ..default }; // pre-upgrade
    let migrated = migrate_pane_config(legacy, vec_position=2);
    assert_eq!(migrated.slot_index, 2);
}

#[test]
fn drag_payload_roundtrip_serde() {
    let p = DragPayload::GridPane { space_id: "s".into(), source_slot: 1, pane_id: "p".into(), pane_label: "L".into() };
    let json = serde_json::to_string(&p).unwrap();
    let de: DragPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(p, de);
}
```

### E2E (webdriver)

`e2e-tests/test/specs/draggable-pill-swap.e2e.mjs`:
1. Open app, navigate to a workspace with 2x2 grid, 4 panes.
2. `browser.execute` synthesizes `dragstart` on Pane[1], then `dragover` + `drop` on Pane[3] with `application/x-athena-grid-swap` JSON.
3. Assert DOM: Pane[3]'s title now matches original Pane[1]'s title; Pane[1]'s title matches original Pane[3]'s.

## Risks

| Risk | Mitigation |
|---|---|
| Legacy panes lack `slot_index` | One-time migration; default to Vec position |
| Touch path has different edge cases than mouse | Touch path tested separately; opt-in via pointer-down timer |
| WKWebView DnD quirks per `CLAUDE.md` known WASM runtime panics on click | Drag listeners attach at mount; if WASM survives mount, DnD fires. Same risk as today. |
| WebDriver `dragAndDrop` known buggy on WKWebView | E2E test uses `browser.execute()` to synthesize events directly, per `CLAUDE.md` note. |
| Persistence race vs in-memory swap | `swap_pane_slots` is synchronous in-memory; persistence is fire-and-forget; UI optimistically updates off the in-memory change. |

## Out of Scope (YAGNI)

- Undo stack (Cmd+Z)
- Cross-workspace drag
- Per-agent-type custom ghost
- Animation library
- Re-mounting dead-code `AgentPanel` component (separate ticket)
- Hex coordinate system (no need — grid is row/col only)
