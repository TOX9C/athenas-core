# Pane-Pill Drag-and-Drop Swap — Revival Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Revive the unmerged `feat/pane-pill-drag-swap` branch by merging it onto current `main` and applying three branch-specific fixes, delivering drag-to-swap of agents across workspace panes with full session migration and uninterrupted PTYs.

**Architecture:** Pointer-event drag (not HTML5 DnD) on a fullscreen `PillDragOverlay` scrim that owns pointermove/up, mirroring the proven resize `DragOverlay` pattern already in `terminal_grid.rs`. A pure `swap_panes_by_id` does a `Vec::swap` on `Space.panes[]` so the whole `PaneConfig` (incl. `id`) trades slots; the idempotent `pty_spawn` reattaches the existing PTY on remount, so the running process is uninterrupted. No backend, no IPC, no schema changes.

**Tech Stack:** Rust + Dioxus 0.7 (WASM), `wasm_bindgen`/`web_sys` JS interop, `cargo test --workspace`, `cargo tauri dev` for manual smoke.

## Global Constraints

- **No HTML5 DnD, no `dataTransfer`, no `draggable`, no `slot_index` field.** Array index *is* the slot; we swap in-place.
- **No backend / `src-tauri/` / `crates/` / IPC changes.** Pure frontend store mutation + existing persistence pipeline.
- **No new persisted field.** `PaneConfig.id` already exists; the whole `PaneConfig` (id + all agent fields) travels with the agent on swap.
- **Do NOT fix the inherited `use_session_signal`-in-`use_memo` panic risk** at `terminal_grid.rs:319` and `:836` — preexisting on `main`, out of scope (user decision 2026-07-06).
- **Accepted cost:** xterm.js scrollback is lost at both swapped slots on each swap (renderer-side state, not PTY-side). Process uninterrupted. Documented in spec.
- Immutability rule (per `~/.claude/rules/common/coding-style.md`): the store mutation goes through the existing `update_space` hook; the swap core is a pure `Vec::swap`.
- Commit-message format: `<type>: <description>` (attribution disabled globally).

---

## File Structure

