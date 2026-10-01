//! Queue-or-send helper shared by the status bar queue input: messages
//! typed while the pane's agent is busy queue and flush on prompt return;
//! when idle they write straight through so nothing feels laggy.

use crate::stores::agent_queue::AgentQueueState;
use dioxus::prelude::{Signal, WritableExt};

pub fn queue_transfer(
    mut queue: Signal<AgentQueueState>,
    pane_id: &str,
    text: &str,
    busy: bool,
) {
    if text.trim().is_empty() {
        return;
    }
    if busy {
        queue.write().enqueue(pane_id, text);
        return;
    }
    let pane_id = pane_id.to_string();
    let text = text.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = crate::tauri_bridge::pty_write(&pane_id, &format!("{}\n", text)).await;
    });
}
