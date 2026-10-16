//! Open a URL in the user's default system browser.
//!
//! Used by the link chooser ("open in App Browser / External Browser") that
//! a terminal link click now triggers. The generic `update_open_release`
//! command cannot be reused here: it pins a compile-time constant URL so a
//! compromised webview cannot turn it into an open-redirect. Terminal links
//! are renderer-supplied, so this command must validate instead of pin.
//!
//! Validation reuses `athena_browser::normalize_url` — the same function the
//! in-app browser panels use — so the two destinations accept exactly the
//! same set of URLs. That rejects `javascript:`, `data:`, and other non-HTTP
//! schemes, plus control characters, which is what keeps this from being an
//! open-redirect or a script-execution primitive.

use tauri_plugin_shell::ShellExt;

use super::CommandError;

/// Open `url` in the OS default browser.
///
/// Rejects anything that is not an `http`/`https` URL (after normalization,
/// so bare hosts like `github.com` are accepted and get a scheme) rather
/// than silently opening a malformed target. The failure is reported to the
/// caller so the UI can surface it instead of discarding the click.
#[tauri::command]
pub async fn open_external(url: String, app: tauri::AppHandle) -> Result<(), CommandError> {
    // Same rules the embedded browser enforces; a scheme-shaped entry the
    // embedded browser would refuse must not be handed to the OS.
    let normalized = athena_browser::normalize_url(&url)
        .map_err(|e| CommandError::InvalidInput(format!("Refusing to open link: {e}")))?;

    #[allow(deprecated)] // tauri-plugin-shell open(); the opener plugin would be a new dep
    app.shell()
        .open(normalized, None)
        .map_err(|e| CommandError::Internal(format!("Could not open the system browser: {e}")))
}

#[cfg(test)]
mod tests {
    // The validate-then-open boundary is the security-relevant part of this
    // command, so it is exercised against the same helper the command calls.

    #[test]
    fn accepts_http_and_https_and_bare_hosts() {
        assert!(athena_browser::normalize_url("https://github.com").is_ok());
        assert!(athena_browser::normalize_url("http://localhost:3000").is_ok());
        assert!(athena_browser::normalize_url("github.com").is_ok());
    }

    #[test]
    fn rejects_non_http_schemes() {
        for scheme in [
            "javascript:alert(1)",
            "data:text/html,<b>x",
            "file:///etc/passwd",
        ] {
            assert!(
                athena_browser::normalize_url(scheme).is_err(),
                "expected rejection: {scheme}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_control_characters() {
        assert!(athena_browser::normalize_url("").is_err());
        assert!(athena_browser::normalize_url("   ").is_err());
        assert!(athena_browser::normalize_url("https://a.com/\u{7}").is_err());
    }

    #[test]
    fn normalizes_a_bare_host_to_https() {
        assert_eq!(
            athena_browser::normalize_url("github.com").unwrap(),
            "https://github.com"
        );
        // Local hosts are the documented http exception.
        assert_eq!(
            athena_browser::normalize_url("localhost:3000").unwrap(),
            "http://localhost:3000"
        );
    }
}
