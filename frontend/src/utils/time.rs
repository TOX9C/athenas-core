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
}
