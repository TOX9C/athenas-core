# Pane-Pill Drag-and-Drop Swap — Design

**Date:** 2026-07-04
**Branch target:** new branch off `main` → PR to `main`
**Status:** Spec — pending user review

---

## 1. Goal

Drag an agent's **pane pill** (the rounded header inside each workspace pane
that shows the agent label / title) and drop it onto another pane's pill to
**swap the two agents**. Each agent takes the grid slot — and therefore the
size — of the other.

User's example: in a 2-top / 1-bottom grid (two agents share the top row at
50% width each, one shell takes the bottom row full-width), grabbing the
top-left pill and dropping it on the bottom pill moves the top-left agent to
the bottom (it becomes full-width) and moves the bottom agent to the top-left
slot (it becomes 50%). **The agents trade identities; sizes stay bound to
slots.**

## 2. Non-goals

- No "six-dot" grip handle. The **entire pill** is the grab surface.
- No pane-to-pane position reordering distinct from identity (we don't move
  empty slots around — there is always a 1:1 identity swap).
- No cross-space dragging (pills live within one space; you can't drag a pill
  from Space A into Space B).
- No drag of the swarm-board constellation stars or the sidebar agent list in
  this iteration — only the **workspace pane pills** in `terminal_grid.rs`.
  (Scope is deliberately the one place agents visibly coexist in a grid.)

## 3. Confirmed decisions (from brainstorming)

| Decision | Choice |
|---|---|
| Swap semantics | **Approach A** — swap identities in `Space.panes[]`; slots keep their sizes |
| Drag technique | **Pointer-event drag** (`pointerdown`/`move`/`up`) with a floating ghost; **no HTML5 DnD**, no `dataTransfer` |
| Grab surface | Whole pill header — no grip icon |
| Branch | New branch off `main` → PR to `main` |

## 4. Why pointer-events, not HTML5 DnD

The previous attempt used native `draggable` + `dataTransfer` and was reverted
after three WKWebView-specific bugs (see git history `55587d5e`, `e80e1f5b`,
`6dfacd1a` and the revert sweep ending at `9a1a6824`):

1. WKWebView needs `ondragenter` on the cell wrapper (not just `ondragover`)
   to keep the drop target armed.
2. Element-reference bugs in `tauri-webdriver` for `Node.contains()` during
   DnD hit-testing.
3. `text/plain` payload fallbacks were required because custom MIME types
   weren't delivered reliably.

Pointer-event dragging sidesteps **all three**: there is no `dataTransfer`,
no drop-target arming protocol, and target hit-testing is done with our own
geometry (`document.elementFromPoint` or client-rect checks), not the
browser's DnD state machine. This is the same approach the existing
`ColDivider` / `RowDivider` resize drag (`DragKind::Col` / `Row`,
`DragInfo`) already uses successfully in `terminal_grid.rs` — so it is
proven in this exact WKWebView surface.

## 5. Data model & mutation

### 5.1 What gets swapped

`Space.panes: Vec<PaneConfig>` is read left→right, row by row, to place
panes into the flexbox grid (`terminal_grid.rs:163-176`). Array index
**is** the grid slot. So a swap of two panes is a swap of two entries in
this Vec.

Swapping **identities with session migration** means **the entire
`PaneConfig` swaps** between the two slots — `id` and all agent fields move
together. The slot keeps only its grid position (array index) and its
flex-grow weight; everything the user perceives as "the agent" (its PTY
session id, agent type, label, resume state) travels to the new slot.

**CONFIRMED DECISION (§11.1):** `id` **moves with the agent** — full
session migration. The user explicitly accepted the cost (scrollback loss
on swap, below).

This works because **`pty_spawn` is idempotent by id**
(`crates/athena-terminal/src/session.rs:295` — "already exists, returning
existing"; verified by tests `spawn_with_same_id_returns_existing` and
`spawn_concurrent_same_id_races_to_single_session`). When `XtermMount`
remounts at the new slot with the swapped `pane_id`, its `pty_spawn` call
returns the **existing** PTY session — no duplicate process, no re-exec.
The backend PTY is the durable home; the frontend xterm.js renderer is a
transient view onto it.

