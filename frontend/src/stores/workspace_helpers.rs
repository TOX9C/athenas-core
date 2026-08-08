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
