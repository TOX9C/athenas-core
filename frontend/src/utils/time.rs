//! Shared wall-clock time helpers.
//!
//! WASM-safe: uses `js_sys::Date::now()` on `wasm32` and falls back to
//! `std::time::SystemTime` on native targets so the helpers are testable in
//! `cargo test` without panicking.

/// Current wall-clock time in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Format a Unix-millisecond timestamp relative to `now_ms` as a short
/// "… ago" string. THE relative-time renderer for the frontend — session
/// lists and agent status bars both use this so wording never drifts
/// between panes.
///
/// Negative deltas (clock skew) render as "just now".
pub fn format_time_ago_at(timestamp_ms: i64, now_ms: i64) -> String {
    let secs = (now_ms.saturating_sub(timestamp_ms)).max(0) / 1000;
    if secs < 10 {
        "just now".to_string()
    } else if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Format a Unix-millisecond timestamp relative to the current time.
pub fn format_time_ago(timestamp_ms: i64) -> String {
    format_time_ago_at(timestamp_ms, now_ms() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_ms_returns_recent_epoch_time() {
        // Must be non-zero (epoch is long past) and roughly monotonic across
        // two immediate calls (allowing the clock to tick forward).
        let a = now_ms();
        assert!(a > 1_000_000, "unexpectedly small now_ms: {a}");
        let b = now_ms();
        assert!(b >= a, "clock went backwards: {a} -> {b}");
    }

    #[test]
    fn format_time_ago_buckets_and_skew() {
        let now: i64 = 10 * 86_400_000;
        assert_eq!(format_time_ago_at(now - 30_000, now), "30s ago");
        assert_eq!(format_time_ago_at(now - 2 * 60_000, now), "2m ago");
        assert_eq!(format_time_ago_at(now - 3 * 3_600_000, now), "3h ago");
        assert_eq!(format_time_ago_at(now - 4 * 86_400_000, now), "4d ago");
        // Future / clock-skew timestamps clamp to "just now".
        assert_eq!(format_time_ago_at(now + 1_000, now), "just now");
        assert_eq!(format_time_ago_at(now - 5_000, now), "just now");
    }
}
