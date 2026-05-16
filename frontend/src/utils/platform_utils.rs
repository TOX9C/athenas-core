//! Platform utilities -- ported from src/utils/platformUtils.ts
//!
//! Provides platform detection and platform-specific constants.
//! Uses runtime detection via navigator.userAgent because cfg!(target_os)
//! is resolved at compile time and is always the host OS when cross-compiling
//! to WASM, not the browser's actual platform.

/// Check if the current platform is macOS by inspecting the user agent.
pub fn is_mac() -> bool {
    web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .map(|ua| ua.contains("Mac"))
        .unwrap_or(false)
}

/// Get the default shell path for the current platform.
/// Note: in a WASM context this is only a hint; the actual shell is
/// determined by the Tauri backend via `pty_default_shell`.
pub fn get_default_shell() -> &'static str {
    if is_mac() {
        "/bin/zsh"
    } else if web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .map(|ua| ua.contains("Windows"))
        .unwrap_or(false)
    {
        "cmd.exe"
    } else {
        "/bin/bash"
    }
}

/// Get the modifier key symbol for the current platform.
pub fn mod_key() -> &'static str {
    if is_mac() {
        "\u{2318}"
    } else {
        "Ctrl"
    }
}