| File | Responsibility | Source |
|---|---|---|
| `frontend/src/components/workspace/pill_drag.rs` | `PillDrag` state, `PillDragOverlay` scrim, `PillDragGhost`, hit-test interop | New (from merge) + fix #1 (ghost style) + fix #2 (`pointer_id`) |
| `frontend/src/components/workspace/terminal_grid.rs` | Wires the pill grab surface into `PaneItem` + mounts the overlay/ghost in `WorkspaceGrid` | Modified (from merge) — no extra edits |
| `frontend/src/stores/workspace.rs` | `swap_pane_agents` method + `swap_panes_by_id` pure core + 5 unit tests | Modified (from merge) |
| `frontend/src/components/workspace/mod.rs` | `pub mod pill_drag;` registration | Modified (from merge) |
| `frontend/src/components/shared/toast.rs` | (incidental edit carried by the branch) — delete the dead `show_notification_toast` stub (fix #3) | Modified (from merge) |
| `frontend/public/styles.css` | `.pane-wrap.is-dnd-target`, `.dnd-ghost`, `.dnd-overlay`, `.dnd-overlay.is-grabbing` | Modified (from merge) |
| `frontend/src/types/theme.rs` | Branch carried a stale duplicate; **git auto-drops it on merge** (keeps main's `stores/ui.rs` layout) | Auto-rejected by merge |

**No backend files touched.** No `PaneConfig` schema changes.

---

## Task 0: Park uncommitted perf edits and create the revival branch

**Why:** The working tree has uncommitted performance edits (`terminal_grid.rs`, `xterm_mount.rs`, `agent_info_poller.rs`, `output_event_bus.rs`, `shared/toast.rs`, `types/theme.rs`) — the `use_terminal_registry().session_signal(...)` refactor. These must be committed before merging `feat/pane-pill-drag-swap` so they don't get clobbered or cause working-tree conflicts (the branch touches `terminal_grid.rs`, `toast.rs`, and `theme.rs` too). We branch first per global git rules (not committing directly to `main`).

**Files:**
- Worktree state only (no file edits this task).

- [ ] **Step 1: Confirm the dirty working tree matches the known perf work (sanity guard before committing)**

Run:
```bash
git status --short
git diff --stat
```
Expected: 6 modified files (`frontend/src/components/agents/output_event_bus.rs`, `frontend/src/components/shared/toast.rs`, `frontend/src/components/workspace/agent_info_poller.rs`, `frontend/src/components/workspace/terminal_grid.rs`, `frontend/src/components/workspace/xterm_mount.rs`, `frontend/src/types/theme.rs`). The diffs are the `use_session_signal`→`use_terminal_registry().session_signal(...)` per-session-signal refactor (no DnD code). If anything unexpected appears, STOP and surface it before continuing.

- [ ] **Step 2: Create the revival branch off `main` BEFORE committing (so the perf commit lands on the branch, not main)**

Run:
```bash
git checkout -b feat/pane-pill-drag-swap-revival
git rev-parse --abbrev-ref HEAD
```
Expected: `feat/pane-pill-drag-swap-revival`

- [ ] **Step 3: Commit the uncommitted perf edits on the new branch**

Run:
```bash
git add -A
git status --short
```
Expected: same 6 files staged, nothing new.

Then commit:
```bash
git commit -m "perf(terminal): preserve uncommitted per-session-signal refactor before pane-pill DnD revival merge"
```

- [ ] **Step 4: Confirm a clean tree before the merge**

Run:
```bash
git status --short
```
Expected: empty (clean working tree).

---

## Task 1: Merge the branch and verify the build is green

**Why:** Get the prior implementation onto the revival branch as the foundation. Per the review, the merge is clean against a `main` checkout (git auto-drops the stale `types/theme.rs` duplicate and the unrelated `src-tauri/src/state.rs` refactor). We merge with `--no-ff` to keep the branch's 5 commits visible in history, then build+test.

**Files:**
- Merged in: `pill_drag.rs`, `workspace.rs` (swap method + tests), `terminal_grid.rs` (wiring), `workspace/mod.rs`, `styles.css`, `shared/toast.rs`.

- [ ] **Step 1: Merge the upstream branch with `--no-ff` (no squash — preserve the 5 prior commits)**

Run:
```bash
git fetch --all 2>/dev/null
git merge --no-ff feat/pane-pill-drag-swap -m "feat(workspace): merge pane-pill drag-and-drop swap from feat/pane-pill-drag-swap"
```
Expected: "Merge made by the 'ort' strategy." with a list of changed files including `frontend/src/components/workspace/pill_drag.rs` (new), `frontend/src/components/workspace/terminal_grid.rs`, `frontend/src/stores/workspace.rs`, `frontend/src/components/workspace/mod.rs`, `frontend/public/styles.css`, `frontend/src/components/shared/toast.rs`. **`frontend/src/types/theme.rs` should NOT appear in the merge summary** (git auto-dropped the branch's stale duplicate). If a merge conflict appears, STOP — do not resolve blindly; surface it (expected conflict files: none, per the review's clean-merge verdict). If `types/theme.rs` shows as conflicted, prefer **main's** version (`git checkout --ours frontend/src/types/theme.rs`).

- [ ] **Step 2: Confirm `pill_drag.rs` is now on disk and the module is registered**

Run:
```bash
test -f frontend/src/components/workspace/pill_drag.rs && echo "pill_drag.rs OK"
grep -n "pub mod pill_drag" frontend/src/components/workspace/mod.rs
grep -c "pill_drag" frontend/src/components/workspace/terminal_grid.rs
grep -n "pub fn swap_pane_agents\|pub fn swap_panes_by_id" frontend/src/stores/workspace.rs
```
Expected:
- `pill_drag.rs OK`
- a line `pub mod pill_drag;` in `mod.rs`
- a nonzero count of `pill_drag` occurrences in `terminal_grid.rs`
- two lines: `pub fn swap_pane_agents(...)` and `pub fn swap_panes_by_id(...)`

- [ ] **Step 3: Compile-check the workspace**

Run:
```bash
cargo check --workspace 2>&1 | tail -40
```
Expected: `Finished` with no errors. **Likely failure to watch for:** the branch's `terminal_grid.rs` uses `display_label` and `drag_agent_type` in the `onpointerdown` — if the merge landed those references but the surrounding `PaneItem` changed on main, you'll see borrow-checker or "cannot find value `display_label`" errors. If so, STOP and fix in the smallest possible way (do not delete the wiring); report exactly which symbol is undefined.

- [ ] **Step 4: Run the existing unit tests (the 5 `swap_panes_tests` must pass)**

Run:
```bash
cargo test --workspace swap_pane 2>&1 | tail -30
cargo test --workspace walk_to_data_pane 2>&1 | tail -10
```
Expected: 5 `swap_panes_tests::*` tests PASS (`swaps_two_panes_by_id_full_config_including_id`, `cross_row_swap_swaps_pane_config_only_slots_keep_index`, `noop_when_ids_equal`, `noop_when_pane_id_missing`, `preserves_unrelated_panes_and_grid_template`) + 1 `walk_to_data_pane_id_none_for_none` PASS. If any fail, STOP — the merge introduced a regression; report the failure.

- [ ] **Step 5: Run clippy**

Run:
```bash
cargo clippy --workspace -- -D warnings 2>&1 | tail -40
```
Expected: no warnings (the `-D warnings` makes warnings fail the command). If new lints appear from the merged code, record them — we'll address the ones our fixes introduce in Tasks 2-4, but pre-existing branch lints must be fixed here so the tree is clippy-green before we layer fixes on top.

- [ ] **Step 6: Commit the merge (if `--no-ff` left it staged) and push the branch**

Run:
```bash
git status --short
```
If the merge is already committed (Step 1's `-m` commits it), this is clean. Then:
```bash
git push -u origin feat/pane-pill-drag-swap-revival
```
Expected: branch pushed with the merge commit. (Network/remote: push only if remote access is available; if it fails, note it and continue — the local branch is the source of truth for the rest of the plan.)

---

## Task 2: Fix #2 — Add `pointer_id` to `PillDrag` for multi-touch guard

**Why:** Without `pointer_id`, a second `pointerdown` mid-drag overwrites `pill_drag` with a new source. The prior spec (§11.4) intended this guard but never shipped it. This is the struct-level + handler-level change; the ghost-style fix (Task 3) is independent and tested separately.

**Files:**
- Modify: `frontend/src/components/workspace/pill_drag.rs` (the `PillDrag` struct, `PillDragOverlay`'s `onpointermove`/`onpointerup`).
- Modify: `frontend/src/components/workspace/terminal_grid.rs` (the `onpointerdown` that constructs `PillDrag`, to set `pointer_id`).

**Interfaces:**
- `PillDrag` gains `pub pointer_id: i32`. Every place that constructs a `PillDrag` literal must set it; the two existing construction sites are `terminal_grid.rs` (the pill `onpointerdown`) — `pill_drag.rs` itself only reads it.
- `dioxus::prelude::PointerEvent` exposes `e.data.pointer_id() -> Option<i32>`.

- [ ] **Step 1: Add the `pointer_id` field to the `PillDrag` struct**

In `frontend/src/components/workspace/pill_drag.rs`, find:
```rust
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
```
Insert after the `source_color` field:
```rust
    /// Originating pointer id. While a drag is in flight, `pointermove`/`up`
    /// events from any other pointer are ignored — guards multi-touch so a
    /// second finger can't hijack or cancel the drag.
    pub pointer_id: i32,
```

- [ ] **Step 2: Guard `onpointermove` and `onpointerup` against non-originating pointers**

In `PillDragOverlay` (`pill_drag.rs`), the `onpointermove` closure currently begins:
```rust
    let onpointermove = move |e: PointerEvent| {
        let coords = e.data.client_coordinates();
        let mut current = match drag.read().clone() {
            Some(d) => d,
            None => return,
        };
```
Replace with (add the pointer_id guard right after `coords`):
```rust
    let onpointermove = move |e: PointerEvent| {
        let coords = e.data.client_coordinates();
        let mut current = match drag.read().clone() {
            Some(d) => d,
            None => return,
        };
        // Multi-touch guard: only the originating pointer drives the drag.
        // A second finger pressing mid-drag must not reposition the ghost
        // or change the drop target.
        if e.data.pointer_id() != Some(current.pointer_id) {
            return;
        }
```
(The rest of the closure — threshold check, `cur_x`/`cur_y`, `find_drop_target`, `drag.set` — is unchanged.)

In the `onpointerup` closure, it currently begins:
```rust
    let onpointerup = move |_e: PointerEvent| {
        let finished = drag.read().clone();
        drag.set(None);
        let Some(d) = finished else {
            return;
        };
```
Replace the `_e` binding with `e` and add the guard so a stray second-pointer `pointerup` can't clear an in-flight drag from the originating pointer:
```rust
    let onpointerup = move |e: PointerEvent| {
        // Multi-touch guard: ignore pointerup from any pointer other than the
        // one that started the drag. (The originating pointer's up commits.)
        let is_origin = drag
            .read()
            .as_ref()
            .map(|d| e.data.pointer_id() == Some(d.pointer_id))
            .unwrap_or(true);
        if !is_origin {
            return;
        }
        let finished = drag.read().clone();
        drag.set(None);
        let Some(d) = finished else {
            return;
        };
        if !d.moved {
            // A click, not a drag — no swap.
            return;
        }
        if let Some(target) = d.target_pane_id.as_ref() {
            if target != &d.source_pane_id {
                {
                    let mut ws = workspace.write();
                    ws.swap_pane_agents(&d.source_space_id, &d.source_pane_id, target);
                }
                // Focus the moved agent at its new slot. `set_active` keys on
                // pane id (not slot index), and `swap_pane_agents` migrated the
                // pane id with the agent, so this stays valid post-swap.
                terminal_store.write().set_active(d.source_pane_id.clone());
            }
        }
        // Dropping outside any pane, or on self → no-op; drag already cleared.
    };
```

- [ ] **Step 3: Set `pointer_id` at the construction site in `terminal_grid.rs`**

In `frontend/src/components/workspace/terminal_grid.rs`, find the `onpointerdown` handler that builds the `PillDrag` literal. It currently ends:
```rust
                        pill_drag.set(Some(crate::components::workspace::pill_drag::PillDrag {
                            source_pane_id: drag_pane_id.clone(),
                            source_space_id: drag_space_id.clone(),
                            source_label: label_text,
                            source_color: color.to_string(),
                            start_x: coords.x,
                            start_y: coords.y,
                            cur_x: coords.x,
                            cur_y: coords.y,
                            moved: false,
                            target_pane_id: None,
                        }));
```
Add `pointer_id` (default to `-1` for the rare synthetic pointer with no id, matching the reviewer's "desktop-first" guidance — real pointers have id ≥ 0, so `-1` only ever fails to match a real second pointer, which is the safe fallback):
```rust
                        pill_drag.set(Some(crate::components::workspace::pill_drag::PillDrag {
                            source_pane_id: drag_pane_id.clone(),
                            source_space_id: drag_space_id.clone(),
                            source_label: label_text,
                            source_color: color.to_string(),
                            pointer_id: e.data.pointer_id().unwrap_or(-1),
                            start_x: coords.x,
                            start_y: coords.y,
                            cur_x: coords.x,
                            cur_y: coords.y,
                            moved: false,
                            target_pane_id: None,
                        }));
```

- [ ] **Step 4: Compile-check**

Run:
```bash
cargo check --workspace 2>&1 | tail -30
```
Expected: `Finished`, no errors. The only field the struct gained is `pointer_id`; the construction site is the single literal that needs it. If the compiler reports a missing field elsewhere, that means a second construction site exists — find it and add `pointer_id: <value>` there too (likely `pointer_id` is from that call's own `PointerEvent`).

- [ ] **Step 5: Run tests + clippy**

Run:
```bash
cargo test --workspace 2>&1 | tail -20
cargo clippy --workspace -- -D warnings 2>&1 | tail -20
```
Expected: all tests PASS (the change is logic-only; the 5 swap tests + the walk test are unaffected), clippy clean.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/workspace/pill_drag.rs frontend/src/components/workspace/terminal_grid.rs
git commit -m "feat(workspace): guard pill-drag against multi-touch via pointer_id"
```

---

## Task 3: Fix #1 — Remove per-frame allocation in `PillDragGhost` style

**Why:** `PillDragGhost` rebuilds its `style` string via `format!` on every `pointermove` frame (~60fps), allocating a new string each frame. Most of the style is static (position, z-index, border-radius, etc. are already in the `.dnd-ghost` CSS class — only the dynamic bits are `left`/`top`/`border-color`). We trim the inline style to just the dynamic values and let the CSS class carry the rest. `border-color` is constant per drag, so it can be set once via a CSS custom property; only `left`/`top` change per frame.

**Files:**
- Modify: `frontend/src/components/workspace/pill_drag.rs` (`PillDragGhost` component).
- Modify: `frontend/public/styles.css` (`.dnd-ghost` — move `border-color` to read a `--dnd-ghost-color` custom property).

**Interfaces:**
- `PillDragGhost` consumes `Signal<Option<PillDrag>>` (unchanged). The component sets `--dnd-ghost-color` (from `source_color`) and the dynamic `left`/`top` inline.

- [ ] **Step 1: Move `border-color` to a CSS custom property in `.dnd-ghost`**

In `frontend/public/styles.css`, find the `.dnd-ghost` rule. It currently has:
```css
  border: 1px solid var(--accent);
```
Replace with:
```css
  border: 1px solid var(--dnd-ghost-color, var(--accent));
```
(`--dnd-ghost-color` is set inline by `PillDragGhost` from `source_color`; falls back to `--accent` if unset.)

- [ ] **Step 2: Trim `PillDragGhost`'s inline style to only the dynamic `left`/`top` + the color custom property**

In `frontend/src/components/workspace/pill_drag.rs`, the `PillDragGhost` component's `rsx!` currently is:
```rust
    rsx! {
        div {
            class: "dnd-ghost",
            style: "left: {d.cur_x:.0}px; top: {d.cur_y:.0}px; border-color: {d.source_color};",
            "{d.source_label}"
        }
    }
```
Replace with:
```rust
    rsx! {
        div {
            class: "dnd-ghost",
            // Per-frame: only the cursor position changes. `source_color` is
            // constant for the whole drag, so it goes on a CSS custom property
            // (set once on mount via the static part below) and the
            // `.dnd-ghost` class reads it as the border color. Keeping the
            // dynamic string to two integers avoids a per-frame `format!`
            // allocation of the full style in the ~60fps pointermove hot path.
            style: "--dnd-ghost-color: {d.source_color}; left: {d.cur_x:.0}px; top: {d.cur_y:.0}px;",
            "{d.source_label}"
        }
    }
```
(Note: `--dnd-ghost-color` repeats per frame, but it's a cheap substring of an already-cloned `d.source_color` — the win is dropping the separate `border-color` declaration and not concatenating a third token. If profiling later shows even this string is hot, the next step is a registered-asset transform; out of scope here per "default: simplest path; optimize if measured.")

- [ ] **Step 3: Compile-check**

Run:
```bash
cargo check --workspace 2>&1 | tail -20
```
Expected: `Finished`, no errors.

- [ ] **Step 4: Run tests + clippy**

Run:
```bash
cargo test --workspace 2>&1 | tail -15
cargo clippy --workspace -- -D warnings 2>&1 | tail -15
```
Expected: tests PASS, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/workspace/pill_drag.rs frontend/public/styles.css
git commit -m "perf(workspace): drop per-frame style string alloc in PillDragGhost via CSS custom property"
```

---

## Task 4: Fix #3 — Delete the dead `show_notification_toast` stub

**Why:** The branch incidentally added `show_notification_toast` as a TODO stub that's never called — dead code carried on the revival. Delete it rather than leave an orphan. (If a real toast-on-swap is desired later, it's a separate enhancement — out of scope here.)

**Files:**
- Modify: `frontend/src/components/shared/toast.rs`.

- [ ] **Step 1: Locate and confirm the stub is unused**

Run:
```bash
grep -rn "show_notification_toast" frontend/src/ 2>/dev/null
```
Expected: exactly one line — the definition in `frontend/src/components/shared/toast.rs` (around line 60-67). If any call site appears, STOP — the stub is actually used; do not delete (revisit scope with the user). If only the definition shows, proceed.

- [ ] **Step 2: Delete the stub function**

In `frontend/src/components/shared/toast.rs`, delete this block (including the preceding blank line if present):
```rust
/// Show a notification toast programmatically.
pub fn show_notification_toast(
    _toast_type: ToastType,
    _title: &str,
    _message: &str,
    _agent_type: Option<&str>,
) {
    // TODO: wire to Tauri IPC or global signal for toast dispatch
}
```
If `ToastType` was imported solely for this stub's signature, an unused-import warning will surface at compile — handle it in Step 3 if so.

- [ ] **Step 3: Compile-check (catches any orphaned import)**

Run:
```bash
cargo check --workspace 2>&1 | tail -20
```
Expected: `Finished`. If a `warning: unused import: ToastType` (or similar) appears, remove the now-unused `use ... ToastType` import from the top of `toast.rs` (it was only needed for the deleted stub), then re-run `cargo check` until clean.

- [ ] **Step 4: Run tests + clippy**

Run:
```bash
cargo test --workspace 2>&1 | tail -15
cargo clippy --workspace -- -D warnings 2>&1 | tail -15
```
Expected: tests PASS, clippy clean (no unused-import / dead-code warnings).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/shared/toast.rs
git commit -m "chore(toast): remove dead show_notification_toast stub carried by pane-pill DnD merge"
```

---

## Task 5: Final verification + manual smoke test

**Why:** The three fixes are in. Before declaring done, run the full verification gate and the manual smoke test from the spec (§9.2). This task carries no code edits — only verification.

**Files:**
- None (verification only).

- [ ] **Step 1: Full workspace check/test/clippy**

Run:
```bash
cargo test --workspace 2>&1 | tail -25
cargo clippy --workspace -- -D warnings 2>&1 | tail -25
```
Expected: all tests PASS (the 5 `swap_panes_tests` + the `walk_to_data_pane_id_none_for_none` test + the rest of the workspace's existing tests), clippy clean.

- [ ] **Step 2: Build the frontend dist (release mode) and run the debug app for manual smoke**

Run:
```bash
bash frontend/build-dist.sh
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -10
```
Expected: dist built (release), debug binary compiled. If the build-dist step fails, STOP — surface the error (likely a frontend compile issue our fixes introduced).

- [ ] **Step 3: Launch the app for the manual smoke**

Run (in a separate terminal, or background):
```bash
cargo tauri dev
```
Wait for the app window. Expected: app window appears, workspace renders with panes. (Per project memory, ignore any preexisting `use_session_signal`-in-`use_memo` runtime panic risk — out of scope. If a DIFFERENT panic appears during the smoke test, STOP and report it.)

- [ ] **Step 4: Manual smoke test matrix (spec §9.2)** — check each in the running app:

- [ ] In a 1×2 grid: grab pill A's header, drag onto pill B, release. **Confirm labels swap** (A's agent now at B's slot, B's at A's) and the running process in each pane is uninterrupted (output continues without re-spawn).
- [ ] User's exact case: with a 2-top + 1-bottom shell, drag the top-left pill onto the bottom pill. Confirm the top-left agent becomes full-width bottom, the bottom agent becomes 50% top.
- [ ] Drag a pill onto **itself** → no swap.
- [ ] Press a pill and release **without moving** (sub-threshold) → no swap; pane is still focused (click still works); double-click the title still enters rename.
- [ ] Press a pill and release over the **fullscreen icon / close icon** → no drag started (the icon's pointerdown stops propagation).
- [ ] Drag a pill and release **over the sidebar / titlebar** (off-workspace) → cancels, no swap.
- [ ] With an agent **mid-run** (e.g. a long output streaming): swap it. Confirm the process continues uninterrupted in its new slot. **Confirm scrollback is lost at both swapped slots** (the accepted cost) — the fresh xterm shows new output but not prior scrollback.
- [ ] **Multi-touch (fix #2):** with a drag in flight (pointer 1 down, moving), press with pointer 2 → pointer 2's presses are ignored; the ghost keeps following pointer 1 and the drop commits on pointer 1 up. (If you don't have a touch device, skip — this is desktop-first; the guard is verified by code review and the unit-level compile.)
- [ ] After a swap: close one pane → confirm the right pane id is removed and active is reassigned (no panic, no stale `pill_drag`, no panic on next render).
- [ ] Quit the app, relaunch → confirm the swapped `panes[]` order **persisted** across restart.

- [ ] **Step 5: Final commit (if any artifacts) and push**

Run:
```bash
git status --short
git push origin feat/pane-pill-drag-swap-revival
```
Expected: clean tree (all fixes already committed in Tasks 2-4); branch pushed. The revival branch is now ready for a PR to `main`.

- [ ] **Step 6: Update project memory**

Append a pointer line to `MEMORY.md`:
```
- [Pane-pill DnD revival](project-pane-pill-dnd-revival.md) — revival of feat/pane-pill-drag-swap; pointer-event drag, full session migration, accepted scrollback-loss cost; inherited use_session_signal-in-use_memo panic left for separate ticket
```
And write `memory/project-pane-pill-dnd-revival.md` (frontmatter `type: project`) summarizing: branch name (`feat/pane-pill-drag-swap-revival`), the three fixes applied, the accepted tradeoffs (scrollback loss; inherited panic out of scope), and the merge provenance (revived from unmerged `feat/pane-pill-drag-swap`).

---

## Self-Review (writer's pass)

**1. Spec coverage:**
- §3.1 merge → Task 1. ✓
- §5.1 Fix #1 (ghost hot-path alloc) → Task 3. ✓
- §5.2 Fix #2 (pointer_id) → Task 2. ✓
- §5.3 Fix #3 (delete dead stub) → Task 4. ✓
- §3.2 inherited panic **not** fixed → explicitly excluded; Task 0 Step 1 sanity guard notes it; no task fixes it (correct). ✓
- §9.1 unit tests → Task 1 Step 4 runs them; they shipped from the merge unchanged. ✓
- §9.2 manual smoke → Task 5 Step 4 (full matrix). ✓
- §9.3 E2E → explicitly out of scope; no task. ✓
- §12 branch hygiene (drop stale `types/theme.rs`, unrelated `state.rs`) → Task 1 Step 1 verifies `types/theme.rs` doesn't appear in merge; `state.rs` not in scope (backend untouched). ✓
- Pre-merge dirty working tree handling → Task 0 (park perf edits + branch first). ✓

**2. Placeholder scan:** No TBD/TODO/"implement later"/"similar to Task N". Every code step shows full code. ✓

**3. Type consistency:**
- `PillDrag.pointer_id: i32` — set in Task 2 Step 3 to `e.data.pointer_id().unwrap_or(-1)`; read in Task 2 Step 2 guards as `e.data.pointer_id() == Some(current.pointer_id)`. Consistent (`Option<i32>` vs `i32` via the `Some(...)` compare). ✓
- `--dnd-ghost-color` set inline in Task 3 Step 2 and consumed in CSS Task 3 Step 1. ✓
- `swap_pane_agents(&self, space_id, a, b)` signature matches the merge (Task 1) and the call site in `PillDragOverlay` (Task 2 Step 2 preserves the existing call). ✓
- `swap_panes_by_id(&mut Space, a, b) -> bool` — shipped from merge, not touched by any fix. ✓

No issues found.
