# Pane-Pill Drag-and-Drop Swap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drag an agent's workspace pane pill (whole pill = grab handle) and drop it onto another pane's pill to swap the two agents — full session migration (PTY follows the agent), each taking the other's grid slot/size.

**Architecture:** A pure-frontend `swap_pane_agents` method on `WorkspaceStore` does a `Vec::swap` on `Space.panes[]`. A pointer-driven drag (pointerdown on the pill → fullscreen overlay captures pointermove/up → `document.elementFromPoint` hit-tests the drop target → on pointerup, call `swap_pane_agents`) mirrors the proven `DragOverlay` resize-drag pattern already in `terminal_grid.rs`. xterm remounts on each pane key change; `pty_spawn` is idempotent by id so no PTY duplicates.

**Tech Stack:** Dioxus 0.7 (WASM), Rust, web-sys/wasm-bindgen interop, Tauri 2 IPC, existing `WorkspaceStore` SQLite persistence.

## Global Constraints

- **Branch:** cut a new branch off `main` (e.g. `feat/pane-pill-drag-swap`); PR back to `main`. Do NOT work on `redesign/astrolabe-starlight` (another agent is mid-flight there).
- **No six-dot grip.** The entire pill header `div` is the grab surface.
- **No HTML5 DnD.** Use PointerEvents only (`onpointerdown`/`onpointermove`/`onpointerup`). No `draggable`, no `dataTransfer`, no `ondragstart`/`ondragover`/`ondrop`.
- **Full session migration.** `PaneConfig.id` (PTY session) swaps with the rest of the config. Scrollback is lost on swap — accepted.
- **No backend changes.** No `src-tauri/` edits, no `crates/` edits, no new Tauri commands, no `PaneConfig` field changes (no `slot_index`).
- **Frontend-only persistence.** Reuse the existing `WorkspaceState::save()` path (called by `update_space`); no new IPC.
- **Coding style:** immutability — never mutate in place where a returned copy is clearer; small focused files; functions <50 lines; comprehensive error handling; no hardcoded values (use the existing CSS variables: `--accent`, `--ring`, `--line-lapis`, `--lit-edge`, `--bgSecondary`, `--text`, `--textMuted`, `--radius-pill`, `--dur-fast`, `--ease`).
- **Build commands:** `bash frontend/build-dist.sh --debug` (debug); `cargo test --workspace` (unit tests); `cargo run --manifest-path src-tauri/Cargo.toml` (run app).
- **Test framework:** Rust unit tests (`#[cfg(test)] mod tests` inside the file). No new test crate.

## File Structure

| File | Responsibility | Lines (est.) |
|---|---|---|
| `frontend/src/stores/workspace.rs` | Add `swap_pane_agents` method to `WorkspaceState` + unit tests | +90 |
| `frontend/src/components/workspace/pill_drag.rs` | **NEW** — `PillDrag` state struct, `PillDragOverlay` component (fullscreen overlay with pointermove/up + `elementFromPoint` hit-test), `PillDragGhost` rendering helper, JS interop for hit-test | ~190 |
| `frontend/src/components/workspace/terminal_grid.rs` | Add `pill_drag` signal; thread it + `space_id` into `PaneItem`; mark pane wrappers with `data-pane-id` + conditional `is-dnd-target` class; add `onpointerdown` to pill header; `onpointerdown` stop-prop on icon buttons; render `PillDragOverlay` + `PillDragGhost` | ~45 |
| `frontend/src/components/workspace/mod.rs` | Declare `pub mod pill_drag;` | +1 |
| `frontend/public/styles.css` | `.is-dnd-target` gold ring; `.dnd-ghost` floating pill style | +25 |

**Decomposition rationale:** the pointer-drag mechanics (state struct, overlay, ghost, hit-test interop) are a self-contained subsystem with one responsibility — the drag session lifecycle. Keeping it in `pill_drag.rs` keeps `terminal_grid.rs` focused on layout/rendering. The store mutation is a one-method addition to the existing `WorkspaceState`, mirroring `add_pane_to_space`/`remove_pane_from_space`.

---

### Task 1: `swap_pane_agents` store method (TDD)

**Files:**
- Modify: `frontend/src/stores/workspace.rs` (add method inside `impl WorkspaceState`, near `remove_pane_from_space` at line ~97; add `#[cfg(test)] mod tests` block at file end if none, or extend existing)
- Test: `frontend/src/stores/workspace.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `WorkspaceState::update_space(&mut self, id: &str, f: impl FnOnce(&mut Space))` (line 82), `Space.panes: Vec<PaneConfig>` (`types/workspace.rs:102`), `PaneConfig.id: String` (`types/workspace.rs:63`), `WorkspaceState::save()` (called inside `update_space`).
- Produces: `pub fn swap_pane_agents(&mut self, space_id: &str, pane_id_a: &str, pane_id_b: &str)` — full `PaneConfig` swap between two slots by id; no-op if either id missing or ids equal; persists via the existing `update_space`/`save` path.

- [ ] **Step 1: Write the failing tests**

Append to `frontend/src/stores/workspace.rs` (extend the existing `#[cfg(test)] mod tests` if present; create one at the file end if not):