**Honest cost of this choice:** the xterm.js renderer instance is destroyed
and recreated on remount (Dioxus keys `XtermMount` as `xterm-{pane_id}`,
`terminal_grid.rs:30`). xterm.js stores scrollback **in the renderer
instance**, not the PTY, so **scrollback is lost on each swap** — the
fresh xterm reattaches to the live PTY and shows new output as it arrives,
but prior scrollback is gone. The running process itself is uninterrupted.
This is the trade-off the user explicitly accepted. (Mitigation if it
proves annoying later: a "snapshot scrollback to PTY" path — out of scope
here.)

### 5.2 The store mutation

A new method on `WorkspaceStore`:

```rust
/// Swap two panes within a space, by pane id — full migration: the entire
/// PaneConfig (including `id`) trades places. Persists via the existing
/// workspace-save path. No-op (logs) if either id is missing or equal.
pub fn swap_pane_agents(&mut self, space_id: &str, pane_id_a: &str, pane_id_b: &str)
```

Implementation outline (immutable update, per coding-style rules):

1. `self.update_space(space_id, |space| { ... })` — reuse the existing
   mutation hook used by rename (`terminal_grid.rs:545-554`).
2. Inside, find indices `ia`, `ib` of the two pane ids. If not found or
   equal, return early.
3. `space.panes.swap(ia, ib);` — the standard Vec::swap moves both whole
   `PaneConfig` values (id + all fields) in one atomic op. No field surgery.

### 5.3 Registry & active-session handling (frontend)

Because `id` moves, the `TerminalRegistry` (keyed by pane id) and
`TerminalStore.known_pane_ids` / `active_session_id` need no remap:

- The registry/`known_pane_ids` set is keyed by pane **id**, and the ids
  themselves don't change — only which **slot** renders which id. So the
  same id now renders at slot B; its registry signal and PTY follow it.
- `active_session_id` is a pane id (not a slot), so it stays valid and
  points to whichever pane is now wherever — no reassignment needed.
- The xterm renderer at slot A remounts (key changes from `xterm-α` to
  `xterm-β`); the new `XtermMount` calls `pty_spawn(β, ...)`, which returns
  the **existing** PTY β. Slot B symmetrically remounts onto the existing
  PTY α. Both PTYs are uninterrupted. **Scrollback is lost** (xterm.js
  renderer-side state, §“Honest cost” in §5.1).

No backend command, no Tauri IPC change, no registry renames. Pure frontend
`Vec::swap` + the existing xterm keying does all the lifting.

This is a pure data mutation in the store; the grid's `col_widths` /
`row_heights` runtime signals are **untouched** (sizes stay with slots —
Approach A).

### 5.4 Running agents — allowed (confirmed §11.2)

Swapping is allowed even when an agent is mid-run. Because `id` moves with
the agent (§5.1), the running process follows the agent to the new slot —
the pill and the underlying process stay in agreement (no label/process
mismatch). The only cost is the xterm renderer remount at both slots
(§5.1 "Honest cost").

## 6. Drag state model

A single `use_signal` in `WorkspaceGrid` holds the drag session:

```rust
#[derive(Clone, Debug)]
struct PillDrag {
    source_pane_id: String,   // pane being dragged
    source_space_id: String,  // space it belongs to (drag is within one space)
    source_label: String,     // pill text shown on the ghost
    source_color: String,     // agent color shown on the ghost
    pointer_id: Option<u32>,  // for pointer capture / multi-touch guard
    start_x: f64,
    start_y: f64,
    cur_x: f64,               // updated on pointermove
    cur_y: f64,
    moved: bool,              // crossed the drag threshold?
    target_pane_id: Option<String>, // hit-tested drop target under cursor
}
```

