//! Per-agent message queue: messages typed while an agent (terminal pane or
//! chat) is busy are queued here, with reorder/delete affordances, and
//! delivered (by the caller — chat submit or PTY write) when the agent
//! returns to a prompt.
//!
//! Bounded per queue: past `MAX_QUEUED_PER_PANE` the oldest message is
//! evicted so a paste loop cannot grow memory without bound.

use dioxus::prelude::*;
use std::collections::{HashMap, VecDeque};

#[path = "agent_queue_model.rs"]
mod agent_queue_model;
pub use agent_queue_model::QueuedMessage;

const MAX_QUEUED_PER_PANE: usize = 50;

#[derive(Clone, PartialEq, Default)]
pub struct AgentQueueState {
    /// Queue per agent pane id (terminal) or the fixed "chat" scope.
    pub queues: HashMap<String, VecDeque<QueuedMessage>>,
}

impl AgentQueueState {
    pub fn enqueue(&mut self, scope: &str, text: &str) -> Option<String> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        // Millisecond ids collide under fast loops; add a per-process counter.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = format!(
            "q-{}-{}",
            crate::utils::time::now_ms(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let queue = self.queues.entry(scope.to_string()).or_default();
        queue.push_back(QueuedMessage {
            id,
            text: text.to_string(),
        });
        while queue.len() > MAX_QUEUED_PER_PANE {
            queue.pop_front();
        }
        None
    }

    /// Pop the next queued message for delivery.
    pub fn take_next(&mut self, scope: &str) -> Option<QueuedMessage> {
        let queue = self.queues.get_mut(scope)?;
        let msg = queue.pop_front();
        if queue.is_empty() {
            self.queues.remove(scope);
        }
        msg
    }

    pub fn remove(&mut self, scope: &str, id: &str) {
        if let Some(queue) = self.queues.get_mut(scope) {
            queue.retain(|m| m.id != id);
            if queue.is_empty() {
                self.queues.remove(scope);
            }
        }
    }

    /// Move a queued message one slot toward the front.
    pub fn move_up(&mut self, scope: &str, id: &str) {
        if let Some(queue) = self.queues.get_mut(scope) {
            if let Some(idx) = queue.iter().position(|m| m.id == id) {
                if idx > 0 {
                    queue.swap(idx - 1, idx);
                }
            }
        }
    }

    /// Move a queued message one slot toward the back.
    pub fn move_down(&mut self, scope: &str, id: &str) {
        if let Some(queue) = self.queues.get_mut(scope) {
            if let Some(idx) = queue.iter().position(|m| m.id == id) {
                if idx + 1 < queue.len() {
                    queue.swap(idx, idx + 1);
                }
            }
        }
    }

    pub fn queued(&self, scope: &str) -> Vec<QueuedMessage> {
        self.queues
            .get(scope)
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn is_empty(&self, scope: &str) -> bool {
        self.queues.get(scope).map(|q| q.is_empty()).unwrap_or(true)
    }
}

pub fn provide_agent_queue_store() {
    use_context_provider(|| Signal::new(AgentQueueState::default()));
}

pub fn use_agent_queue_store() -> Signal<AgentQueueState> {
    use_context::<Signal<AgentQueueState>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_order_and_take_next() {
        let mut s = AgentQueueState::default();
        s.enqueue("p1", "first");
        s.enqueue("p1", "second");
        s.enqueue("p1", "third");
        assert_eq!(s.take_next("p1").unwrap().text, "first");
        assert_eq!(s.take_next("p1").unwrap().text, "second");
        assert_eq!(s.take_next("p1").unwrap().text, "third");
        assert!(s.take_next("p1").is_none());
    }

    #[test]
    fn reorder_and_remove() {
        let mut s = AgentQueueState::default();
        s.enqueue("p1", "a");
        s.enqueue("p1", "b");
        s.enqueue("p1", "c");
        let ids: Vec<String> = s.queued("p1").iter().map(|m| m.id.clone()).collect();
        s.move_up("p1", &ids[2]);
        let texts: Vec<String> = s.queued("p1").into_iter().map(|m| m.text).collect();
        assert_eq!(texts, vec!["a", "c", "b"]);
        s.move_down("p1", &ids[0]);
        let texts: Vec<String> = s.queued("p1").into_iter().map(|m| m.text).collect();
        assert_eq!(texts, vec!["c", "a", "b"]);
        s.remove("p1", &ids[1]);
        let texts: Vec<String> = s.queued("p1").into_iter().map(|m| m.text).collect();
        assert_eq!(texts, vec!["c", "a"]); // a was swapped down past b; removing b leaves [c, a]
    }

    #[test]
    fn capped_at_50_oldest_evicted() {
        let mut s = AgentQueueState::default();
        for i in 0..60 {
            s.enqueue("p1", &format!("m{i}"));
        }
        let q = s.queued("p1");
        assert_eq!(q.len(), 50);
        assert_eq!(q[0].text, "m10");
        assert_eq!(q[49].text, "m59");
    }

    #[test]
    fn empty_scope_is_empty_and_safe() {
        let s = AgentQueueState::default();
        assert!(s.is_empty("missing"));
        assert!(s.queued("missing").is_empty());
        let mut s = s;
        assert!(s.take_next("missing").is_none());
        s.remove("missing", "nope"); // no panic
    }
}