```rust
#[cfg(test)]
mod swap_pane_agents_tests {
    use super::*;
    use crate::types::workspace::{AgentType, PaneConfig, Space};

    fn space_with_panes(ids: &[&str]) -> Space {
        let panes = ids
            .iter()
            .map(|id| PaneConfig {
                id: id.to_string(),
                agent_type: if *id == "shell" {
                    AgentType::Shell
                } else {
                    AgentType::Claude
                },
                label: Some(format!("label-{}", id)),
                ..Default::default()
            })
            .collect();
        Space {
            id: "s1".to_string(),
            name: "S".to_string(),
            dir: "/tmp".to_string(),
            grid: crate::types::workspace::GridTemplate::X1x2,
            panes,
            color: String::new(),
            created_at: 0,
            last_opened_at: 0,
        }
    }

    fn state_with(space: Space) -> WorkspaceState {
        WorkspaceState {
            spaces: vec![space],
            active_space_id: Some("s1".to_string()),
        }
    }

    #[test]
    fn swaps_two_panes_by_id_full_config_including_id() {
        let mut state = state_with(space_with_panes(&["alpha", "beta", "shell"]));
        state.swap_pane_agents("s1", "alpha", "beta");
        let space = state.spaces.iter().find(|s| s.id == "s1").unwrap();
        // slot 0 now holds beta, slot 1 holds alpha — full PaneConfig swapped
        assert_eq!(space.panes[0].id, "beta");
        assert_eq!(space.panes[0].label.as_deref(), Some("label-beta"));
        assert_eq!(space.panes[1].id, "alpha");
        assert_eq!(space.panes[1].label.as_deref(), Some("label-alpha"));
        // shell untouched at slot 2
        assert_eq!(space.panes[2].id, "shell");
    }

    #[test]
    fn cross_row_swap_swaps_pane_config_only_slots_keep_index() {
        // 2x2: panes indices 0,1 (top row) and 2,3 (bottom row)
        let mut state = state_with(space_with_panes(&["a", "b", "c", "d"]));
        state.swap_pane_agents("s1", "a", "d");
        let space = state.spaces.iter().find(|s| s.id == "s1").unwrap();
        // slot 0 (top-left) now holds what was at slot 3 (bottom-right)
        assert_eq!(space.panes[0].id, "d");
        assert_eq!(space.panes[3].id, "a");
    }

    #[test]
    fn noop_when_ids_equal() {
        let mut state = state_with(space_with_panes(&["a", "b"]));
        state.swap_pane_agents("s1", "a", "a");
        let space = state.spaces.iter().find(|s| s.id == "s1").unwrap();
        assert_eq!(space.panes[0].id, "a");
        assert_eq!(space.panes[1].id, "b");
    }

    #[test]
    fn noop_when_pane_id_missing() {
        let mut state = state_with(space_with_panes(&["a", "b"]));
        // first id missing
        state.swap_pane_agents("s1", "ghost", "a");
        // second id missing
        state.swap_pane_agents("s1", "a", "ghost");
        // both missing
        state.swap_pane_agents("s1", "x", "y");
        let space = state.spaces.iter().find(|s| s.id == "s1").unwrap();
        assert_eq!(space.panes[0].id, "a");
        assert_eq!(space.panes[1].id, "b");
    }

    #[test]
    fn noop_when_space_id_missing() {
        let mut state = state_with(space_with_panes(&["a", "b"]));
        state.swap_pane_agents("nonexistent", "a", "b");
        let space = state.spaces.iter().find(|s| s.id == "s1").unwrap();
        assert_eq!(space.panes[0].id, "a");
        assert_eq!(space.panes[1].id, "b");
    }

    #[test]
    fn preserves_unrelated_panes_and_grid_template() {
        let mut state = state_with(space_with_panes(&["a", "b", "c", "shell"]));
        let grid_before = state
            .spaces
            .iter()
            .find(|s| s.id == "s1")
            .unwrap()
            .grid;
        state.swap_pane_agents("s1", "a", "shell");
        let space = state.spaces.iter().find(|s| s.id == "s1").unwrap();
        assert_eq!(space.panes.len(), 4);
        assert_eq!(space.grid, grid_before);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path frontend/Cargo.toml --lib stores::workspace::swap_pane_agents_tests 2>&1 | tail -20`
(If the frontend has no Cargo.toml, run from repo root: `cargo test --workspace stores::workspace::swap_pane_agents_tests 2>&1 | tail -20`)
Expected: compile error — `swap_pane_agents` method not found on `WorkspaceState` (or `no method named swap_pane_agents`).

- [ ] **Step 3: Write the minimal implementation**

Add this method to `impl WorkspaceState` in `frontend/src/stores/workspace.rs`, immediately after `remove_pane_from_space` (line ~102, before `set_spaces`):

```rust
    /// Swap two panes within a space by pane id — full session migration.
    /// The entire `PaneConfig` (including `id`, so the PTY session follows
    /// the agent) trades places between the two slots. Grid slot indices
    /// (and therefore each slot's flex-grow size) are unchanged; only the
    /// values at the two indices swap. Persists via the existing
    /// `update_space`/`save` path. No-op if the space, either pane id is
    /// missing, or the two ids are equal.
    pub fn swap_pane_agents(&mut self, space_id: &str, pane_id_a: &str, pane_id_b: &str) {
        if pane_id_a == pane_id_b {
            return;
        }
        self.update_space(space_id, |space| {
            let ia = space.panes.iter().position(|p| p.id == pane_id_a);
            let ib = space.panes.iter().position(|p| p.id == pane_id_b);
            match (ia, ib) {
                (Some(ia), Some(ib)) => space.panes.swap(ia, ib),
                // one or both ids absent — leave the space untouched
                _ => {}
            }
        });
    }
```