Modeled on the existing `DragInfo` for resize (`terminal_grid.rs:31-48`),
but a separate type — pill-drag and resize-drag never coexist, but keeping
them distinct avoids overloading `DragKind`.

**Drag threshold:** 4px (matches typical WKWebView slop). Below it, the
pointerdown is treated as a click (lets double-click rename and button
clicks pass through — §7.3).

**Pointer capture:** on `pointerdown` we do **not** capture immediately
(capturing on the pill would swallow the dblclick that renames). Instead we
listen on `window` for `pointermove`/`pointerup` once a drag starts. Dioxus
0.7 global listeners via `use_effect` + `window` event handlers
(`dioxus::events` + `web_sys::window`), the same pattern already used for
global keybindings elsewhere in `lib.rs`.

## 7. Component changes

### 7.1 `WorkspaceGrid` (`terminal_grid.rs`)

- Add `pill_drag: use_signal(|| None::<PillDrag>)`.
- Pass `pill_drag` down to each `PaneItem` as a prop (alongside
  `fullscreen_pane_id`).
- Render a `PillDragGhost { drag: pill_drag }` sibling when `pill_drag` is
  `Some` (analogous to the existing `DragOverlay` for resize at line ~253).

### 7.2 `PaneItem` pill header — pointer handlers

The pill header `div` is at `terminal_grid.rs:513` (the inner rounded
container that holds the title + badge + buttons). Add to that `div`:

- `onpointerdown` — record start position, source pane id, label, color, and
  pointer id into `pill_drag` (in `moved: false` "pending" state). **Do not
  preventDefault** (we need click/dblclick to still work until a drag is
  detected).
- `onpointermove` — only fires while hovering; the **window-level**
  move handler (§7.4) is authoritative. (Keep the pill-level handler as a
  no-op or remove; the window listener covers it.)
- No `ondragstart` etc. — we are not using HTML5 DnD.

Drop-target affordance: the pane **wrapper** `div` (`pane-wrap-...`, line
~209) gets `data-pane-id` and a conditional class
`is-dnd-target` when `pill_drag.target_pane_id == Some(pane.id)` and it
isn't the source. CSS renders a gold ring (reuse `--accent` / `--ring`).

### 7.3 Coexistence with existing pill interactions

The pill header already has:
- `ondoubleclick` on the title span → edit/rename (line ~545).
- `onclick` on the fullscreen button (line ~579) and close button (line ~597),
  both `e.stop_propagation()`.

Rules:
- The pill's `onpointerdown` is added to the pill container. The two
  buttons' existing `onclick` handlers already stop propagation, but
  `pointerdown` is a separate event from `click`. So we **also** add
  `onpointerdown: |e| e.stop_propagation()` on both icon buttons so that
  grabbing the icons never starts a pill drag.
- The title span's `ondoubleclick` continues to work because we only
  **start** the drag after the 4px move threshold; a pure click / dblclick
  (no move) leaves `pill_drag` in the pending state and the `pointerup`
  handler clears it without firing a swap.
- `set_active` (pane focus) currently fires on the **outer** pane
  `onpointerdown` (line ~507). That stays — clicking a pane still selects
  it. The pill's `onpointerdown` (added) does not call `set_active` again
  (avoid double assignment), but a successful drop will call `set_active`
  on the source so the just-moved agent's new slot is focused.

### 7.4 Window-level move/up handlers (new helper)

A small module `frontend/src/components/workspace/pill_drag.rs` (new file,
~150 lines):

- `install_pill_drag_listeners(pill_drag: Signal<Option<PillDrag>>,
  workspace: Signal<WorkspaceStore>)` — called from a `use_effect` in
  `WorkspaceGrid`. Registers `pointermove` and `pointerup` on `window`.
