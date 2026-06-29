//! Notification sound utility.
//!
//! Plays a short sine-wave "ding" via the Web Audio API. Implemented with
//! typed `web_sys` bindings (not `wasm_bindgen::inline_js`) so it does not
//! require `'unsafe-eval'` in the Content-Security-Policy.
//!
//! A single `AudioContext` is created lazily and reused across calls:
//! browsers cap the number of live `AudioContext`s (~6), so creating one per
//! notification would mute the app after a handful of dings.

#[cfg(target_arch = "wasm32")]
pub fn play_ding() {
    use std::sync::OnceLock;
    use web_sys::{AudioContext, OscillatorType};

    // Reuse a single AudioContext for the lifetime of the app. The first
    // call constructs it; subsequent calls reuse it. This avoids the ~6
    // AudioContext browser cap.
    static CTX: OnceLock<Option<AudioContext>> = OnceLock::new();
    let ctx = match CTX.get_or_init(|| AudioContext::new().ok()) {
        Some(c) => c,
        None => {
            log::warn!("Notification sound: AudioContext unavailable");
            return;
        }
    };

    // Browsers suspend AudioContexts created before a user gesture; resume
    // is a no-op if already running. Ignore errors (e.g. autoplay policy).
    let _ = ctx.resume();

    let result = (|| -> Result<(), wasm_bindgen::JsValue> {
        let osc = ctx.create_oscillator()?;
        let gain = ctx.create_gain()?;

        osc.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&ctx.destination())?;

        osc.frequency().set_value(880.0);
        osc.set_type(OscillatorType::Sine);

        let now = ctx.current_time();
        let gain_param = gain.gain();
        gain_param.set_value_at_time(0.3, now)?;
        gain_param.exponential_ramp_to_value_at_time(0.0001, now + 0.5)?;

        osc.start()?;
        osc.stop_with_when(now + 0.5)?;
        // OscillatorNode auto-disconnects after `stop()` fires; no manual
        // cleanup needed. Dropping the JS handles here is fine — the nodes
        // are kept alive by the audio graph until they finish.
        Ok(())
    })();

    if let Err(e) = result {
        log::debug!("Notification sound failed: {:?}", e);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn play_ding() {
    log::debug!("Notification sound played (native placeholder)");
}