Note: `update_space` calls `self.save()` unconditionally (line ~86), so a successful swap persists, and a no-op swap (ids equal → early return before `update_space`) does not write. The missing-id cases inside `update_space` produce a no-op closure but still call `save()` — that's the same behavior as `add_pane_to_space`/`remove_pane_from_space`, so it's consistent with the existing store contract. (Acceptable: a redundant save on a no-op is harmless; the persistence layer coalesces writes.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace stores::workspace::swap_pane_agents_tests 2>&1 | tail -20`
Expected: PASS — 6 tests, 0 failures.

- [ ] **Step 5: Run the whole workspace test suite to confirm no regressions**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: PASS, no new failures.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/stores/workspace.rs
git commit -m "feat(workspace): swap_pane_agents store method (full session migration)"
```

---

### Task 2: `pill_drag.rs` — `PillDrag` state struct + hit-test interop

**Files:**
- Create: `frontend/src/components/workspace/pill_drag.rs`
- Modify: `frontend/src/components/workspace/mod.rs` (add `pub mod pill_drag;`)

**Interfaces:**
- Consumes: `dioxus::prelude::*` (`Signal`, `use_effect`, `rsx`, `Element`, `#[component]`, `PointerEvent`), `web_sys` / `wasm_bindgen` / `JsCast` (already used in `terminal_grid.rs:2`), `crate::stores::workspace::WorkspaceState` (Signal of `WorkspaceState`), `crate::stores::terminal::use_terminal_store`.
- Produces:
  - `#[derive(Clone, Debug)] pub struct PillDrag { pub source_pane_id: String, pub source_space_id: String, pub source_label: String, pub source_color: String, pub start_x: f64, pub start_y: f64, pub cur_x: f64, pub cur_y: f64, pub moved: bool, pub target_pane_id: Option<String> }`
  - `pub const PILL_DRAG_THRESHOLD_F64: f64 = 4.0;`
  - `pub fn nearest_pane_id_under_point(x: f64, y: f64) -> Option<String>` — JS interop helper that calls `document.elementFromPoint(x, y)` then walks up the DOM to the nearest `[data-pane-id]` and returns its value.

- [ ] **Step 1: Declare the module**

Edit `frontend/src/components/workspace/mod.rs`. Add at the top with the other `pub mod` lines (after `pub mod grid_template;`):

```rust
pub mod pill_drag;
```

- [ ] **Step 2: Write the failing test for the hit-test helper's pure-Rust path**

The `nearest_pane_id_under_point` function does DOM work that's hard to unit-test in WASM, but the **DOM walk** (walk up from an element to the nearest `[data-pane-id]` ancestor, return its value) is pure logic over `web_sys::Element`. We test the walk via a small extracted helper `walk_to_data_pane_id`.

Append to the new `frontend/src/components/workspace/pill_drag.rs`:

```rust
//! Pane-pill drag-and-drop: `PillDrag` session state, the fullscreen pointer
//! overlay (`PillDragOverlay`), the floating ghost (`PillDragGhost`), and the
//! `document.elementFromPoint` drop-target hit-test.
//!
//! Mirrors the proven `DragOverlay` resize pattern in `terminal_grid.rs`:
//! a fullscreen fixed overlay mounts on pointerdown and owns pointermove/ up,
//! so WKWebView never loses the drag when the cursor leaves the source pill.
//! Uses PointerEvents (not MouseEvent) for unified mouse/touch/stylus.

use dioxus::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Document, Element, HtmlElement};

/// Drag threshold in CSS pixels. Below this, a pointerdown is treated as a
/// click (lets double-click rename + icon-button clicks pass through).
pub const PILL_DRAG_THRESHOLD: f64 = 4.0;

/// A live pane-pill drag session. `None` when idle.
#[derive(Clone, Debug)]
pub struct PillDrag {
    /// Pane id being dragged (the source pill's pane).
    pub source_pane_id: String,
    /// Space the drag started in. Drops only swap within the same space.
    pub source_space_id: String,
    /// Pill text shown on the ghost.
    pub source_label: String,
    /// Agent color shown on the ghost (CSS color string).
    pub source_color: String,
    /// pointer-down position (client coords).
    pub start_x: f64,
    pub start_y: f64,
    /// Latest pointer position (client coords), updated on pointermove.
    pub cur_x: f64,
    pub cur_y: f64,
    /// Has the pointer crossed the threshold? If false, pointerup is a click
    /// (no swap, no drag preview shown).
    pub moved: bool,
    /// Hit-tested target pane id (same space, not the source), or None.
    pub target_pane_id: Option<String>,
}

/// Walk up the DOM from `el` to the nearest ancestor (inclusive) carrying a
/// `data-pane-id` attribute, returning its value. Pure function over the DOM
/// — extracted so the walk logic is unit-testable in isolation.
///
/// `data-pane-id` is set on pane-wrapper divs by `terminal_grid.rs` Task 4.
fn walk_to_data_pane_id(el: Option<Element>) -> Option<String> {
    let mut node = el;
    while let Some(elem) = node {
        if let Ok(value) = elem.get_attribute("data-pane-id").ok().flatten() {
            return Some(value);
        }
        node = elem.parent_element();
    }
    None
}
```

Then add the test module at the file end:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_to_data_pane_id_none_for_none() {
        assert_eq!(walk_to_data_pane_id(None), None);
    }
}
```

Note: deeper DOM-walk tests require a live DOM (wasm-bindgen-test), which is out of scope for this plan's Rust-native tests. The `None`->`None` test guards the trivial path; the live-DOM behavior is covered by manual + e2e testing (Task 6).

- [ ] **Step 3: Run the test to verify it fails (compile error)**

Run: `cargo test --workspace components::workspace::pill_drag 2>&1 | tail -20`
Expected: FAIL — module `pill_drag` not declared yet (it is now, via Step 1) or `walk_to_data_pane_id` not found. Run again:
`cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: compiles (the file has the function). The test should now compile and pass once the module is wired. If it doesn't compile, fix imports first.