- On `pointermove`:
  1. Read `pill_drag`. If `None` or `!moved` and under threshold → return.
  2. If past threshold: set `moved = true`. Now we're committed. Set
     `e.prevent_default()` and `body` `cursor: grabbing`.
  3. Update `cur_x`/`cur_y`.
  4. Hit-test: `document.elementFromPoint(cur_x, cur_y)`; walk up the DOM to
     find the nearest `[data-pane-id]`; if it's within the same space and
     not the source, set `target_pane_id`. Else `None`.
- On `pointerup`:
  1. If `moved && target.is_some() && target != source`: call
     `workspace.write().swap_pane_agents(space_id, src, tgt)`, then
     `terminal_store.write().set_active(src)` (focus the moved agent's new
     slot).
  2. Reset: `pill_drag.set(None)`, restore body cursor.
  3. If `!moved` (was just a click on the pill) → also clear `pill_drag`.

`document.elementFromPoint` requires `wasm_bindgen` + `JsCast` (already used
at `terminal_grid.rs:2`). A small JS interop helper returns the nearest
ancestor with `data-pane-id` for an `(x,y)`; pure function, no state.

### 7.5 `PillDragGhost` component

A `position: fixed` floating pill rendered from `source_label` +
`source_color`, following `cur_x`/`cur_y` with a slight vertical offset.
`pointer-events: none; z-index: 9999`. Visually a small frost pill (reuse
the `.pill` token aesthetic: rounded `999px`, lit-edge, lapis border). No
backdrop-filter on the ghost (perf — it moves every frame). Fade in once
`moved` is true; if `!moved`, render nothing (keeps a click invisible).

## 8. Persistence

`swap_pane_agents` reuses the existing `update_space` path, which already
triggers workspace persistence to the `SessionStore` on every mutation
(verified pattern: `a6578fe2` and the rename handler at
`terminal_grid.rs:545-554`). **No new IPC command, no Tauri command, no
backend change.** Pure frontend store mutation → existing save pipeline.

Restart invariance: after a swap and restart, `panes[]` reloads in the
swapped order; sizes default back to `GridTemplate` equal-flex (the existing
behavior — sizes were never persisted anyway, §“reverted memory note: sizes
are runtime signals”).

## 9. Files touched

| File | Change | Lines |
|---|---|---|
| `frontend/src/stores/workspace.rs` | new `swap_pane_agents` method + unit tests | ~60 |
| `frontend/src/components/workspace/pill_drag.rs` | **new** — listeners + ghost + interop | ~180 |
| `frontend/src/components/workspace/terminal_grid.rs` | add `pill_drag` signal, prop-thread into `PaneItem`, ghost render, `data-pane-id` + `is-dnd-target` class, pointer-down on pill, stop-prop on icon buttons, `install_pill_drag_listeners` effect | ~40 |
| `frontend/src/components/workspace/mod.rs` | declare `mod pill_drag;` | 1 |
| `frontend/public/styles.css` | `.is-dnd-target` ring + `.dnd-ghost` pill styles | ~25 |

No backend (`src-tauri/`, `crates/`) changes. No `PaneConfig` field changes
(Decision A — no `slot_index`; we swap in place, array index is the slot).

## 10. Test plan

### 10.1 Unit (rust, `cargo test --workspace`)
- `stores/workspace.rs::swap_pane_agents`:
  - swaps two panes by id — full `PaneConfig` (including `id`) trades slots
    (assert `panes[ia] == old_panes[ib]` and vice versa for every field)
  - no-op when ids equal
  - no-op (early return) when an id is missing
  - persist triggered (mock or observed via a `did_persist` flag if the
    store exposes one; otherwise assert `Space.panes` post-state)
  - grid slot indices `ia`/`ib` are unchanged by the swap (only the values
    at those indices trade places) — sizes stay bound to slots ✓ Approach A

### 10.2 Manual (debug build, `cargo tauri dev`)
- 1×1 / 1×2 / 2×2 grids: drag pill A → pill B, confirm labels swap and
  sizes follow slots (A takes B's size, B takes A's size).
