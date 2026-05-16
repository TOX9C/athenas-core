//! Notification sound utility — ported from src/utils/notificationSound.ts
//!
//! In the browser-based TS version, this uses the Web Audio API to play
//! a short sine-wave "ding". In the Tauri/Rust context, we use the
//! Tauri notification system or a no-op placeholder for non-WASM targets.

/// Play a notification ding sound.
///
/// In a Tauri desktop build, this can be wired to a system notification
/// or a bundled audio file. In the WASM build, this calls into the
/// browser's Web Audio API via wasm-bindgen.
#[cfg(target_arch = "wasm32")]
pub fn play_ding() {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = r#"
        export function play_ding_js() {
            try {
                var ctx = new (window.AudioContext || window.webkitAudioContext)();
                var osc = ctx.createOscillator();
                var gain = ctx.createGain();
                osc.connect(gain);
                gain.connect(ctx.destination);
                osc.frequency.value = 880;
                osc.type = 'sine';
                gain.gain.setValueAtTime(0.3, ctx.currentTime);
                gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.5);
                osc.start(ctx.currentTime);
                osc.stop(ctx.currentTime + 0.5);
            } catch(e) {}
        }
    "#)]
    extern "C" {
        fn play_ding_js();
    }

    play_ding_js();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn play_ding() {
    log::debug!("Notification sound played (native placeholder)");
}
