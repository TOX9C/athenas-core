//! Read-only macOS TCC status for the permissions Athena actually uses.
//!
//! There is deliberately no Accessibility/Screen Recording row: no code path
//! uses those APIs, and listing them would repeat the "check System Settings"
//! misdirection this module exists to end. Requests still happen lazily on
//! first use (voice paths); this module only *reports*.

use super::CommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    Unknown,
    /// Platform/feature has no such permission concept (non-macOS build or
    /// `voice` feature off). Constructed only under those cfgs; the variant
    /// is part of the serialized frontend contract, hence the allow.
    #[allow(dead_code)]
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PermissionRow {
    pub id: String,
    pub status: PermissionStatus,
}

/// Map an `AVAuthorizationStatus` (typed, from AVFoundation).
///
/// Case order per the generated bindings: NotDetermined=0 Restricted=1
/// Denied=2 Authorized=3 — this DIFFERS from UserNotifications, so each
/// framework gets its own typed mapper; never share raw positional values.
/// `restricted` maps to Denied for UI purposes (parental controls — the
/// user cannot grant it in Settings anyway).
#[cfg(target_os = "macos")]
fn mic_status_from_av(raw: objc2_av_foundation::AVAuthorizationStatus) -> PermissionStatus {
    use objc2_av_foundation::AVAuthorizationStatus as AV;
    match raw {
        AV::Authorized => PermissionStatus::Granted,
        AV::Denied | AV::Restricted => PermissionStatus::Denied,
        AV::NotDetermined => PermissionStatus::NotDetermined,
        _ => PermissionStatus::Unknown, // out-of-range raw value
    }
}

/// Map a `UNAuthorizationStatus` (typed, from UserNotifications).
///
/// Case order: NotDetermined=0 Denied=1 Authorized=2 Provisional=3
/// Ephemeral=4. Provisional/Ephemeral count as Granted (the OS delivers
/// notifications under those states).
#[cfg(target_os = "macos")]
fn notif_status_from_un(raw: objc2_user_notifications::UNAuthorizationStatus) -> PermissionStatus {
    use objc2_user_notifications::UNAuthorizationStatus as UN;
    match raw {
        UN::Authorized | UN::Provisional | UN::Ephemeral => PermissionStatus::Granted,
        UN::Denied => PermissionStatus::Denied,
        UN::NotDetermined => PermissionStatus::NotDetermined,
        _ => PermissionStatus::Unknown, // out-of-range raw value
    }
}

