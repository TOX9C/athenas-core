# Pane-Pill Drag-and-Drop Swap — Revival Design

**Date:** 2026-07-06
**Branch target:** `feat/pane-pill-drag-swap-revival` off `main` → PR to `main`
**Status:** Spec — approved by user 2026-07-06 (scope: feature + branch fixes only)

---

## 1. Goal

Revive the unmerged `feat/pane-pill-drag-swap` branch by merging it onto current
`main` and applying three branch-specific fixes. The feature: drag an agent's
**pane pill** (the rounded header inside each workspace pane showing the agent
label) and drop it onto another pane's pill to **swap the two agents**. Each
agent takes the grid slot — and therefore the size — of the other. The
underlying PTY sessions stay alive and uninterrupted (full session migration
via idempotent `pty_spawn`).

User's example: 2-top / 1-bottom grid (two agents share the top row at 50%
each, one shell occupies the bottom row full-width). Grabbing the top-left
pill and dropping it on the bottom pill moves the top-left agent to the
bottom (becomes full-width) and moves the bottom agent to the top-left slot
(becomes 50%). **Agents trade identities; sizes stay bound to slots.**

## 2. Background — why revive, not reimplement

A prior deep-dive (workspace exploration subagent + senior Rust review
subagent, 2026-07-06) established:

- **No DnD swap code exists on `main`** — nothing to delete. The only
  "drag" code on `main` is `DragInfo`/`DragKind::{Col,Row}` + `DragOverlay`
  (pane **resize**, flex-weight manipulation) and `DraggableItem`/
  `dropped_context` (the unrelated Athena drag-into-chat-as-context
  feature).
- A complete prior implementation lives on the unmerged branch
  `feat/pane-pill-drag-swap` (5 commits, never merged to `main`,
  not in the working tree) plus a full ~460-line design spec (commit
  `66776054`).
- The Rust reviewer's verdict: **REVIVE-WITH-MINOR-FIXES**. The branch
  is well-architected, merges cleanly onto current `main` with zero
  conflicts, has 5 passing unit tests, and needs only minor cleanup —
  not logic rewrites.

### Architectural choices inherited from the branch's prior spec
(commit `66776054`, all hard calls already resolved there):

| Decision | Choice | Why |
|---|---|---|
| Swap semantics | **Approach A** — swap identities in `Space.panes[]`; slots keep their sizes | Sizes are runtime flex signals; only identity should move |
| Drag technique | **Pointer-event drag** (`pointerdown`/`move`/`up`) with a floating ghost; **no HTML5 DnD**, no `dataTransfer` | Previous HTML5 DnD attempt was reverted for 3 WKWebView bugs (drop-target arming protocol, `tauri-webdriver` `Node.contains` element-ref bugs, unreliable custom-MIME delivery). Pointer events sidestep all three and are already proven in the surface via the resize `DragOverlay`. |
| Grab surface | Whole pill header — no grip icon | YAGNI |
| Session migration | Full: `id` moves with the agent | `pty_spawn` is idempotent by id, so remounting `XtermMount` (keyed `xterm-{pane_id}`) reattaches the existing PTY; running process uninterrupted |

## 3. Scope

### 3.1 In scope
1. Merge `feat/pane-pill-drag-swap` onto current `main` (clean merge — git
   auto-drops the branch's stale `types/theme.rs` duplicate and unrelated
   `src-tauri/src/state.rs` refactor).
2. Apply the **three branch-specific fixes** in §5.
3. Verify: `cargo check --workspace`, `cargo test --workspace`,
   `cargo clippy -- -D warnings`, and a manual smoke test in
   `cargo tauri dev`.

### 3.2 Out of scope (explicit non-goals)
- **Do NOT fix the inherited `use_session_signal`-in-`use_memo` panic risk**
  at `terminal_grid.rs:319` and `:933`. This is a **preexisting landmine on
  `main`** that the branch did *not* introduce. It is acknowledged but left
  for a separate ticket. (User decision, 2026-07-06.)
