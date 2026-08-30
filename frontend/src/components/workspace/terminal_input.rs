//! Shared, per-pane PTY input serialization.
//!
//! xterm.js emits input synchronously while Tauri IPC is asynchronous. Keeping
//! one queue per pane prevents keyboard, clipboard, and native file-drop input
//! from overtaking one another.

use crate::tauri_bridge::pty_write;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct InputChannel {
    queue: std::collections::VecDeque<String>,
    draining: bool,
    active: bool,
}

#[derive(Clone, Default)]
pub struct TerminalInputRouter {
    channels: Rc<RefCell<HashMap<String, Rc<RefCell<InputChannel>>>>>,
}

impl TerminalInputRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Activate a pane. An in-flight channel is reused during a remount so its
    /// pending PTY write remains ordered; an idle channel is created lazily.
    pub fn activate(&self, pane_id: &str) {
        let channel = {
            let mut channels = self.channels.borrow_mut();
            channels
                .entry(pane_id.to_string())
                .or_insert_with(|| Rc::new(RefCell::new(InputChannel::default())))
                .clone()
        };
        channel.borrow_mut().active = true;
    }

    /// Stop and clear a pane's queue before its xterm mount is disposed.
    pub fn deactivate(&self, pane_id: &str) {
        let channel = self.channels.borrow().get(pane_id).cloned();
        let Some(channel) = channel else { return };

        let remove_now = {
            let mut state = channel.borrow_mut();
            state.active = false;
            state.queue.clear();
            !state.draining
        };

        if remove_now {
            remove_channel_if_same(&self.channels, pane_id, &channel);
        }
    }

    /// Enqueue input for an active pane. All callers use this method, so a
    /// dropped path cannot race normal xterm input or bracketed clipboard paste.
    pub fn enqueue(&self, pane_id: &str, data: impl Into<String>) {
        let Some(channel) = self.channels.borrow().get(pane_id).cloned() else {
            return;
        };
        enqueue_channel(
            self.channels.clone(),
            channel,
            pane_id.to_string(),
            data.into(),
        );
    }

    #[cfg(test)]
    fn channel_count(&self) -> usize {
        self.channels.borrow().len()
    }
}

fn remove_channel_if_same(
    channels: &Rc<RefCell<HashMap<String, Rc<RefCell<InputChannel>>>>>,
    pane_id: &str,
    channel: &Rc<RefCell<InputChannel>>,
) {
    let mut channels = channels.borrow_mut();
    if channels
        .get(pane_id)
        .is_some_and(|current| Rc::ptr_eq(current, channel))
    {
        channels.remove(pane_id);
    }
}

fn enqueue_channel(
    channels: Rc<RefCell<HashMap<String, Rc<RefCell<InputChannel>>>>>,
    channel: Rc<RefCell<InputChannel>>,
    pane_id: String,
    data: String,
) {
    {
        let mut state = channel.borrow_mut();
        if !state.active {
            return;
        }
        state.queue.push_back(data);
        if state.draining {
            return;
        }
        state.draining = true;
    }

    wasm_bindgen_futures::spawn_local(async move {
        loop {
            let next = {
                let mut state = channel.borrow_mut();
                if !state.active {
                    state.queue.clear();
                    state.draining = false;
                    drop(state);
                    remove_channel_if_same(&channels, &pane_id, &channel);
                    return;
                }
                match state.queue.pop_front() {
                    Some(data) => data,
                    None => {
                        state.draining = false;
                        return;
                    }
                }
            };

            if let Err(error) = pty_write(&pane_id, &next).await {
                web_sys::console::error_1(
                    &format!("TerminalInputRouter: pty_write failed: {error:?}").into(),
                );
            }
        }
    });
}

/// Quote one path as a literal POSIX shell argument.
pub fn shell_quote_path(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

/// Format a native drop as shell arguments. A trailing space lets the user
/// continue typing without executing the command.
pub fn format_dropped_paths(paths: &[String]) -> Option<String> {
    let paths = paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(|path| shell_quote_path(path))
        .collect::<Vec<_>>();
    (!paths.is_empty()).then(|| format!("{} ", paths.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::{format_dropped_paths, shell_quote_path};

    #[test]
    fn quotes_shell_metacharacters_and_spaces() {
        assert_eq!(
            shell_quote_path("/tmp/a b; $(touch nope).png"),
            "'/tmp/a b; $(touch nope).png'"
        );
    }

    #[test]
    fn escapes_apostrophes() {
        assert_eq!(
            shell_quote_path("/tmp/user's shot.png"),
            "'/tmp/user'\\''s shot.png'"
        );
    }

    #[test]
    fn formats_multiple_paths_with_trailing_space() {
        assert_eq!(
            format_dropped_paths(&["/tmp/a.png".into(), "/tmp/b file.txt".into()]),
            Some("'/tmp/a.png' '/tmp/b file.txt' ".into())
        );
    }

    #[test]
    fn empty_paths_are_ignored() {
        assert_eq!(format_dropped_paths(&[String::new()]), None);
    }

    #[test]
    fn deactivating_an_idle_pane_releases_its_channel() {
        let router = super::TerminalInputRouter::new();
        router.activate("pane-a");
        assert_eq!(router.channel_count(), 1);

        router.deactivate("pane-a");

        assert_eq!(router.channel_count(), 0);
    }
}