- [ ] **Step 4: Add `nearest_pane_id_under_point` (the JS interop)**

Append to `frontend/src/components/workspace/pill_drag.rs` (above the test module):

```rust
/// Find the topmost pane under screen point `(x, y)` by calling
/// `document.elementFromPoint` then walking up to the nearest
/// `[data-pane-id]` ancestor. Returns its value, or `None` if the point is
/// not over any pane (e.g. over the sidebar, the titlebar, or the
/// drag-overlay scrim itself).
///
/// Note: `document.elementFromPoint` returns the topmost element at the
/// point — during a drag this is usually the `PillDragOverlay` scrim
/// (it has `pointer-events: auto` to receive events). The scrim sets
/// `data-no-drop` on itself; callers MUST transparently pass through the
/// scrim by hiding it from hit-testing (see `find_drop_target`, which
/// temporarily disables the scrim before calling this). Implementation of
/// that pass-through lives in Task 4's overlay; this pure helper does only
/// the DOM walk.
pub fn nearest_pane_id_under_point(x: f64, y: f64) -> Option<String> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let element = document.element_from_point(x as f32, y as f32);
    walk_to_data_pane_id(element)
}
```

- [ ] **Step 5: Build to verify it compiles**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -25`
Expected: compiles clean. (`element_from_point` takes `f32` in web-sys; the cast is intentional. unused import warnings for `Closure`/`Document`/`HtmlElement` are OK for now — they're used by `PillDragOverlay` in Task 3.)

- [ ] **Step 6: Run the test**

Run: `cargo test --workspace components::workspace::pill_drag 2>&1 | tail -15`
Expected: PASS — `walk_to_data_pane_id_none_for_none`.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/workspace/pill_drag.rs frontend/src/components/workspace/mod.rs
git commit -m "feat(workspace): pill_drag module — PillDrag state + hit-test interop"
```

---

### Task 3: `PillDragOverlay` + `PillDragGhost` components

**Files:**
- Modify: `frontend/src/components/workspace/pill_drag.rs` (add `PillDragOverlay` and `PillDragGhost` components)
- Modify: `frontend/public/styles.css` (add `.is-dnd-target` ring + `.dnd-ghost` pill styles)

**Interfaces:**
- Consumes: `PillDrag`, `nearest_pane_id_under_point`, `PILL_DRAG_THRESHOLD` (Task 2), `Signal<Option<PillDrag>>`, `Signal<WorkspaceState>` (workspace store), `crate::stores::workspace::use_workspace_store`, `crate::stores::terminal::use_terminal_store`. `PillDragOverlay` props: `{ drag: Signal<Option<PillDrag>>, workspace: Signal<WorkspaceState> }`. `PillDragGhost` props: `{ drag: Signal<Option<PillDrag>> }`.
- Produces: `pub fn PillDragOverlay(props: PillDragOverlayProps) -> Element` and `pub fn PillDragGhost(props: PillDragGhostProps) -> Element`, both `#[component]`. These render `rsx!`. `PillDragOverlay` reads `drag`, and on `pointerup` with `moved && target != source` calls `workspace.write().swap_pane_agents(space_id, src, tgt)` then `terminal_store.write().set_active(src)`.

- [ ] **Step 1: Add the CSS (no test for CSS — verified manually Task 6)**

Append to `frontend/public/styles.css`:

```css
/* ── Pane pill drag-and-drop swap ───────────────────────────────────────── */

/* Highlighted drop target — gold ring around the pane wrapper currently
   under the dragged pill. */
.pane-astrolabe-mark.is-dnd-target {
    box-shadow: inset 0 0 0 2px var(--accent), 0 0 18px color-mix(in srgb, var(--accent) 35%, transparent);
    border-color: var(--accent);
    transition: box-shadow var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease);
}

/* Floating ghost pill that follows the cursor during a drag. */
.dnd-ghost {
    position: fixed;
    z-index: 10000;
    pointer-events: none;
    transform: translate(-50%, calc(-50% - 14px));
    font-family: var(--font-ui);
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text);
    background: linear-gradient(180deg, rgba(14,22,40,0.92), rgba(8,11,22,0.96));
    border: 1px solid var(--accent);
    border-radius: var(--radius-pill);
    box-shadow: 0 6px 20px rgba(0,0,0,0.45), 0 0 14px color-mix(in srgb, var(--accent) 45%, transparent);
    padding: 4px 12px;
    white-space: nowrap;
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
}

/* Fullscreen overlay that owns pointermove/up during a drag. Transparent;
   sits above all content (z-index 9999) so the cursor always lands on it. */
.dnd-overlay {
    position: fixed;
    top: 0; left: 0; right: 0; bottom: 0;
    z-index: 9999;
    background: transparent;
}
.dnd-overlay.is-grabbing { cursor: grabbing; }
```