- No HTML5 DnD, no `dataTransfer`, no `slot_index` field, no backend/IPC
  changes, no cross-space drag, no sidebar/constellation drag.
- No new persisted field (`id` already exists; we swap in-place).
- No E2E test in this revival (spec §10.3 plans it as a later, non-blocking
  addition).

## 4. Data model & mutation (unchanged from the branch — verified sound)

`Space.panes: Vec<PaneConfig>` is read left→right, row by row, to place
panes into the flexbox grid. Array index **is** the grid slot, so a swap of
two panes is a swap of two entries in this Vec. Swapping **identities with
session migration** means the entire `PaneConfig` swaps between the two
slots — `id` and all agent fields move together. The slot keeps only its
grid position (array index) and its flex-grow weight; everything the user
perceives as "the agent" (PTY session id, agent type, label, resume state)
travels to the new slot.

### 4.1 The store mutation (already implemented + tested on the branch)

```rust
/// Pure helper — swaps two panes by id. Returns false (no-op) if either id
/// is missing or the ids are equal. Slot indices are unchanged; only the
/// values at those indices trade places (sizes stay bound to slots).
pub fn swap_panes_by_id(space: &mut Space, a: &str, b: &str) -> bool

/// WorkspaceStore wrapper — looks up the space, calls the helper, and
/// triggers persistence via the existing `update_space` → `save()` path.
pub fn swap_pane_agents(&mut self, space_id: &str, a: &str, b: &str)
```

Implementation outline (immutable update, per coding-style rules):
1. `self.update_space(space_id, |space| swap_panes_by_id(space, a, b))`.
2. Inside `swap_panes_by_id`: find indices `ia`, `ib`; if not found or
   equal, return false. `space.panes.swap(ia, ib);` returns true.

### 4.2 Why session migration is safe

`pty_spawn` is idempotent by id (`crates/athena-terminal/src/session.rs:295`
— "already exists, returning existing"; verified by tests
`spawn_with_same_id_returns_existing` and
`spawn_concurrent_same_id_races_to_single_session`). When `XtermMount`
remounts at the new slot with the swapped `pane_id`, its `pty_spawn` call
returns the **existing** PTY session — no duplicate process, no re-exec.
The backend PTY is the durable home; the xterm.js renderer is a transient
view onto it.

### 4.3 Accepted cost (documented, user-accepted in prior spec)

The xterm.js renderer instance is destroyed and recreated on remount
(Dioxus keys `XtermMount` as `xterm-{pane_id}`). xterm.js stores scrollback
**in the renderer instance**, not the PTY, so **scrollback is lost at both
swapped slots** on each swap — the fresh xterm reattaches to the live PTY
and shows new output as it arrives, but prior scrollback is gone. The
running process itself is uninterrupted. (Mitigation if it proves annoying
later: a "snapshot scrollback to PTY" path — out of scope here.)

### 4.4 Registry & active-session handling (unchanged)

Because `id` moves, the `TerminalRegistry` (keyed by pane id) and
`TerminalStore.known_pane_ids` / `active_session_id` need no remap — the same
id now renders at the other slot; its registry signal and PTY follow it.
`active_session_id` is a pane id (not a slot), so it stays valid. Pure
frontend `Vec::swap` + the existing xterm keying does all the lifting.

## 5. The three branch-specific fixes (the only deltas vs. the merge)

### 5.1 Fix #1 — Ghost hot-path allocation (MEDIUM)
- **Location:** `frontend/src/components/workspace/pill_drag.rs:233`
  (`PillDragGhost` style attribute).
- **Problem:** The ghost's `style` is rebuilt via `format!` on every
  `pointermove` frame (~60fps), allocating a new string each frame.
- **Fix:** Pre-build the static style segments (color, z-index,
  `position: fixed`, `pointer-events: none`, border-radius) once; only the
  `transform: translate(cur_x px, cur_y px)` segment updates per frame. No
  per-frame full-string allocation.

