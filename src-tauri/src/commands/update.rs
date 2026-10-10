//! In-app update check against the project's GitHub releases.
//!
//! The app is distributed unsigned as DMG + Homebrew cask — there is no
//! notarized binary and no update server, so `tauri-plugin-updater` is not
//! usable. Instead: fetch the latest release tag from the GitHub API, compare
//! it with the running version, and let the UI offer a link to the release
//! page. The check runs in Rust because the webview CSP pins `connect-src` to
//! `'self' ipc:` (enforced by `check:tauri-security`), so no renderer fetch to
//! api.github.com is possible.
//!
//! The release URL is a compile-time constant — the renderer never supplies
//! a URL to open, so a compromised webview cannot turn `update_open_release`
//! into an open-redirect.

use tauri_plugin_shell::ShellExt;

use super::CommandError;

const RELEASE_API: &str = "https://api.github.com/repos/TOX9C/athenas-core/releases/latest";
const RELEASE_PAGE: &str = "https://github.com/TOX9C/athenas-core/releases/latest";
const REQUEST_TIMEOUT_SECS: u64 = 10;

/// Parse a `vX.Y.Z` release tag into a comparable tuple. Returns `None` for
/// anything that isn't exactly three dot-separated numbers after the leading
/// `v` (prereleases like `v3.5.0-rc1` are treated as "no info" — the release
/// workflow only ever publishes plain `vX.Y.Z` tags, enforced by
/// `check:release-identity`).
fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let core = tag.strip_prefix('v')?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// True only when `latest` is strictly newer than `current` (tuple ordering,
/// never string comparison — "3.10.0" < "3.9.0" as strings).
fn is_newer(latest: (u64, u64, u64), current: (u64, u64, u64)) -> bool {
    latest > current
}

/// Check the latest published release and compare it with the running app
/// version. Any network/parse failure surfaces as a `CommandError` — the
/// caller decides whether that's worth showing (the startup auto-check
/// stays silent on failure; the About button shows a message).
#[tauri::command]
pub async fn update_check() -> Result<String, CommandError> {
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| CommandError::Internal(format!("HTTP client error: {e}")))?
        .get(RELEASE_API)
        // GitHub rejects requests without a User-Agent with 403.
        .header("User-Agent", "athenas-core-update-check")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            CommandError::Internal(format!("Could not reach GitHub (are you offline?): {e}"))
        })?;
    let resp = resp.error_for_status().map_err(|e| {
        // 403 here is almost always GitHub's unauthenticated rate limit.
        CommandError::Internal(format!("GitHub request failed: {e}"))
    })?;
    let body = resp
        .text()
        .await
        .map_err(|e| CommandError::Internal(format!("GitHub response unreadable: {e}")))?;
    let release: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| CommandError::Internal(format!("GitHub response invalid: {e}")))?;
    let tag = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CommandError::Internal("GitHub response missing tag_name".to_string()))?;

    let latest = parse_version(tag)
        .ok_or_else(|| CommandError::Internal(format!("Unrecognized release tag: {tag}")))?;

    let current = parse_version(&format!("v{}", env!("CARGO_PKG_VERSION")))
        .expect("CARGO_PKG_VERSION is maintained as plain X.Y.Z by check:release-identity");

    serde_json::to_string(&serde_json::json!({
        "current": env!("CARGO_PKG_VERSION"),
        "latest": tag,
        "update_available": is_newer(latest, current),
    }))
    .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Open the latest-release page in the user's default browser. Takes no
/// arguments: the URL is a constant, so the renderer cannot use this to open
/// arbitrary URLs. Uses the already-registered shell plugin's Rust API — no
/// `shell:allow-open` capability is granted to the webview.
#[tauri::command]
pub fn update_open_release(app: tauri::AppHandle) -> Result<(), CommandError> {
    #[allow(deprecated)] // tauri-plugin-shell open(); opener plugin would be a new dep
    app.shell()
        .open(RELEASE_PAGE, None)
        .map_err(|e| CommandError::Internal(format!("Could not open the release page: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_tags() {
        assert_eq!(parse_version("v3.4.0"), Some((3, 4, 0)));
        assert_eq!(parse_version("v10.20.30"), Some((10, 20, 30)));
    }

    #[test]
    fn rejects_junk_tags() {
        assert_eq!(parse_version("3.4.0"), None, "missing v prefix");
        assert_eq!(parse_version("v3.4"), None, "two components");
        assert_eq!(parse_version("v3.4.0.1"), None, "four components");
        assert_eq!(parse_version("v3.4.0-rc1"), None, "prerelease suffix");
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn compares_numerically_not_lexically() {
        // The classic trap: "3.10.0" < "3.9.0" as strings.
        assert!(is_newer((3, 10, 0), (3, 9, 0)));
        assert!(is_newer((3, 5, 0), (3, 4, 9)));
        assert!(!is_newer((3, 4, 0), (3, 4, 0)));
        assert!(!is_newer((3, 2, 0), (3, 4, 0)));
    }
}