- [ ] **Step 2: Add `PillDragOverlay` to `pill_drag.rs`**

Insert above the `#[cfg(test)] mod tests` block in `frontend/src/components/workspace/pill_drag.rs`:

```rust
// ---------------------------------------------------------------------------
// PillDragOverlay — fullscreen pointer-capturing scrim
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct PillDragOverlayProps {
    pub drag: Signal<Option<PillDrag>>,
    pub workspace: Signal<crate::stores::workspace::WorkspaceState>,
}

#[component]
pub fn PillDragOverlay(props: PillDragOverlayProps) -> Element {
    let mut drag = props.drag;
    let workspace = props.workspace;

    let onpointermove = move |e: PointerEvent| {
        let coords = e.data.client_coordinates();
        let mut current = match drag.read().clone() {
            Some(d) => d,
            None => return,
        };
        // First move past the threshold commits the drag.
        if !current.moved {
            let dx = coords.x - current.start_x;
            let dy = coords.y - current.start_y;
            if dx * dx + dy * dy < PILL_DRAG_THRESHOLD * PILL_DRAG_THRESHOLD {
                return;
            }
            current.moved = true;
        }
        current.cur_x = coords.x;
        current.cur_y = coords.y;
        // Hit-test: temporarily hide the overlay from elementFromPoint by
        // setting pointer-events:none on the scrim. We restore it next frame.
        // (The scrim is THIS element; toggling pointer-events would drop the
        // subsequent pointermove events, so we instead hide it via a
        // `data-no-drop` opt-out and have `find_drop_target` skip it.)
        current.target_pane_id = find_drop_target(coords.x, coords.y, &current);
        drag.set(Some(current));
    };

    let onpointerup = move |e: PointerEvent| {
        let finished = drag.read().clone();
        drag.set(None);
        let Some(d) = finished else { return; };
        if !d.moved {
            return; // a click, not a drag
        }
        if let Some(target) = d.target_pane_id.as_ref() {
            if target != &d.source_pane_id {
                {
                    let mut ws = workspace.write();
                    ws.swap_pane_agents(&d.source_space_id, &d.source_pane_id, target);
                }
                // Focus the moved agent's new slot (id follows it; set_active
                // is a pane-id, not a slot, so it stays valid post-swap).
                let mut term = crate::stores::terminal::use_terminal_store().write();
                term.set_active(d.source_pane_id.clone());
            }
        }
        // dropping outside any pane, or on self → no-op, just clears.
    };

    let is_grabbing = drag
        .read()
        .as_ref()
        .map(|d| d.moved)
        .unwrap_or(false);
    let cursor = if is_grabbing { " is-grabbing" } else { "" };

    rsx! {
        div {
            class: "dnd-overlay{cursor}",
            "data-no-drop": "true",
            onpointermove: onpointermove,
            onpointerup: onpointerup,
        }
    }
}
```

- [ ] **Step 3: Add `find_drop_target` + `PillDragGhost` to `pill_drag.rs`**

Insert above `PillDragOverlay` (so it's defined before use; or below if you prefer — Rust doesn't care about order in a module):

```rust
/// Hit-test the drop target at `(x, y)`, transparently passing through the
/// drag overlay scrim. The scrim carries `data-no-drop`; while it's the
/// topmost element under the cursor, we temporarily set
/// `pointer-events:none` on it so `elementFromPoint` sees through to the
/// pane below, then restore it.
fn find_drop_target(x: f64, y: f64, drag: &PillDrag) -> Option<String> {
    let window = web_sys::window()?;
    let document = window.document()?;
    // Repeatedly find the top element; if it's the scrim, hide it and retry.
    // Cap iterations to avoid infinite loops on unexpected DOM shapes.
    for _ in 0..8 {
        let top = document.element_from_point(x as f32, y as f32);
        match &top {
            Some(el) => {
                if is_overlay_scrim(el) {
                    // hide the scrim so the next call sees through it
                    if let Ok(html) = el.clone().dyn_into::<HtmlElement>() {
                        let prev = html.style_pointer_events();
                        html.style_set_pointer_events("none");
                        let result = nearest_pane_id_under_point(x, y);
                        html.style_set_pointer_events(&prev);
                        // filter: must be same space, not the source
                        return filter_target(result, drag);
                    }
                    // fall through and retry once if pointer-events toggle failed
                    continue;
                }
                // hit something that isn't the scrim — walk up from it
                return filter_target(walk_to_data_pane_id(top), drag);
            }
            None => return None,
        }
    }
    None
}

/// True if `el` is the `PillDragOverlay` scrim (carries `data-no-drop`).
fn is_overlay_scrim(el: &Element) -> bool {
    el.get_attribute("data-no-drop").ok().flatten()
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Keep the target only if it's in the same space (test) and not the source.
/// We can't know the target's space id from its pane id alone, so we accept
/// any pane id in the document; the space-mismatch case is rare and harmless
/// (the swap would no-op in the store if the id isn't in this space).
fn filter_target(found: Option<String>, drag: &PillDrag) -> Option<String> {
    found.and_then(|id| {
        if id == drag.source_pane_id {
            None
        } else {
            Some(id)
        }
    })
}

// ---------------------------------------------------------------------------
// PillDragGhost — floating label that follows the cursor
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
pub struct PillDragGhostProps {
    pub drag: Signal<Option<PillDrag>>,
}

#[component]
pub fn PillDragGhost(props: PillDragGhostProps) -> Element {
    let drag = props.drag;
    let d = match drag.read().clone() {
        Some(d) if d.moved => d,
        _ => return rsx! {},
    };
    rsx! {
        div {
            class: "dnd-ghost",
            style: "left: {d.cur_x:.0}px; top: {d.cur_y:.0}px;",
            "{d.source_label}"
        }
    }
}
```

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -30`
Expected: compiles clean. Common fixes if it fails:
  - `PointerEvent` not imported — it's in `dioxus::prelude::*` (already wildcard-imported). If `data.client_coordinates()` doesn't exist on PointerData, switch to `e.page_coordinates()` or read `client_x()`/`client_y()` — check the existing `ColDivider`'s `e.data.client_coordinates()` usage at `terminal_grid.rs:913`, which is `MouseEvent`; for `PointerEvent` the same `data.client_coordinates()` API should exist on `PointerData` (Dioxus 0.7). If not, use `e.client_coordinates()`.
  - `style_pointer_events` / `style_set_pointer_events` — these are web-sys `CssStyleDeclaration` methods; if naming differs, use `element.set_attribute("style", "pointer-events: none;")` instead, restoring by removing the attribute. Prefer the `dyn_into::<HtmlElement>()` `style()` accessor if available: `html.style().set_property("pointer-events", "none")`.

- [ ] **Step 5: Run the existing tests to confirm no regressions**

Run: `cargo test --workspace 2>&1 | tail -15`
Expected: PASS — same count as after Task 1, plus the Task 2 test.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/workspace/pill_drag.rs frontend/public/styles.css
git commit -m "feat(workspace): PillDragOverlay + PillDragGhost (pointer drag, hit-test, swap on drop)"
```