/// Read-only snapshot of the OS-level permissions the app actually uses.
#[tauri::command]
pub async fn os_permissions_status() -> Result<Vec<PermissionRow>, CommandError> {
    #[cfg(target_os = "macos")]
    {
        let mut rows = Vec::new();

        // Microphone: AVFoundation is the authoritative TCC query (the cpal
        // capture path prompts lazily, so this is often notDetermined until
        // the user records once).
        let mic = tokio::task::spawn_blocking(|| {
            #[cfg(feature = "voice")]
            {
                use objc2_av_foundation::AVCaptureDevice;
                use objc2_foundation::NSBundle;
                // authorizationStatusForMediaType requires a bundle for stable
                // TCC identity; a bare `cargo run` gets Unknown, not a crash.
                if NSBundle::mainBundle().bundleIdentifier().is_none() {
                    return PermissionStatus::Unknown;
                }
                // SAFETY: class method; the media-type global is a constant
                // published by the framework (non-nil on every real macOS).
                let media_type = unsafe { objc2_av_foundation::AVMediaTypeAudio };
                match media_type {
                    Some(mt) => {
                        let status = unsafe {
                            AVCaptureDevice::authorizationStatusForMediaType(mt)
                        };
                        mic_status_from_av(status)
                    }
                    None => PermissionStatus::Unknown,
                }
            }
            #[cfg(not(feature = "voice"))]
            {
                PermissionStatus::NotApplicable
            }
        })
        .await
        .map_err(|e| CommandError::Internal(format!("mic status task: {e}")))?;
        rows.push(PermissionRow {
            id: "microphone".into(),
            status: mic,
        });

        // Speech recognition: SFSpeechRecognizer is already in the tree via
        // the `voice` feature — same class the transcription path checks.
        let speech = tokio::task::spawn_blocking(|| {
            #[cfg(feature = "voice")]
            {
                use objc2_foundation::NSBundle;
                if NSBundle::mainBundle().bundleIdentifier().is_none() {
                    return PermissionStatus::Unknown;
                }
                // SAFETY: plain class-method call with no arguments.
                let status = unsafe { objc2_speech::SFSpeechRecognizer::authorizationStatus() };
                match status {
                    objc2_speech::SFSpeechRecognizerAuthorizationStatus::Authorized => {
                        PermissionStatus::Granted
                    }
                    objc2_speech::SFSpeechRecognizerAuthorizationStatus::Denied => {
                        PermissionStatus::Denied
                    }
                    objc2_speech::SFSpeechRecognizerAuthorizationStatus::NotDetermined => {
                        PermissionStatus::NotDetermined
                    }
                    _ => PermissionStatus::Denied, // restricted
                }
            }
            #[cfg(not(feature = "voice"))]
            {
                PermissionStatus::NotApplicable
            }
        })
        .await
        .map_err(|e| CommandError::Internal(format!("speech status task: {e}")))?;
        rows.push(PermissionRow {
            id: "speech_recognition".into(),
            status: speech,
        });

        // Notifications: UNUserNotificationCenter settings callback →
        // status, converted to a blocking recv like voice.rs's pattern.
        let notif = tokio::task::spawn_blocking(|| {
            #[cfg(feature = "voice")]
            {
                use block2::RcBlock;
                use objc2_foundation::NSBundle;
                if NSBundle::mainBundle().bundleIdentifier().is_none() {
                    return PermissionStatus::Unknown;
                }
                use objc2_user_notifications::UNUserNotificationCenter;
                let center = UNUserNotificationCenter::currentNotificationCenter();
                let (tx, rx) = std::sync::mpsc::channel::<std::ptr::NonNull<objc2_user_notifications::UNNotificationSettings>>();
                let block = RcBlock::new(
                    move |settings: std::ptr::NonNull<objc2_user_notifications::UNNotificationSettings>| {
                        let _ = tx.send(settings);
                    },
                );
                // The UNUserNotificationCenter method is safe in objc2 0.3
                // bindings (block lifetime managed by RcBlock here).
                center.getNotificationSettingsWithCompletionHandler(&block);
                match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                    Ok(ptr) => {
                        // SAFETY: the framework hands us a valid settings
                        // object for the duration of the callback.
                        let settings = unsafe { ptr.as_ref() };
                        notif_status_from_un(settings.authorizationStatus())
                    }
                    _ => PermissionStatus::Unknown,
                }
            }
            #[cfg(not(feature = "voice"))]
            {
                PermissionStatus::NotApplicable
            }
        })
        .await
        .map_err(|e| CommandError::Internal(format!("notification status task: {e}")))?;
        rows.push(PermissionRow {
            id: "notifications".into(),
            status: notif,
        });

        Ok(rows)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(vec![
            PermissionRow {
                id: "microphone".into(),
                status: PermissionStatus::NotApplicable,
            },
            PermissionRow {
                id: "speech_recognition".into(),
                status: PermissionStatus::NotApplicable,
            },
            PermissionRow {
                id: "notifications".into(),
                status: PermissionStatus::NotApplicable,
            },
        ])
    }
}

/// Open the System Settings pane for a permission. The URL is chosen
/// server-side from a fixed allowlist — the renderer cannot open arbitrary
/// URLs through this command.
#[tauri::command]
pub async fn os_permissions_open_settings(panel: String) -> Result<(), CommandError> {
    let url = match panel.as_str() {
        "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "speech_recognition" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition"
        }
        "notifications" => {
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
        }
        _ => {
            return Err(CommandError::InvalidInput(format!(
                "unknown permission panel: {panel}"
            )))
        }
    };
    let opened = std::process::Command::new("open")
        .arg(url)
        .status()
        .map(|s| s.success())
        .map_err(|e| CommandError::Internal(format!("failed to open System Settings: {e}")))?;
    if !opened {
        return Err(CommandError::Internal(
            "System Settings failed to open".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn av_mapper_covers_all_cases() {
        use objc2_av_foundation::AVAuthorizationStatus as AV;
        assert_eq!(mic_status_from_av(AV::NotDetermined), PermissionStatus::NotDetermined);
        assert_eq!(mic_status_from_av(AV::Restricted), PermissionStatus::Denied);
        assert_eq!(mic_status_from_av(AV::Denied), PermissionStatus::Denied);
        assert_eq!(mic_status_from_av(AV::Authorized), PermissionStatus::Granted);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn un_mapper_covers_all_cases() {
        use objc2_user_notifications::UNAuthorizationStatus as UN;
        assert_eq!(notif_status_from_un(UN::NotDetermined), PermissionStatus::NotDetermined);
        assert_eq!(notif_status_from_un(UN::Denied), PermissionStatus::Denied);
        assert_eq!(notif_status_from_un(UN::Authorized), PermissionStatus::Granted);
        assert_eq!(notif_status_from_un(UN::Provisional), PermissionStatus::Granted);
        assert_eq!(notif_status_from_un(UN::Ephemeral), PermissionStatus::Granted);
    }

    #[test]
    fn row_ids_are_stable_strings() {
        let row = PermissionRow {
            id: "microphone".into(),
            status: PermissionStatus::Granted,
        };
        let json = serde_json::to_string(&row).unwrap();
        assert!(json.contains(r#""id":"microphone""#));
        assert!(json.contains(r#""status":"granted""#));
    }
}