### 5.2 Fix #2 — Multi-touch / second-pointer guard (MEDIUM)
- **Location:** `PillDrag` struct + `PillDragOverlay` handlers in
  `pill_drag.rs`.
- **Problem:** `PillDrag` has no `pointer_id` field (the prior spec's §11.4
  intended this but it was never implemented). A second `pointerdown` mid-drag
  overwrites `pill_drag` with a new source.
- **Fix:** Add `pointer_id: i32` to `PillDrag`. Set it from
  `e.data.pointer_id()` in the `onpointerdown` handler. In
  `PillDragOverlay`'s `onpointermove` and `onpointerup`, ignore events whose
  `pointer_id` doesn't match the originating one.

### 5.3 Fix #3 — Delete dead `show_notification_toast` stub (LOW)
- **Location:** `frontend/src/components/shared/toast.rs` (branch diff).
- **Problem:** `show_notification_toast` is an orphan TODO function never
  called by any component path. The branch incidentally touched this file.
- **Fix:** Delete the stub rather than carry dead code. (If a real
  toast-on-swap is wanted later, it's a separate enhancement.)

## 6. Drag interaction model (unchanged from the branch)

`PillDrag` signal in `WorkspaceGrid` (the struct, after fix #2):

```rust
#[derive(Clone, Debug)]
struct PillDrag {
    source_pane_id: String,
    source_space_id: String,
    source_label: String,
    source_color: String,
    pointer_id: i32,      // NEW — fix #2
    start_x: f64, start_y: f64,
    cur_x: f64, cur_y: f64,
    moved: bool,
    target_pane_id: Option<String>,
}
```

- **4px move threshold.** Below it, pointerdown → pointerup is a plain
  click; rename dblclick and button clicks all survive.
- **Window-level `pointermove`/`pointerup`** live on `PillDragOverlay`, a
  Dioxus component that auto-unmounts when `pill_drag` clears — so event
  listeners disappear automatically (no JS `Closure` leaks, no
  `removeEventListener` footguns; this is the property the Rust reviewer
  flagged as "solid, keep verbatim").
- **Hit-test:** `document.elementFromPoint(cur_x, cur_y)` → walk DOM to
  nearest `[data-pane-id]`.
- **Drop:** if `moved && target.is_some() && target != source` →
  `workspace.write().swap_pane_agents(space_id, src, tgt)` then
  `terminal_store.write().set_active(src)`. Else cancel. Always reset
  `pill_drag.set(None)`.

## 7. Component changes (from the merge + fixes)

| File | Change |
|---|---|
| `frontend/src/components/workspace/pill_drag.rs` | (from merge) `PillDrag` state, `PillDragOverlay` + `PillDragGhost`, hit-test interop; + fix #1 (ghost style) and fix #2 (`pointer_id`) |
| `frontend/src/components/workspace/terminal_grid.rs` | (from merge) `pill_drag` signal, prop-thread into `PaneItem`, ghost render, `data-pane-id` + `is-dnd-target` class, pointer-down on pill, stop-prop on icon buttons, `install_pill_drag_listeners` effect — no extra edits |
| `frontend/src/stores/workspace.rs` | (from merge) `swap_pane_agents` + `swap_panes_by_id` + `mod swap_panes_tests` (5 unit tests) |
| `frontend/src/components/workspace/mod.rs` | (from merge) `mod pill_drag;` |
| `frontend/src/components/shared/toast.rs` | (from merge, incidental edit) + fix #3 (delete `show_notification_toast` stub) |
| `frontend/public/styles.css` | (from merge) `.is-dnd-target` ring + `.dnd-ghost` pill styles |
| `frontend/src/types/theme.rs` | git auto-drops the branch's stale duplicate (keeps main's `stores/ui.rs` layout) |

No backend (`src-tauri/`, `crates/`) changes. No `PaneConfig` field changes.
No IPC command, no Tauri command.

## 8. Persistence (unchanged from the branch)

`swap_pane_agents` reuses the existing `update_space` → `save()` pipeline.
No new IPC, no Tauri command, no backend change. Restart invariance: after a
swap and restart, `panes[]` reloads in the swapped order; sizes default back
to `GridTemplate` equal-flex (existing behavior — sizes are runtime signals,
not persisted).

## 9. Test plan

### 9.1 Unit (rust, `cargo test --workspace`)
The 5 existing `swap_panes_tests` cover:
- `swaps_two_panes_by_id_full_config_including_id` — full `PaneConfig` swap
  including `id` and label
- `cross_row_swap_swaps_pane_config_only_slots_keep_index` — verifies 0↔3
  swap, slots keep their indices
- `noop_when_ids_equal` — returns false, state unchanged
- `noop_when_pane_id_missing` — returns false, state unchanged
- `preserves_unrelated_panes_and_grid_template` — Vec length and grid
  invariant unchanged

`save()` integration is host-untestable (js-sys statics) — accepted
limitation, documented in code.

### 9.2 Manual (debug build, `cargo tauri dev`)
- 1×1 / 1×2 / 2×2 grids: drag pill A → pill B, confirm labels swap and
  sizes follow slots.
- User's exact case: 2-top + 1-bottom shell — drag top-left onto bottom;
  top-left agent becomes full-width bottom, bottom agent becomes 50% top.
- Drag onto **self** → no-op.
- Drag and release **below threshold** → no swap; pill click still focuses
  pane; double-click still renames.
- Drag onto the fullscreen / close icon buttons → no drag starts (icons
  stop-prop on pointerdown).
- Drop **off the workspace** (release over sidebar / titlebar) → cancels,
  no swap.
- Swap while an agent is **running** → process uninterrupted, scrollback
  lost at both slots.
- After swap: close one pane → right pane id removed, active reassigned
  (no panic, no stale `pill_drag`).
- **Multi-touch (fix #2 verification):** start a drag with pointer 1, press
  with pointer 2 mid-drag → pointer 2 is ignored, drag continues with
  pointer 1.
- Restart the app after a swap → swapped `panes[]` order persists.

### 9.3 E2E (later — non-blocking, out of scope for this revival)
A tauri-webdriver pointer down→move→up sequence asserting label swap.

## 10. Edge cases (unchanged from the branch, already resolved)

- **Cross-row / asymmetric grids:** handled by index math automatically —
  `panes[ia]` ↔ `panes[ib]`, the grid reads panes row by row, so the agent
  at `ia` (top) is now at `ib` (bottom) and inherits that slot's flex
  weight. No row/col math in the swap logic.
- **Swapping a running agent:** allowed; `id` moves with it so pill and
  process stay in agreement.
- **Multi-touch:** fixed by `pointer_id` guard (fix #2).
- **Reduced motion:** ghost has no animation (just follows the cursor),
  naturally respecting `prefers-reduced-motion`.
- **Perf:** one `Signal<Option<PillDrag>>` write per `pointermove` frame
  re-evaluates `WorkspaceGrid`. The grid already re-evaluates the same way
  on resize drags; acceptable. Fix #1 additionally removes per-frame string
  allocation from the ghost.

## 11. Risks & mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| `use_session_signal` inside `use_memo` → `RuntimeError: Unreachable` in WKWebView | HIGH, **inherited/preexisting**, **out of scope** | Documented; left for a separate ticket. User decision 2026-07-06. |
| Scrollback lost at both swapped slots | MEDIUM, accepted | xterm.js renderer-side state; process uninterrupted. Mitigation path documented but out of scope. |
| Per-frame ghost allocation | MEDIUM | Fix #1 |
| Multi-touch overwrites source | MEDIUM | Fix #2 |
| Dead `show_notification_toast` stub carried forward | LOW | Fix #3 |

## 12. Branch hygiene
- Drop the branch's stale `types/theme.rs` duplicate (git handles
  automatically on merge).
- Drop the branch's unrelated `src-tauri/src/state.rs` refactor (git keeps
  main's version; already superseded by `fbac2d45`).
- After merge + fixes: `cargo check --workspace`,
  `cargo test --workspace`, `cargo clippy -- -D warnings`.