---

### Task 4: Wire drag into `WorkspaceGrid` + `PaneItem`

**Files:**
- Modify: `frontend/src/components/workspace/terminal_grid.rs`:
  - `WorkspaceGrid` (~line 88-130): add `pill_drag` signal; thread `pill_drag` + `space_id` into `PaneItem`; render `PillDragOverlay` + `PillDragGhost` when dragging; mark pane-wrapper divs with `data-pane-id` and conditional `is-dnd-target` class.
  - `PaneItem` (~line 285-300): accept new props `pill_drag` and `space_id`; add `onpointerdown` to the pill header `div` (line ~513); add `onpointerdown: |e| e.stop_propagation()` to the fullscreen + close icon buttons (lines ~575, ~593).

**Interfaces:**
- Consumes: `PillDrag`, `PillDragOverlay`, `PillDragGhost`, `PILL_DRAG_THRESHOLD` from `pill_drag`; `use_workspace_store`, `set_active` from `use_terminal_store`, `get_agent_label`, `get_agent_color` from `crate::utils::agent_commands`.
- Produces: rendered pill headers that start a drag on `onpointerdown` (recording source pane id, label, color, start coords), pane wrappers tagged `data-pane-id={pane.id}` and ring-highlighted when they are the current drop target, and the overlay/ghost mounted while `pill_drag.is_some()`.

- [ ] **Step 1: Mark pane wrappers with `data-pane-id` + `is-dnd-target`**

In `frontend/src/components/workspace/terminal_grid.rs`, find the pane-wrapper `rsx!` block (~line 209-216):

```rust
                                    rsx! {
                                        div {
                                            key: "pane-wrap-{space.id}-{pane.id}",
                                            class: "pane-astrolabe-mark",
                                            style: "{wrapper_style}",
```

Replace with (adds `data-pane-id`, computes the target class from `pill_drag`):

```rust
                                    let pill_drag_state = pill_drag.read();
                                    let is_target = pill_drag_state
                                        .as_ref()
                                        .and_then(|d| d.target_pane_id.as_ref())
                                        == Some(&pane.id);
                                    drop(pill_drag_state);
                                    let target_class = if is_target {
                                        "pane-astrolabe-mark is-dnd-target"
                                    } else {
                                        "pane-astrolabe-mark"
                                    };
                                    rsx! {
                                        div {
                                            key: "pane-wrap-{space.id}-{pane.id}",
                                            class: "{target_class}",
                                            "data-pane-id": "{pane.id}",
                                            style: "{wrapper_style}",
```

(Surrounding code unchanged. `pill_drag` signal is added to `WorkspaceGrid` in Step 3 below.)

- [ ] **Step 2: Thread `pill_drag` + `space_id` into `PaneItemProps`**

In the `PaneItemProps` struct (~line 286), add fields:

```rust
#[derive(Props, Clone, PartialEq)]
struct PaneItemProps {
    space_id: String,
    pane_id: String,
    cwd: String,
    agent_type: AgentType,
    is_shell: bool,
    resume_id: Option<String>,
    resume_cmd: Option<String>,
    resume_dismissed: Option<bool>,
    custom_cmd: Option<String>,
    custom_agent_id: Option<String>,
    label: Option<String>,
    fullscreen_pane_id: Signal<Option<String>>,
    // NEW — drag-and-drop:
    pill_drag: Signal<Option<crate::components::workspace::pill_drag::PillDrag>>,
}
```

At the call site (~line 233-245, the `PaneItem { ... }` invocation in `WorkspaceGrid`), add the two new props:

```rust
                                            PaneItem {
                                                key: "pane-{space.id}-{pane.id}",
                                                space_id: space.id.clone(),
                                                pane_id: pane.id.clone(),
                                                cwd: space.dir.clone(),
                                                agent_type: pane.agent_type.clone(),
                                                is_shell: matches!(pane.agent_type, AgentType::Shell | AgentType::Custom),
                                                resume_id: pane.resume_id.clone(),
                                                resume_cmd: pane.resume_cmd.clone(),
                                                resume_dismissed: pane.resume_dismissed.clone(),
                                                custom_cmd: pane.custom_cmd.clone(),
                                                custom_agent_id: pane.custom_agent_id.clone(),
                                                label: pane.label.clone(),
                                                fullscreen_pane_id: fullscreen_pane_id,
                                                pill_drag: pill_drag,
                                            }
```

- [ ] **Step 3: Add the `pill_drag` signal + overlay/ghost render in `WorkspaceGrid`**

In `WorkspaceGrid` (~line 121-127), alongside the other signals, add:

```rust
    let pill_drag: Signal<Option<crate::components::workspace::pill_drag::PillDrag>> =
        use_signal(|| None);
    let workspace_for_drag = use_workspace_store();
```

At the end of the `WorkspaceGrid` `rsx!`, after the existing `if drag.cloned().is_some() { DragOverlay { .. } }` block (~line 251), add:

```rust
        // Pill drag-and-drop — overlay owns pointermove/up; ghost follows cursor.
        // Rendered outside the grid rows so they remain non-interactive-barrier.
        if let Some(_) = pill_drag.read().as_ref() {
            crate::components::workspace::pill_drag::PillDragOverlay {
                drag: pill_drag,
                workspace: workspace_for_drag,
            }
        }
        crate::components::workspace::pill_drag::PillDragGhost {
            drag: pill_drag,
        }
```

(Note: `PillDragGhost` renders nothing internally when `drag` is `None` or `!moved`, so unconditionally mounting it is harmless and avoids a key-flip. The overlay only mounts when a drag is in flight.)

- [ ] **Step 4: Add `onpointerdown` to the pill header in `PaneItem`**

In `PaneItem` (~line 510-518), the pill header is the inner rounded `div`. Add `onpointerdown`. The pill header block currently starts:

```rust
            // Pill header — distinct, refined, sits inside the pane
            div {
                style: "flex-shrink: 0; padding: 6px 8px 0 8px;",

                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 4px 12px; background: linear-gradient(180deg, rgba(14,22,40,0.62), rgba(8,11,22,0.70)); border: 1px solid var(--line-lapis); box-shadow: inset 0 1px 0 var(--lit-edge); border-radius: 999px; flex-shrink: 0;",
```

Add `onpointerdown` to the inner pill `div` (the rounded one), capturing source data. Insert immediately before the existing `style:` line on that inner div:

```rust
                    onpointerdown: move |e: dioxus::prelude::PointerEvent| {
                        // Only primary button / contact starts a drag.
                        if e.data.buttons() != 1 && !e.data.is_primary() {
                            return;
                        }
                        let coords = e.data.client_coordinates();
                        let label_text = display_label.clone();
                        let color = crate::utils::agent_commands::get_agent_color(&props.agent_type);
                        pill_drag.set(Some(crate::components::workspace::pill_drag::PillDrag {
                            source_pane_id: props.pane_id.clone(),
                            source_space_id: props.space_id.clone(),
                            source_label: label_text,
                            source_color: color,
                            start_x: coords.x,
                            start_y: coords.y,
                            cur_x: coords.x,
                            cur_y: coords.y,
                            moved: false,
                            target_pane_id: None,
                        }));
                    },

                    style: "display: flex; align-items: center; gap: 8px; padding: 4px 12px; background: linear-gradient(180deg, rgba(14,22,40,0.62), rgba(8,11,22,0.70)); border: 1px solid var(--line-lapis); box-shadow: inset 0 1px 0 var(--lit-edge); border-radius: 999px; flex-shrink: 0;",
```

(Place this `onpointerdown` handler on the rounded inner pill `div`, not the outer `flex-shrink: 0; padding: 6px 8px 0 8px;` container — we want only the pill itself to initiate a drag, not the resume banner or shell body.)

`display_label` is computed earlier inside `PaneItem` (used by the title span at ~line 555). Confirm it's in scope; if it's named `left_label` or computed inline, use that variable instead. (Check around `terminal_grid.rs:460-480`; the existing code uses `display_label` at line ~560 in the title span render. Use the same binding.)

- [ ] **Step 5: Stop propagation of pointerdown on the icon buttons**

The fullscreen button (~line 575) and close button (~line 593) currently have `onclick` with `e.stop_propagation()`. Add `onpointerdown: |e| e.stop_propagation()` to each so grabbing the icons never starts a pill drag. For the fullscreen button:

```rust
                        button {
                            class: "icon-btn",
                            title: if is_fullscreen { "Exit Fullscreen" } else { "Fullscreen" },
                            onpointerdown: move |e: dioxus::prelude::PointerEvent| {
                                e.stop_propagation();
                            },
                            onclick: move |e| {
                                e.stop_propagation();
                                if is_fullscreen {
                                    fullscreen_pane_id.set(None);
                                } else {
                                    fullscreen_pane_id.set(Some(pane_id_for_fullscreen.clone()));
                                }
                            },
```

