//! Athena voice input: record the microphone (cpal) and transcribe it with
//! macOS on-device speech recognition (Apple Speech framework via the `speech`
//! crate). Desktop-only — the phone mirror has no mic-capture path.
//!
//! UX contract: `voice_record_start` begins a background capture session;
//! `voice_record_stop` ends it, writes a WAV to the temp dir, transcribes it
//! on-device (`requiresOnDeviceRecognition`), and returns the transcript text.
//! Nothing is uploaded: both the audio and the recognition run on the Mac.
//!
//! The frontend auto-stops at 30 s; the capture buffer hard-caps at 60 s so a
//! forgotten recording cannot grow without bound.
//!
//! Threading note: `cpal::Stream` is `!Send`, so it can never live in
//! `AppState` (Tauri managed state must be `Send + Sync`). Instead a dedicated
//! capture thread owns the stream; `VoiceRecording` holds only the channel
//! handles used to stop it and read the samples back.

use crate::state::AppState;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use tauri::State;

/// Hard cap on captured samples (60 s at 16 kHz mono). Preallocated so the
/// realtime audio callback never allocates per frame.
const MAX_CAPTURE_SAMPLES: usize = 60 * 16_000;
/// Below this peak amplitude the capture is treated as silence.
const SILENCE_PEAK: f32 = 0.005;

/// A live voice capture session. `Send + Sync` even though the cpal stream is
/// not: the stream lives on the capture thread, and this struct only carries
/// the handles to stop it (`stop_tx`), join it, and read its samples.
pub struct VoiceRecording {
    /// Signal the capture thread to drop the stream (end capture).
    stop_tx: std::sync::mpsc::Sender<()>,
    /// Joined on stop so the final sample is in before we read the buffer.
    join: Option<std::thread::JoinHandle<()>>,
    /// f32 samples written by the realtime audio callback.
    samples: Arc<parking_lot::Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

/// Pick an input config: prefer 16 kHz mono F32 (the standard for speech),
/// falling back to the first supported config otherwise.
fn pick_input_config(
    device: &cpal::Device,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), String> {
    let mut fallback: Option<(cpal::StreamConfig, cpal::SampleFormat)> = None;
    for supported in device
        .supported_input_configs()
        .map_err(|e| format!("failed to enumerate audio inputs: {e}"))?
    {
        let fmt = supported.sample_format();
        let rate_ok = supported.min_sample_rate() <= cpal::SampleRate(16_000)
            && supported.max_sample_rate() >= cpal::SampleRate(16_000);
        if fmt == cpal::SampleFormat::F32 && rate_ok {
            return Ok((
                supported
                    .with_sample_rate(cpal::SampleRate(16_000))
                    .config(),
                fmt,
            ));
        }
        if fallback.is_none() {
            fallback = Some((supported.with_max_sample_rate().config(), fmt));
        }
    }
    fallback.ok_or_else(|| "no usable audio input configuration".to_string())
}

/// Build (but do not play) the input stream for the chosen config, writing
/// normalized f32 samples into `samples`. Called on the capture thread.
fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    samples: Arc<parking_lot::Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String> {
    fn stream_err_fn(e: cpal::StreamError) {
        log::error!("voice capture stream error: {e}");
    }
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut buf = samples.lock();
                let room = MAX_CAPTURE_SAMPLES.saturating_sub(buf.len());
                let take = room.min(data.len());
                if take > 0 {
                    buf.extend_from_slice(&data[..take]);
                }
            },
            stream_err_fn,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let mut buf = samples.lock();
                for &v in data {
                    if buf.len() >= MAX_CAPTURE_SAMPLES {
                        break;
                    }
                    buf.push(v as f32 / 32768.0);
                }
            },
            stream_err_fn,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let mut buf = samples.lock();
                for &v in data {
                    if buf.len() >= MAX_CAPTURE_SAMPLES {
                        break;
                    }
                    buf.push((v as f32 / 32768.0) - 1.0);
                }
            },
            stream_err_fn,
        ),
    }
    .map_err(|e| format!("failed to start microphone capture: {e}"))?;
    Ok(stream)
}

/// Start capturing the microphone on a dedicated thread. Idempotent guard:
/// errors if a recording is already in progress. Capture continues until
/// `voice_record_stop`.
#[tauri::command]
pub async fn voice_record_start(state: State<'_, AppState>) -> Result<(), String> {
    // Fast idempotency check. The guard is dropped before the (potentially
    // slow) capture build so the mutex isn't held across the await below —
    // holding a parking_lot guard across `.await` would serialize concurrent
    // voice commands for the whole build and pin a runtime thread.
    {
        let slot = state.voice_recording.lock();
        if slot.is_some() {
            return Err("a voice recording is already in progress".to_string());
        }
    }

    let recording = tokio::task::spawn_blocking(build_recording)
        .await
        .map_err(|e| format!("voice capture task failed: {e}"))??;

    // Re-check under the lock: a concurrent start may have slipped in while
    // we were building. If so, tear down the recording we just built rather
    // than silently overwrite the winner.
    let mut slot = state.voice_recording.lock();
    if slot.is_some() {
        drop(slot);
        let _ = recording.stop_tx.send(());
        if let Some(join) = recording.join {
            let _ = join.join();
        }
        return Err("a voice recording is already in progress".to_string());
    }
    *slot = Some(recording);
    log::info!("voice recording started");
    Ok(())
}