- User's exact case: 2-top + 1-bottom shell — drag top-left onto bottom;
  top-left agent becomes full-width bottom, bottom agent becomes 50% top.
- Drag onto **self** → no-op.
- Drag and release **below threshold** → no swap; pill click still
  focuses pane (and double-click still renames).
- Drag onto the fullscreen / close icon buttons → no drag starts (icons
  stop-prop on pointerdown).
- Drag while a target pane is **fullscreen** → drop on the fullscreened
  pane swaps; dropping outside any pane cancels.
- Drag off the workspace (release over sidebar / titlebar) → cancels, no
  swap.
- After swap: close one pane → confirms the right pane id is removed and
  active reassigned (no panic, no stale `pill_drag`).
- Restart the app after a swap → swapped `panes[]` order persists.

### 10.3 E2E (later — not blocking this spec)
A tauri-webdriver test that performs a pointer-down → pointer-move →
pointer-up sequence between two pills and asserts label swap. (Note: the
existing tauri-wd `Node.contains` bug doesn't affect us — no HTML5 DnD — but
WebDriverIO pointer actions dispatch synthesized PointerEvents which our
window listener will receive. Verify in implementation; if synthesized
events don't carry `pointerId`/`clientX/Y` correctly, fall back to a JS
`dispatchEvent` injection via `browser.execute`.)

## 11. Edge cases & open questions

### 11.1 Does `id` move with the agent? — **Confirmed: yes, full migration.**
See §5.1. The whole `PaneConfig` (id + all fields) swaps between slots. The
PTY follows the agent (idempotent `pty_spawn` reattaches at the new slot).
xterm remounts at both slots; scrollback is lost (xterm-renderer-side
state). User explicitly accepted this cost.

### 11.2 Swapping a running agent — **Confirmed: allow.**
Because `id` moves with the agent, the running process follows the agent to
the new slot — pill and process stay in agreement. No label/process mismatch
(the mismatch only existed in the discarded "id stays with slot" design).
The cost remains the xterm remount + scrollback loss at both slots.

### 11.3 Cross-row / asymmetric grids — handled by index math, no special code
Because we swap `panes[ia]` ↔ `panes[ib]` and the grid reads panes row by
row, a cross-row swap is automatically correct: the agent that was at index
`ia` (top row) is now at index `ib` (bottom row) and inherits whatever flex
weight that slot has. No row/col math in the swap logic itself.

### 11.4 Pointer capture & multi-touch
If a second pointer goes down while a drag is in flight, ignore it (track
`pointer_id`; only the originating pointer drives the drag). Touch stylus /
finger drag works the same as mouse — PointerEvents unify them, which is
another reason we chose them over mouse-only HTML5 DnD.

### 11.5 Reduced motion / accessibility
The ghost pill has no animation (just follows the cursor), so
`prefers-reduced-motion` is naturally respected. A future enhancement:
keyboard reorder (focus a pill, press arrow keys to swap with neighbor) —
out of scope here.

### 11.6 Perf
One `Signal<Option<PillDrag>>` write per `pointermove` frame (~60fps)
causes `WorkspaceGrid` to re-evaluate. The grid already re-evaluates on
resize drags the same way (`DragOverlay`), and `col_widths` decomposition
memory notes that this is acceptable. If profiling shows jank, memoize the
ghost into its own root so only the ghost subtree re-renders, not every
pane. **Default: simplest path; optimize if measured.**

## 12. Reverted-history cross-reference

For confidence, this design re-introduces the *intent* of the reverted
commits (`3cc0c20f`, `0dc3beff`, `a76c88eb`, `f6898471`, `ee6da751`, et al.)
but with a **different technique** (pointer events vs HTML5 DnD) and a
**simpler mutation** (in-place `panes[]` swap vs `slot_index`
re-indexing). No new persisted field is added; `slot_index` is not
re-introduced. The reverted e2e test (`c7b393f6`) is replaced by the §10.3
plan suited to pointer events.

