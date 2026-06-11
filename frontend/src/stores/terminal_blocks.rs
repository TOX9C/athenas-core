use dioxus::prelude::*;
use std::collections::VecDeque;

const MAX_BLOCKS: usize = 100;
/// Hard cap on accumulated output for a single block (current_output + block.output).
/// Prevents unbounded heap growth on long-running commands like `yes` or `tail -f`.
const MAX_OUTPUT_BYTES: usize = 256 * 1024; // 256 KiB
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
/// If the cap is reached, drop the oldest bytes and insert a single
/// truncation marker (only on the first truncation). New data that arrives
/// after the cap is still appended (up to the cap) so the most recent output
/// is always visible.
fn append_capped(buf: &mut String, incoming: &str) {
    if incoming.is_empty() {
        return;
    }
    let cap = MAX_OUTPUT_BYTES;
    let marker = TRUNCATION_MARKER;
    let marker_len = marker.len();

    // Fast path: incoming fits without hitting the cap.
    if buf.len() + incoming.len() <= cap {
        buf.push_str(incoming);
        return;
    }

    // Compose the would-be buffer without the marker, then prepend the marker
    // and trim to `cap` from the right. This guarantees the marker is always
    // present after the first overflow, and the total length never exceeds
    // `cap` (we keep the *most recent* `cap - marker_len` bytes).
    let has_marker = buf.contains(marker);

    if has_marker {
        // Drop oldest bytes (skipping past the marker), then append incoming.
        let marker_pos = buf.find(marker).expect("marker present");
        let after_marker = marker_pos + marker_len;
        // Tail = everything after the marker + incoming, then trim to cap-marker_len.
        let mut tail = String::with_capacity(cap);
        tail.push_str(&buf[after_marker..]);
        tail.push_str(incoming);
        let keep = cap - marker_len;
        if tail.len() > keep {
            // Trim from the front, respecting char boundaries.
            let excess = tail.len() - keep;
            let mut cut = excess;
            while cut < tail.len() && !tail.is_char_boundary(cut) {
                cut += 1;
            }
            tail.drain(..cut);
        }
        let mut out = String::with_capacity(cap);
        out.push_str(marker);
        out.push_str(&tail);
        *buf = out;
    } else {
        // First overflow. The kept payload is `cap - marker_len` bytes drawn
        // from the most recent data (existing buf tail + incoming).
        let keep = cap - marker_len;
        let mut payload = String::with_capacity(keep);
        payload.push_str(buf);
        payload.push_str(incoming);
        if payload.len() > keep {
            let excess = payload.len() - keep;
            let mut cut = excess;
            while cut < payload.len() && !payload.is_char_boundary(cut) {
                cut += 1;
            }
            payload.drain(..cut);
        }
        let mut out = String::with_capacity(cap);
        out.push_str(marker);
        out.push_str(&payload);
        *buf = out;
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

    fn make_store() -> TerminalBlocksStore {
        // Bypass on_command_start (uses js_sys::Date) so tests run on host.
        let block = TerminalBlock {
            id: 0,
            command: "test".to_string(),
            output: String::new(),
            exit_status: None,
            start_time_ms: 0,
            end_time_ms: None,
        };
        TerminalBlocksStore {
            blocks: VecDeque::from(vec![block]),
            current_command: Some("test".to_string()),
            current_output: String::new(),
            generation: 0,
            next_id: 1,
        }
    }

    #[test]
    fn append_output_caps_at_max_bytes() {
        let mut store = make_store();
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

        // Block output should also be capped.
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
        let mut store = make_store();
        let chunk = "a".repeat(2048);
        for _ in 0..500 {
            store.append_output(&chunk);
        }
        let count = store.current_output.matches(TRUNCATION_MARKER).count();
        assert_eq!(count, 1, "truncation marker should appear exactly once");

        let block = store.blocks.back().expect("block exists");
        let count_block = block.output.matches(TRUNCATION_MARKER).count();
        assert_eq!(
            count_block, 1,
            "truncation marker should appear exactly once in block.output"
        );
    }

    #[test]
    fn append_output_preserves_recent_data() {
        let mut store = make_store();
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
        let mut store = make_store();
        store.append_output("hello world\n");
        assert_eq!(store.current_output, "hello world\n");
        assert!(!store.current_output.contains(TRUNCATION_MARKER));
    }

    #[test]
    fn append_output_handles_incoming_larger_than_cap() {
        let mut store = make_store();
        let huge = "z".repeat(MAX_OUTPUT_BYTES * 2);
        store.append_output(&huge);
        assert!(store.current_output.len() <= MAX_OUTPUT_BYTES + TRUNCATION_MARKER.len());
        assert!(store.current_output.contains(TRUNCATION_MARKER));
    }

    #[test]
    fn append_output_respects_utf8_boundaries() {
        let mut store = make_store();
        // Multi-byte chars near the boundary exercise the is_char_boundary logic.
        let chunk = "\u{1F600}".repeat(50_000); // 4-byte emoji
        store.append_output(&chunk);
        store.append_output(&chunk);
        let char_count = store.current_output.chars().count();
        assert!(char_count > 0);
        assert!(store.current_output.len() <= MAX_OUTPUT_BYTES + TRUNCATION_MARKER.len());
    }
}
