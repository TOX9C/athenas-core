//! Strip-ANSI utility — ported from src/utils/commandParser.ts (stripAnsi)
//!
//! Comprehensive ANSI escape code remover with cursor-forward space
//! preservation and post-cleanup normalization.

use once_cell::sync::Lazy;
use regex::Regex;

/// Cursor Forward: `\x1b[<n>C` — replace with `n` spaces to preserve layout.
static CURSOR_FORWARD_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[(\d+)C").unwrap());

/// Composite ANSI-removal pattern covering OSC, CSI, ESC, SS2/SS3,
/// backspace-overlay, and stray control characters.
static ANSI_STRIP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?x)
        \x1b\][^\x07\x1b]*(?:\x07|\x1b\\)  # OSC sequences
        | \x1b\[[0-9;]*[a-zA-Z]            # CSI sequences
        | \x1b[^\[\]]                      # Simple ESC sequences
        | \x1bN|\x1bO                       # SS2 / SS3
        | \x08\x1b\[K                       # Backspace overlay
        | [\x00-\x08\x0b\x0c\x0e-\x1a]     # Stray control chars
        "#,
    )
    .unwrap()
});

/// Post-cleanup: collapse multiple newlines.
static NEWLINE_COLLAPSE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

/// Strip all ANSI escape codes from a string, preserving cursor-forward spacing.
pub fn strip_ansi(input: &str) -> String {
    // Step 1: replace cursor-forward with spaces
    let with_spaces = CURSOR_FORWARD_RE.replace_all(input, |caps: &regex::Captures| {
        let n: usize = caps[1].parse().unwrap_or(1).min(4096);
        " ".repeat(n)
    });

    // Step 2: remove all other ANSI/control sequences
    let stripped = ANSI_STRIP_RE.replace_all(&with_spaces, "");

    // Step 3: normalize line endings
    let normalized = stripped.replace("\r\n", "\n").replace('\r', "");

    // Step 4: remove stray backspaces
    let no_bs: String = normalized.chars().filter(|c| *c != '\x08').collect();

    // Step 5: collapse 3+ newlines to 2
    NEWLINE_COLLAPSE_RE.replace_all(&no_bs, "\n\n").to_string()
}