/// Spawn the capture thread, build + play the stream, and report readiness
/// back through a oneshot. Runs on `spawn_blocking` because it blocks briefly
/// on that readiness handshake.
fn build_recording() -> Result<VoiceRecording, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no microphone found".to_string())?;
    let (config, format) = pick_input_config(&device)?;

    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let samples: Arc<parking_lot::Mutex<Vec<f32>>> = Arc::new(parking_lot::Mutex::new(
        Vec::with_capacity(MAX_CAPTURE_SAMPLES),
    ));
    let samples_for_thread = Arc::clone(&samples);
    let config_for_thread = config.clone();

    let join = std::thread::Builder::new()
        .name("athena-voice-capture".into())
        .spawn(move || {
            // The stream must be created AND dropped on this thread
            // (cpal::Stream is !Send). `stop_rx.recv()` below keeps it alive
            // until the stop command fires.
            let stream =
                match build_input_stream(&device, &config_for_thread, format, samples_for_thread) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(format!("failed to start microphone stream: {e}")));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            let _ = stop_rx.recv();
        })
        .map_err(|e| format!("failed to spawn capture thread: {e}"))?;

    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "timed out starting microphone capture".to_string())??;

    Ok(VoiceRecording {
        stop_tx,
        join: Some(join),
        samples,
        sample_rate: config.sample_rate.0,
        channels: config.channels,
    })
}

/// Stop the microphone capture, transcribe the clip on-device, and return the
/// transcript text. Errors if no recording is in progress or the clip is
/// silence / unrecognizable.
#[tauri::command]
pub async fn voice_record_stop(state: State<'_, AppState>) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let recording = state
            .voice_recording
            .lock()
            .take()
            .ok_or_else(|| "no voice recording in progress".to_string())?;
        let sample_rate = recording.sample_rate;
        let channels = recording.channels;
        // Signal stop and join the capture thread so the final sample is in
        // before we read the buffer. This may block briefly; run off the async
        // runtime.
        let samples = tokio::task::spawn_blocking(move || {
            let _ = recording.stop_tx.send(());
            if let Some(join) = recording.join {
                let _ = join.join();
            }
            recording.samples.lock().clone()
        })
        .await
        .map_err(|e| format!("voice capture task failed: {e}"))?;

        if samples.len() < sample_rate as usize / 5 {
            return Err("Recording too short — no speech detected".to_string());
        }
        let peak = samples.iter().fold(0.0f32, |acc, &s| acc.max(s.abs()));
        if peak < SILENCE_PEAK {
            return Err("No speech detected — the microphone may be muted".to_string());
        }

        // Trim leading/trailing silence in 50 ms blocks so the recognizer gets
        // a tight clip (and the temp file stays small).
        let block = (sample_rate as usize / 20).max(1); // 50 ms
        let threshold = 0.003f32;
        let mut start = 0;
        while start + block <= samples.len()
            && samples[start..start + block]
                .iter()
                .all(|&s| s.abs() < threshold)
        {
            start += block;
        }
        let mut end = samples.len();
        while end >= block
            && samples[end - block..end]
                .iter()
                .all(|&s| s.abs() < threshold)
        {
            end -= block;
        }
        let clipped = if start < end {
            &samples[start..end]
        } else {
            &samples[..0]
        };
        if clipped.is_empty() || clipped.len() < sample_rate as usize / 8 {
            return Err("No speech detected — the microphone may be muted".to_string());
        }

        // 16-bit PCM WAV for maximum recognizer compatibility.
        let path = std::env::temp_dir().join(format!("athena-voice-{}.wav", uuid::Uuid::new_v4()));
        let spec = hound::WavSpec {
            channels: channels.max(1),
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut writer = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
            for &v in clipped {
                let scaled = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
                writer.write_sample(scaled).map_err(|e| e.to_string())?;
            }
            writer.finalize().map_err(|e| e.to_string())?;
        }

        // Speech recognition is blocking (delegate-driven) — run it off the
        // async runtime so the command task stays responsive.
        let path_for_task = path.clone();
        let result = tokio::task::spawn_blocking(move || transcribe_wav(&path_for_task))
            .await
            .map_err(|e| format!("speech recognition task failed: {e}"))?;
        let _ = std::fs::remove_file(&path);
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        Err("Voice input is only available on macOS".to_string())
    }
}