Apply the identical `onpointerdown` to the close button (line ~593). Also check for any other `icon-btn` inside the pill header (`btn-pill` close buttons at lines ~690, ~733, ~778 if present) and add the same `onpointerdown` stop-propagation handler to each — the rule is: any interactive control inside the pill must not initiate a drag.

- [ ] **Step 6: Build and fix compile errors**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -30`
Expected: compiles. Likely fixes:
  - `e.data.buttons()` / `e.data.is_primary()` — Dioxus `PointerData` API. If `is_primary()` isn't a method, check `e.data` for `primary()` field or use `e.data.button() == 0`. Verify the actual API in `frontend/` by grepping `grep -rn "PointerEvent\|pointer_data\|buttons()" frontend/src | head`. If the API differs, adapt — the intent is "only start a drag on the primary pointer's button-down."
  - `client_coordinates` on `PointerData` — should match the `MouseEvent` usage at line 913. If named differently on PointerData, switch to `e.page_coordinates()` or `e.client_coordinates()`.
  - `display_label` not in scope at the pointer handler — hoist its binding above the pill header `div`, or recompute it inline (it's a `String` built from `resolve_pane_label`).
  - `get_agent_color` import — already imported at `terminal_grid.rs:16`. Good.

- [ ] **Step 7: Run unit tests + build the dist to verify WASM compiles**

Run: `cargo test --workspace 2>&1 | tail -10` then `bash frontend/build-dist.sh --debug 2>&1 | tail -20`
Expected: tests pass; dist build succeeds (WASM compiles, `index.html` regenerated).

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components/workspace/terminal_grid.rs
git commit -m "feat(workspace): wire pill pointer-drag into WorkspaceGrid + PaneItem"
```

---

### Task 5: Build verification + e2e test plan (manual smoke)

**Files:**
- No code changes. This task verifies the build runs and the manual smoke test passes. The e2e test is documented (not written as an automated test) because tauri-webdriver pointer-action synthesis for PointerEvents is unreliable (`tauri-wd` element-reference bug — see CLAUDE.md E2E section); it's left as a documented manual test until pointer-event dispatch is verified.

**Interfaces:**
- Consumes: the full app (`cargo run --manifest-path src-tauri/Cargo.toml`).

- [ ] **Step 1: Clean rebuild + run unit tests**

Run: `cargo test --workspace 2>&1 | tail -15`
Expected: all tests pass, including `swap_pane_agents_tests` (6) and `walk_to_data_pane_id_none_for_none` (1).

- [ ] **Step 2: Build the debug app**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15`
Expected: builds without errors or new warnings.

- [ ] **Step 3: Launch the app**

Run: `cargo run --manifest-path src-tauri/Cargo.toml` (in a terminal you can keep alive, or background it).
Expected: app window appears.

- [ ] **Step 4: Manual smoke test — the user's exact case**

In the running app:
1. Create a space with a 2-top / 1-bottom layout (use the grid template selector to get `1x2` then add a third pane → top row 2, bottom row 1; or whatever the template selector yields for 3 panes). Add two agents (e.g. Claude, Gemini) to the top row and a Shell to the bottom. Run each so they have a live PTY.
2. Press-and-drag the top-left pill onto the bottom Shell pill and release.
3. **Expected:** the top-left agent now renders at the bottom (full-width), the Shell now renders at top-left (50%). The pill labels swapped. The just-moved agent (top-left agent at bottom) is the active/focused pane (gold focus ring).
4. **Expected:** scrollback at both panes is reset (xterm remounted), but the running processes (if any) continue uninterrupted at their new positions. Output resumes rendering.
5. Drag a pill and **release over empty space / the sidebar** (not on any pane). **Expected:** no swap; pills return to place; nothing changes.
6. Press-and-release a pill **without moving** (a pure click on the pill). **Expected:** no swap; the pane is selected (focus ring); a double-click still opens rename edit on the title (unchanged behavior).
7. Press on the **fullscreen icon** and the **close icon** in the pill header and drag. **Expected:** no drag starts; the buttons' normal click behavior fires (fullscreen toggles, close removes the pane).
8. After a swap, **close one pane**. **Expected:** no panic, no stale `pill_drag`; the active pane reassigned as before.

- [ ] **Step 5: Document the manual run**

Record the results of Step 4 in the PR description (a checklist with the 8 cases). If any case fails, file the specific failure as a follow-up task and fix before merging.

- [ ] **Step 6: Final commit (if any doc updates)**

If you added notes to the PR description only, no commit. If you added a `docs/` note, commit it:

```bash
git add docs/  # if applicable
git commit -m "docs: pane-pill drag-swap manual verification notes"
```

---

### Task 6: Push the branch + open the PR

**Files:** none.

- [ ] **Step 1: Push the branch**

```bash
git push -u origin feat/pane-pill-drag-swap
```

- [ ] **Step 2: Open the PR to `main`**

Use `gh pr create --base main` with a body that includes:
- One-line summary: drag a workspace pane pill onto another to swap agents (full session migration).
- The 8-case manual smoke checklist from Task 5.
- A "Test plan" section with TODOs ticked when verified.
- Link to the spec `docs/superpowers/specs/2026-07-04-pane-pill-drag-swap-design.md`.

```bash
gh pr create --base main --title "feat(workspace): drag-and-drop pane pill swap (full session migration)" --body "..."
```

End the PR body with the attribution line per the global git-workflow rules.
