use dioxus::prelude::*;
use std::collections::VecDeque;

const MAX_BLOCKS: usize = 100;
/// Hard cap on accumulated output for a single block (current_output + block.output).
/// Prevents unbounded heap growth on long-running commands like `yes` or `tail -f`.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024; // 256 KiB
const TRUNCATION_MARKER: &str = "\n[...truncated at 256 KiB...]";

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalBlock {
    pub id: u64,
    pub command: String,
    pub output: String,
    pub exit_status: Option<i32>,
    pub start_time_ms: u64,
    pub end_time_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerminalBlocksStore {
    pub blocks: VecDeque<TerminalBlock>,
    pub current_command: Option<String>,
    pub current_output: String,
    pub generation: u64,
    pub next_id: u64,
}

impl TerminalBlocksStore {
    pub fn on_prompt_start(&mut self) {
        self.current_output.clear();
        self.current_command = None;
    }

    pub fn on_command_start(&mut self, command: String) {
        while self.blocks.len() >= MAX_BLOCKS {
            self.blocks.pop_front();
        }
        let id = self.next_id;
        self.next_id += 1;
        let block = TerminalBlock {
            id,
            command: command.clone(),
            output: String::new(),
            exit_status: None,
            start_time_ms: js_sys::Date::now() as u64,
            end_time_ms: None,
        };
        self.blocks.push_back(block);
        self.current_command = Some(command);
        self.current_output.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn on_command_finished(&mut self, status: i32) {
        if let Some(block) = self.blocks.back_mut() {
            if block.exit_status.is_none() {
                block.exit_status = Some(status);
                block.end_time_ms = Some(js_sys::Date::now() as u64);
                self.generation = self.generation.wrapping_add(1);
            }
        }
        self.current_output.clear();
        self.current_command = None;
    }

    pub fn append_output(&mut self, text: &str) {
        append_capped(&mut self.current_output, text);
        if let Some(block) = self.blocks.back_mut() {
            if block.exit_status.is_none() {
                append_capped(&mut block.output, text);
            }
        }
    }
}

/// Append `incoming` to `buf`, capping total length at `MAX_OUTPUT_BYTES`.
/// Drops oldest bytes on overflow and inserts a single truncation marker
/// the first time the cap is exceeded. Subsequent overflows drop the head
/// of the buffer to stay within the cap.
fn append_capped(buf: &mut String, incoming: &str) {
    if incoming.is_empty() {
        return;
    }
    let cap = MAX_OUTPUT_BYTES;
    let marker = TRUNCATION_MARKER;

    // Fast path: incoming fits without hitting the cap.
    if buf.len() + incoming.len() <= cap {
        buf.push_str(incoming);
        return;
    }

    if !buf.contains(marker) {
        // First truncation: drop oldest, then append marker + incoming.
        let needed = buf.len() + incoming.len() - cap + marker.len();
        let drop = needed.min(buf.len());
        if drop > 0 {
            // Advance to a char boundary to keep UTF-8 valid.
            let mut start = drop;
            while start < buf.len() && !buf.is_char_boundary(start) {
                start += 1;
            }
            let mut new_buf = String::with_capacity(cap);
            new_buf.push_str(&buf[start..]);
            new_buf.push_str(marker);
            new_buf.push_str(incoming);
            if new_buf.len() > cap {
                let excess = new_buf.len() - cap;
                let mut cut = excess;
                while cut < new_buf.len() && !new_buf.is_char_boundary(cut) {
                    cut += 1;
                }
                new_buf.drain(..cut);
            }
            *buf = new_buf;
        } else {
            // Buffer was empty but incoming alone exceeds cap: trim incoming and
            // prepend the marker.
            let mut cut = incoming.len().saturating_sub(cap);
            while cut < incoming.len() && !incoming.is_char_boundary(cut) {
                cut += 1;
            }
            buf.push_str(marker);
            buf.push_str(&incoming[cut..]);
            if buf.len() > cap {
                let excess = buf.len() - cap;
                let mut cut2 = excess;
                while cut2 < buf.len() && !buf.is_char_boundary(cut2) {
                    cut2 += 1;
                }
                buf.drain(..cut2);
            }
        }
        return;
    }

    // Already truncated previously: just append, then trim head to cap.
    buf.push_str(incoming);
    if buf.len() > cap {
        let excess = buf.len() - cap;
        let mut cut = excess;
        while cut < buf.len() && !buf.is_char_boundary(cut) {
            cut += 1;
        }
        buf.drain(..cut);
    }
}

/// Create the signal-based store.
pub fn use_terminal_blocks_store() -> Signal<TerminalBlocksStore> {
    use_context::<Signal<TerminalBlocksStore>>()
}

/// Initialize the store as a context provider.
pub fn provide_terminal_blocks_store() {
    use_context_provider(|| Signal::new(TerminalBlocksStore::default()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_output_caps_at_max_bytes() {
        let mut store = TerminalBlocksStore::default();
        store.on_command_start("yes".to_string());

        let chunk = "x".repeat(1024); // 1 KiB
        for _ in 0..300 {
            store.append_output(&chunk);
        }

        let bound = MAX_OUTPUT_BYTES + TRUNCATION_MARKER.len();
        assert!(
            store.current_output.len() <= bound,
            "current_output {} exceeds bound {bound}",
            store.current_output.len()
        );
        assert!(
            store.current_output.contains(TRUNCATION_MARKER),
            "truncation marker missing from current_output"
        );

        // Block output is also capped.
        let block = store.blocks.back().expect("block exists");
        assert!(
            block.output.len() <= bound,
            "block.output {} exceeds bound {bound}",
            block.output.len()
        );
        assert!(
            block.output.contains(TRUNCATION_MARKER),
            "truncation marker missing from block.output"
        );
    }

    #[test]
    fn append_output_marker_inserted_only_once() {
        let mut store = TerminalBlocksStore::default();
        store.on_command_start("yes".to_string());
        let chunk = "a".repeat(2048);
        for _ in 0..500 {
            store.append_output(&chunk);
        }
        let count = store.current_output.matches(TRUNCATION_MARKER).count();
        assert_eq!(count, 1, "truncation marker should appear exactly once");
    }

    #[test]
    fn append_output_preserves_recent_data() {
        let mut store = TerminalBlocksStore::default();
        store.on_command_start("echo tail".to_string());

        let filler = "f".repeat(MAX_OUTPUT_BYTES);
        store.append_output(&filler);
        store.append_output("TAIL_MARKER_42");

        assert!(
            store.current_output.contains("TAIL_MARKER_42"),
            "recent tail data should survive cap"
        );
        assert!(store.current_output.len() <= MAX_OUTPUT_BYTES + TRUNCATION_MARKER.len());
    }

    #[test]
    fn append_output_no_truncation_under_cap() {
        let mut store = TerminalBlocksStore::default();
        store.on_command_start("ls".to_string());
        store.append_output("hello world\n");
        assert_eq!(store.current_output, "hello world\n");
        assert!(!store.current_output.contains(TRUNCATION_MARKER));
    }

    #[test]
    fn append_capped_drops_oldest_on_overflow() {
        let mut buf = String::from("OLD");
        // Push enough to exceed cap.
        let incoming = "Z".repeat(MAX_OUTPUT_BYTES);
        append_capped(&mut buf, &incoming);
        assert!(buf.len() <= MAX_OUTPUT_BYTES + TRUNCATION_MARKER.len());
        assert!(buf.contains(TRUNCATION_MARKER));
    }
}
