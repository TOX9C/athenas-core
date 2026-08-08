//! Bounded terminal-block output buffering.

/// Hard cap on accumulated output for a single block (current_output + block.output).
/// Prevents unbounded heap growth on long-running commands like `yes` or `tail -f`.
pub(super) const MAX_OUTPUT_BYTES: usize = 256 * 1024; // 256 KiB
pub(super) const TRUNCATION_MARKER: &str = "\n[...truncated at 256 KiB...]";

/// Append `incoming` to `buf`, capping total length at `MAX_OUTPUT_BYTES`.
/// If the cap is reached, drop the oldest bytes and insert a single
/// truncation marker (only on the first truncation). New data that arrives
/// after the cap is still appended (up to the cap) so the most recent output
/// is always visible.
pub(super) fn append_capped(buf: &mut String, incoming: &str) {
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