/// Transcribe a WAV file with Apple's on-device speech recognizer, using the
/// auto-generated objc2 bindings for Speech.framework (pure Objective-C — no
/// Swift runtime needed). Blocking; run on a background thread.
/// `requiresOnDeviceRecognition` keeps the audio on the machine: no clip or
/// transcript leaves the Mac.
#[cfg(target_os = "macos")]
fn transcribe_wav(path: &std::path::Path) -> Result<String, String> {
    use block2::RcBlock;
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::AnyThread;
    use objc2_foundation::{NSBundle, NSError, NSString, NSURL};
    use objc2_speech::{
        SFSpeechRecognitionResult, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
        SFSpeechURLRecognitionRequest, SFTranscription,
    };

    autoreleasepool(|_pool| {
        // `requestAuthorization` requires NSSpeechRecognitionUsageDescription in
        // the app bundle's Info.plist and crashes without it. A bare `cargo run`
        // binary has no bundle — detect that and fail cleanly instead of
        // crashing the app in dev mode.
        let bundled = NSBundle::mainBundle().bundleIdentifier().is_some();
        if !bundled {
            return Err(
                "Speech recognition needs the bundled Athena.app — voice input is disabled in bare `cargo run` dev mode".to_string(),
            );
        }

        // 1) Authorization. First use prompts once; a denial is sticky until
        //    re-enabled in System Settings > Privacy & Security.
        let status = unsafe { SFSpeechRecognizer::authorizationStatus() };
        if status == SFSpeechRecognizerAuthorizationStatus::NotDetermined {
            let (tx, rx) = std::sync::mpsc::channel::<SFSpeechRecognizerAuthorizationStatus>();
            let block = RcBlock::new(move |s: SFSpeechRecognizerAuthorizationStatus| {
                let _ = tx.send(s);
            });
            // SAFETY: plain class-method call with a valid handler block.
            unsafe { SFSpeechRecognizer::requestAuthorization(&block) };
            let status = rx
                .recv_timeout(std::time::Duration::from_secs(30))
                .map_err(|_| "timed out waiting for speech-recognition permission".to_string())?;
            if status != SFSpeechRecognizerAuthorizationStatus::Authorized {
                return Err(format!(
                    "Speech recognition permission denied — enable it in System Settings > Privacy & Security > Speech Recognition ({status:?})"
                ));
            }
        }

        // 2) Recognizer for the user's default locale.
        // SAFETY: init with a fresh allocation; returns None on unsupported locale.
        let recognizer = unsafe { SFSpeechRecognizer::init(SFSpeechRecognizer::alloc()) }
            .ok_or_else(|| {
                "failed to create a speech recognizer for the current locale".to_string()
            })?;
        // SAFETY: plain property getters.
        if !unsafe { recognizer.supportsOnDeviceRecognition() } {
            return Err(
                "On-device speech recognition is unavailable for the current locale — install the language model in System Settings > Privacy & Security > Speech Recognition"
                    .to_string(),
            );
        }
        if !unsafe { recognizer.isAvailable() } {
            return Err(
                "Speech recognizer is unavailable right now — try again in a moment".to_string(),
            );
        }

        // 3) File-based request, forced on-device with punctuation.
        let path_str = NSString::from_str(path.to_string_lossy().as_ref());
        let url = NSURL::fileURLWithPath(&path_str);
        let request = unsafe {
            SFSpeechURLRecognitionRequest::initWithURL(SFSpeechURLRecognitionRequest::alloc(), &url)
        };
        // SAFETY: plain property setters on a live request object.
        unsafe {
            request.setRequiresOnDeviceRecognition(true);
            request.setShouldReportPartialResults(false);
            request.setAddsPunctuation(true);
        }

        // 4) Run the task and wait for the final result. The recognizer, task,
        //    and handler all stay alive in this scope until the channel fires.
        let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
        // The result handler receives raw pointers (nullable); the framework
        // owns them for the duration of the callback, so borrow, never retain.
        let handler = RcBlock::new(
            move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
                if !result.is_null() {
                    // SAFETY: non-null result pointer, valid for the callback.
                    let result = unsafe { &*result };
                    // SAFETY: plain property getters on the delivered result.
                    if unsafe { result.isFinal() } {
                        // SAFETY: plain property getters.
                        let transcript: Retained<SFTranscription> =
                            unsafe { result.bestTranscription() };
                        let text = unsafe { transcript.formattedString() }.to_string();
                        let _ = tx.send(Ok(text));
                        return;
                    }
                }
                if !error.is_null() {
                    // SAFETY: non-null error pointer, valid for the callback.
                    let error = unsafe { &*error };
                    let desc = error.localizedDescription().to_string();
                    let _ = tx.send(Err(desc));
                }
            },
        );
        // SAFETY: valid request + handler block; the returned task is retained
        // for the duration of the call.
        unsafe {
            let _task = recognizer.recognitionTaskWithRequest_resultHandler(&request, &handler);
        }

        let text = rx
            .recv_timeout(std::time::Duration::from_secs(60))
            .map_err(|_| "speech recognition timed out".to_string())??;
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err("No speech recognized — try again closer to the microphone".to_string());
        }
        Ok(text)
    })
}
