//! Low-frequency terminal coordination kept outside the root application shell.
//!
//! Per-pane terminal data is reactive through `TerminalRegistry`. This controller
//! owns only cross-pane membership/selection work so terminal-store reads do not
//! subscribe the high-level `App` component to terminal lifecycle changes.

use dioxus::prelude::*;

use crate::stores::terminal::{use_terminal_registry, use_terminal_store};
use crate::stores::workspace::use_workspace_store;

fn select_active_pane(current: Option<&str>, pane_ids: &[String]) -> Option<String> {
    current
        .filter(|id| pane_ids.iter().any(|pane_id| pane_id == id))
        .map(str::to_string)
        .or_else(|| pane_ids.first().cloned())
}

#[derive(Props, Clone, PartialEq)]
pub struct TerminalControllerProps {
    /// Incremented by the root keyboard handler when Cmd+W requests closing the
    /// active pane. A counter is used instead of a boolean so repeated requests
    /// cannot be lost while the async PTY kill is in flight.
    pub close_request: Signal<u64>,
}

/// Coordinates terminal membership and active-pane selection without rendering
/// UI. This component is mounted unconditionally for the lifetime of the app.
#[component]
pub fn TerminalController(props: TerminalControllerProps) -> Element {
    crate::utils::perf_metrics::mark_render("TerminalController");
    let workspace = use_workspace_store();
    let mut terminal_store = use_terminal_store();
    let terminal_registry = use_terminal_registry();

    // Keep the active pane valid when switching spaces. This effect is scoped to
    // this controller, so changes to TerminalStore cannot invalidate App's shell.
    use_effect(move || {
        let active_space_pane_ids = {
            let state = workspace.read();
            state
                .active_space_id
                .as_ref()
                .and_then(|space_id| {
                    state
                        .spaces
                        .iter()
                        .find(|space| &space.id == space_id)
                        .map(|space| {
                            space
                                .panes
                                .iter()
                                .map(|pane| pane.id.clone())
                                .collect::<Vec<String>>()
                        })
                })
                .unwrap_or_default()
        };

        let current_active = terminal_store.read().active_session_id.clone();
        let Some(selected_pane_id) =
            select_active_pane(current_active.as_deref(), &active_space_pane_ids)
        else {
            return;
        };

        if current_active.as_deref() != Some(selected_pane_id.as_str()) {
            terminal_store.write().set_active(selected_pane_id);
        }
    });

    // Consume close requests exactly once. The root performs the editable-field
    // guard and only increments this signal for an actual application shortcut.
    // While a PTY kill is in flight, leave the request unconsumed; the effect is
    // retried when the in-flight flag clears, so rapid Cmd+W presses close panes
    // serially instead of issuing duplicate kills for one pane.
    let mut last_close_request = use_signal(|| 0_u64);
    let mut close_in_flight = use_signal(|| false);
    use_effect(move || {
        let request = (props.close_request)();
        if request == last_close_request() || close_in_flight() {
            return;
        }
        last_close_request.set(request);

        let (space_id, pane_id) = {
            let state = workspace.read();
            let space_id = state.active_space_id.clone();
            let pane_id = space_id.as_ref().and_then(|id| {
                state
                    .spaces
                    .iter()
                    .find(|space| space.id == *id)
                    .and_then(|space| {
                        let active = terminal_store.read().active_session_id.clone();
                        active
                            .filter(|pane_id| space.panes.iter().any(|pane| &pane.id == pane_id))
                            .or_else(|| space.panes.first().map(|pane| pane.id.clone()))
                    })
            });
            (space_id, pane_id)
        };

        let (Some(space_id), Some(pane_id)) = (space_id, pane_id) else {
            return;
        };

        // Kill first, then remove the pane. If IPC fails, the visible pane
        // remains available instead of leaving an untracked backend PTY behind.
        close_in_flight.set(true);
        let registry_for_kill = terminal_registry.clone();
        let mut workspace_for_kill = workspace;
        let mut terminal_store_for_kill = terminal_store;
        let mut close_in_flight_for_spawn = close_in_flight;
        spawn(async move {
            if crate::tauri_bridge::pty_kill(&pane_id).await.is_ok() {
                workspace_for_kill
                    .write()
                    .remove_pane_from_space(&space_id, &pane_id);
                registry_for_kill.mark_closing(&pane_id);
                let mut store = terminal_store_for_kill.write();
                store.known_pane_ids.remove(&pane_id);
                if store.active_session_id.as_deref() == Some(pane_id.as_str()) {
                    store.active_session_id = store.known_pane_ids.iter().next().cloned();
                }
                store.generation = store.generation.wrapping_add(1);
            }
            close_in_flight_for_spawn.set(false);
        });
    });

    rsx! {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn preserves_active_pane_when_it_is_in_the_current_space() {
        let pane_ids = vec!["pane-1".to_string(), "pane-2".to_string()];
        assert_eq!(
            super::select_active_pane(Some("pane-2"), &pane_ids),
            Some("pane-2".to_string())
        );
    }

    #[test]
    fn falls_back_to_first_pane_when_active_pane_is_not_visible() {
        let pane_ids = vec!["pane-1".to_string(), "pane-2".to_string()];
        assert_eq!(
            super::select_active_pane(Some("other-space-pane"), &pane_ids),
            Some("pane-1".to_string())
        );
        assert_eq!(super::select_active_pane(None, &[]), None);
    }
}
