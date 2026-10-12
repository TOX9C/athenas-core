//! Pure workspace layout helpers.

use crate::types::workspace::{GridTemplate, Space};

/// Select the smallest grid template that can hold the given pane count.
pub fn grid_for_pane_count(count: usize) -> GridTemplate {
    if count <= 1 {
        GridTemplate::X1x1
    } else if count <= 2 {
        GridTemplate::X1x2
    } else if count <= 4 {
        GridTemplate::X2x2
    } else if count <= 6 {
        GridTemplate::X2x3
    } else if count <= 9 {
        GridTemplate::X3x3
    } else if count <= 12 {
        GridTemplate::X3x4
    } else {
        GridTemplate::X4x4
    }
}

/// Growth-only template policy for pane add/remove: keep the current
/// template while it has spare capacity; grow to the smallest fitting
/// template only when exceeded. Never downgrades — a user-chosen template
/// (e.g. 4 panes in a 3x3 grid) survives adding a 5th pane as a 3x3 with
/// two empty slots... which grows on the 10th. This stops the gratuitous
/// whole-grid reflow (every pane resizing at once) on routine pane adds.
pub fn ensure_grid_capacity(current: GridTemplate, pane_count: usize) -> GridTemplate {
    if current.capacity() < pane_count {
        grid_for_pane_count(pane_count)
    } else {
        current
    }
}

/// Swap two panes within a space by pane id — *pure* core of
/// [`WorkspaceState::swap_pane_agents`], extracted so the swap logic is
/// unit-testable on the host without the WASM-only `save()` path. Returns
/// `true` if a swap occurred. No-op (returns `false`) if either pane id is
/// missing from the space or the two ids are equal. Slot indices — and
/// therefore each slot's flex-grow size — are unchanged; only the `PaneConfig`
/// values at the two indices trade places (full session migration: `id`,
/// and the PTY session it keys, follows the agent).
pub fn swap_panes_by_id(space: &mut Space, pane_id_a: &str, pane_id_b: &str) -> bool {
    if pane_id_a == pane_id_b {
        return false;
    }
    let ia = space.panes.iter().position(|p| p.id == pane_id_a);
    let ib = space.panes.iter().position(|p| p.id == pane_id_b);
    match (ia, ib) {
        (Some(ia), Some(ib)) => {
            space.panes.swap(ia, ib);
            true
        }
        // one or both ids absent — leave the space untouched
        _ => false,
    }
}

#[cfg(test)]
mod swap_panes_tests {
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

    // NOTE: tests exercise `swap_panes_by_id` (the pure free function), not
    // `swap_pane_agents`. The method is a thin persistence wrapper around
    // `swap_panes_by_id` + `update_space`/`save()`, and `save()` touches
    // js-sys statics that panic on the non-wasm host test target
    // (`cannot access imported statics on non-wasm targets`). Testing the
    // extracted pure core keeps the swap semantics fully covered on the
    // host, mirroring how `grid_for_pane_count` is itself a host-testable
    // free function. The method's wiring (does-it-call-`swap_panes_by_id`)
    // is a compile-time guarantee plus manual smoke verification (plan Task 5).

    #[test]
    fn swaps_two_panes_by_id_full_config_including_id() {
        let mut space = space_with_panes(&["alpha", "beta", "shell"]);
        assert!(swap_panes_by_id(&mut space, "alpha", "beta"));
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
        let mut space = space_with_panes(&["a", "b", "c", "d"]);
        assert!(swap_panes_by_id(&mut space, "a", "d"));
        // slot 0 (top-left) now holds what was at slot 3 (bottom-right)
        assert_eq!(space.panes[0].id, "d");
        assert_eq!(space.panes[3].id, "a");
    }

    #[test]
    fn noop_when_ids_equal() {
        let mut space = space_with_panes(&["a", "b"]);
        assert!(!swap_panes_by_id(&mut space, "a", "a"));
        assert_eq!(space.panes[0].id, "a");
        assert_eq!(space.panes[1].id, "b");
    }

    #[test]
    fn noop_when_pane_id_missing() {
        let mut space = space_with_panes(&["a", "b"]);
        // first id missing
        assert!(!swap_panes_by_id(&mut space, "ghost", "a"));
        // second id missing
        assert!(!swap_panes_by_id(&mut space, "a", "ghost"));
        // both missing
        assert!(!swap_panes_by_id(&mut space, "x", "y"));
        assert_eq!(space.panes[0].id, "a");
        assert_eq!(space.panes[1].id, "b");
    }

    #[test]
    fn preserves_unrelated_panes_and_grid_template() {
        let mut space = space_with_panes(&["a", "b", "c", "shell"]);
        let grid_before = space.grid;
        assert!(swap_panes_by_id(&mut space, "a", "shell"));
        assert_eq!(space.panes.len(), 4);
        assert_eq!(space.grid, grid_before);
    }
}

#[cfg(test)]
mod ensure_grid_capacity_tests {
    use super::*;
    use crate::types::workspace::GridTemplate;

    #[test]
    fn keeps_template_with_spare_capacity_on_add() {
        // 5 panes in a user-chosen 3x3 (capacity 9): the old unconditional
        // grid_for_pane_count would downgrade to 2x3, reflowing every pane.
        assert_eq!(
            ensure_grid_capacity(GridTemplate::X3x3, 5),
            GridTemplate::X3x3
        );
    }

    #[test]
    fn grows_only_when_exceeded() {
        assert_eq!(
            ensure_grid_capacity(GridTemplate::X2x2, 5),
            GridTemplate::X2x3
        );
        // Exactly at capacity stays.
        assert_eq!(
            ensure_grid_capacity(GridTemplate::X2x2, 4),
            GridTemplate::X2x2
        );
    }

    #[test]
    fn growth_matches_grid_for_pane_count() {
        assert_eq!(
            ensure_grid_capacity(GridTemplate::X1x1, 2),
            grid_for_pane_count(2)
        );
        assert_eq!(
            ensure_grid_capacity(GridTemplate::X2x3, 7),
            grid_for_pane_count(7)
        );
    }

    #[test]
    fn never_shrinks() {
        assert_eq!(
            ensure_grid_capacity(GridTemplate::X4x4, 3),
            GridTemplate::X4x4
        );
    }

    #[test]
    fn template_dimensions_are_consistent() {
        let all = [
            (GridTemplate::X1x1, 1, 1),
            (GridTemplate::X1x2, 2, 1),
            (GridTemplate::X2x2, 2, 2),
            (GridTemplate::X2x3, 3, 2),
            (GridTemplate::X3x2, 2, 3),
            (GridTemplate::X3x3, 3, 3),
            (GridTemplate::X3x4, 4, 3),
            (GridTemplate::X4x4, 4, 4),
        ];
        // Variant names and strum labels follow rows x cols ("1x2" = 1 row,
        // 2 columns); the selector's display label is cols x rows (pre-existing).
        for (t, cols, rows) in all {
            assert_eq!(t.columns(), cols, "{:?} columns", t);
            assert_eq!(t.rows(), rows, "{:?} rows", t);
            assert_eq!(t.capacity(), cols * rows, "{:?} capacity", t);
            assert_eq!(format!("{}", t), format!("{rows}x{cols}"), "{:?} label", t);
        }
    }
}
