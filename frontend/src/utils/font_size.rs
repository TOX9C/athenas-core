//! Shared font-size bounds and adjustment logic.
//!
//! The setting is used by the desktop terminal and monospace/code surfaces.
//! Keeping the bounds here ensures keyboard shortcuts, Settings controls, and
//! persisted values all follow the same rules.

use std::cell::RefCell;

pub const MIN_FONT_SIZE: u8 = 10;
pub const MAX_FONT_SIZE: u8 = 24;

/// Normalize a persisted or externally supplied font size to the supported range.
pub fn clamp_font_size(size: u8) -> u8 {
    size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

/// Parse and normalize a persisted font-size value.
pub fn parse_persisted_font_size(value: &str) -> Option<u8> {
    value
        .trim()
        .parse::<i32>()
        .ok()
        .map(|size| size.clamp(MIN_FONT_SIZE as i32, MAX_FONT_SIZE as i32) as u8)
}

/// Move the font size by a signed pixel delta while respecting the supported range.
pub fn adjust_font_size(size: u8, delta: i8) -> u8 {
    let current = clamp_font_size(size) as i16;
    (current + delta as i16).clamp(MIN_FONT_SIZE as i16, MAX_FONT_SIZE as i16) as u8
}

struct PendingFontSizeWrite {
    pending: Option<u8>,
    running: bool,
}

thread_local! {
    static FONT_SIZE_WRITE: RefCell<PendingFontSizeWrite> = const {
        RefCell::new(PendingFontSizeWrite {
            pending: None,
            running: false,
        })
    };
}

/// Persist only the latest font-size value while serializing writes.
///
/// Keyboard repeats can generate several changes before IPC completes. A
/// short debounce collapses those changes, and the single worker prevents an
/// older store write from completing after a newer value.
pub fn persist_font_size(size: u8) {
    let should_start = FONT_SIZE_WRITE.with(|state| {
        let mut state = state.borrow_mut();
        state.pending = Some(clamp_font_size(size));
        if state.running {
            false
        } else {
            state.running = true;
            true
        }
    });

    if should_start {
        wasm_bindgen_futures::spawn_local(async {
            loop {
                gloo::timers::future::TimeoutFuture::new(50).await;
                let next = FONT_SIZE_WRITE.with(|state| state.borrow_mut().pending.take());
                let Some(next) = next else {
                    FONT_SIZE_WRITE.with(|state| state.borrow_mut().running = false);
                    return;
                };
                let _ = crate::tauri_bridge::store_set("font_size", &next.to_string()).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_values_to_supported_range() {
        assert_eq!(clamp_font_size(0), MIN_FONT_SIZE);
        assert_eq!(clamp_font_size(14), 14);
        assert_eq!(clamp_font_size(u8::MAX), MAX_FONT_SIZE);
    }

    #[test]
    fn parses_and_normalizes_persisted_values() {
        assert_eq!(parse_persisted_font_size("14"), Some(14));
        assert_eq!(parse_persisted_font_size("-1"), Some(MIN_FONT_SIZE));
        assert_eq!(parse_persisted_font_size("999"), Some(MAX_FONT_SIZE));
        assert_eq!(parse_persisted_font_size("not-a-size"), None);
    }

    #[test]
    fn increments_and_decrements_by_one() {
        assert_eq!(adjust_font_size(14, 1), 15);
        assert_eq!(adjust_font_size(14, -1), 13);
    }

    #[test]
    fn adjustment_stops_at_both_bounds() {
        assert_eq!(adjust_font_size(MIN_FONT_SIZE, -1), MIN_FONT_SIZE);
        assert_eq!(adjust_font_size(MAX_FONT_SIZE, 1), MAX_FONT_SIZE);
    }

    #[test]
    fn adjustment_normalizes_invalid_current_values_before_stepping() {
        assert_eq!(adjust_font_size(0, 1), MIN_FONT_SIZE + 1);
        assert_eq!(adjust_font_size(u8::MAX, -1), MAX_FONT_SIZE - 1);
    }
}
