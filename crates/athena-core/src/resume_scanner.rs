//! Robust scanner for agent resume IDs in PTY output streams.
//!
//! This is the Rust twin of `frontend/src/utils/resume_scanner.rs`. Claude
//! Code (and similar tools) print a line like `claude --resume <id>` on
//! clean exit. The line arrives as raw PTY bytes split into arbitrary
//! chunks, so a naïve "scan the last N bytes of each individual chunk"
//! approach misses matches that span chunk boundaries or sit earlier than
//! the scanned window.
//!
//! `ResumeScanner` keeps a small rolling text buffer per pane. Each new
//! chunk is ANSI-stripped (so color/bold codes interleaved between the
//! words `claude`, `--resume`, and the id do not break the match),
//! appended, the *entire* buffer is searched, then the buffer is trimmed
//! to a bounded size. A match is only reported once per distinct id, so a
//! long-running session does not re-write the store on every subsequent
//! PTY chunk.
//!
//! The stateless [`scan_text_for_resume_id`] helper scans a single
//! already-accumulated string (e.g. an `OutputBuffer` snapshot) and is used
//! by the backend's app-exit path where there is no per-chunk state to
//! maintain.

const MAX_SCAN_BUFFER: usize = 1024;
const MAX_ID_LEN: usize = 256;

/// Known CLI resume prefixes, in order of preference. The trailing space
/// is significant for matching but is stripped from the returned prefix.
const PREFIXES: &[&str] = &[
    "claude --resume ",
    "codex --resume ",
    "opencode --resume ",
    "gemini --resume ",
];

/// A stateful scanner for a single pane. Create one, feed it text via
/// [`ResumeScanner::feed`], and obtain newly detected resume ids. Each
/// distinct id is returned at most once (deduplicated across feeds).
pub struct ResumeScanner {
    /// Accumulated ANSI-stripped tail of the last feed(s); never grows
    /// beyond `MAX_SCAN_BUFFER`.
    buf: String,
    /// The last id `feed` returned. Used to suppress duplicate reports:
    /// the buffer still contains the resume line after a match, so without
    /// this every subsequent chunk would re-report the same id (and the
    /// caller would re-write the store + re-trigger a disk save on every
    /// PTY write for the rest of the session).
    last_matched: Option<String>,
}

impl Default for ResumeScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ResumeScanner {
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(MAX_SCAN_BUFFER),
            last_matched: None,
        }
    }

    /// Append new text and scan the entire accumulated buffer for a resume
    /// pattern. When a *new* match is found (an id we have not already
    /// reported this scanner), returns `(prefix, id)` where `prefix` is the
    /// matched CLI prefix without the trailing space (e.g. `"claude --resume"`)
    /// and `id` is the resume session identifier. `None` means no new match.
    ///
    /// Incoming text is ANSI-stripped first; color/bold codes interleaved
    /// between the words do not break matching.
    pub fn feed(&mut self, text: &str) -> Option<(String, String)> {
        let stripped = strip_ansi(text);
        self.buf.push_str(&stripped);

        // Trim oldest bytes to keep the buffer bounded.
        if self.buf.len() > MAX_SCAN_BUFFER {
            let cut = self.buf.len() - MAX_SCAN_BUFFER;
            // Ensure we don't cut in the middle of a UTF-8 char boundary.
            // Walk forward from `cut` to the next char boundary. `cut` may
            // land mid-codepoint only for multibyte sequences; advance until
            // `self.buf.is_char_boundary(cut)` holds.
            let mut safe_cut = cut;
            while safe_cut < self.buf.len() && !self.buf.is_char_boundary(safe_cut) {
                safe_cut += 1;
            }
            self.buf = self.buf[safe_cut..].to_string();
        }

        let matched = extract_resume_id(&self.buf);
        match matched {
            Some((prefix, id)) => {
                // Only surface ids we haven't already reported this scanner.
                if self.last_matched.as_deref() == Some(id.as_str()) {
                    None
                } else {
                    self.last_matched = Some(id.clone());
                    Some((prefix, id))
                }
            }
            None => None,
        }
    }
}

/// Stateless scan of a single text blob. Returns the last (= newest)
/// `(prefix_without_trailing_space, id)` match, or `None`. Used by the
/// backend app-exit path over an accumulated output snapshot.
pub fn scan_text_for_resume_id(text: &str) -> Option<(String, String)> {
    extract_resume_id(text)
}

/// Scan for every known agent resume prefix and return the last (= newest)
/// match as `(prefix_without_trailing_space, id)`.
fn extract_resume_id(text: &str) -> Option<(String, String)> {
    let lower = text.to_lowercase();

    let mut last_match: Option<(String, String)> = None;

    for pat in PREFIXES {
        let pat_lower = pat.to_lowercase();
        let mut search_from = 0;
        while let Some(idx) = lower[search_from..].find(&pat_lower) {
            let match_start = search_from + idx;
            let id_start = match_start + pat.len();
            if id_start > text.len() {
                break;
            }
            let rest = &text[id_start..];
            let id: String = rest
                .chars()
                .take(MAX_ID_LEN)
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !id.is_empty() {
                // prefix without trailing space for convenience
                last_match = Some((pat[..pat.len() - 1].to_string(), id));
            }
            search_from = id_start;
            if search_from >= lower.len() {
                break;
            }
        }
    }

    last_match
}

/// Strip ANSI escape sequences (CSI, OSC, charset, and other two-byte
/// escapes) from `input`. Ported byte-for-byte from the frontend scanner
/// so both sides agree on what "stripped" means.
///
/// UTF-8 continuation bytes are all `>= 0x80`, so no escape byte can fall
/// inside a multibyte codepoint. We therefore only ever cut the string at
/// ASCII boundaries, leaving every remaining run valid UTF-8.
fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        if bytes[i] == 0x1b && i + 1 < n {
            match bytes[i + 1] {
                // CSI: ESC [ <params/intermediates> <final 0x40..=0x7e>
                b'[' => {
                    i += 2;
                    while i < n && !(bytes[i] >= 0x40 && bytes[i] <= 0x7e) {
                        i += 1;
                    }
                    if i < n {
                        i += 1; // consume the final byte
                    }
                }
                // OSC: ESC ] ... (terminated by BEL 0x07 or ST "ESC \")
                b']' => {
                    i += 2;
                    while i < n {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < n && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                // Other two-byte escapes: ESC ( B, ESC =, ESC M, ...
                _ => {
                    i += 2;
                }
            }
        } else {
            // Lone trailing ESC (no following byte) is kept as-is.
            out.push(bytes[i]);
            i += 1;
        }
    }
    // Safe: only ASCII escape runs were removed.
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "2d63f514-75ac-4cca-96f4-0d78fa2941b3";

    #[test]
    fn plain_line() {
        let mut s = ResumeScanner::new();
        assert_eq!(
            s.feed(&format!(
                "Resume this session with:\nclaude --resume {UUID}\n"
            )),
            Some(("claude --resume".to_string(), UUID.to_string()))
        );
    }

    #[test]
    fn line_with_ansi_escape_codes() {
        // Claude colors the exit line: the bold/color codes interleave the
        // words `claude`, `--resume`, and the id. Without stripping, the
        // literal substring "claude --resume " is never present and the
        // scan would silently miss the resume id.
        let mut s = ResumeScanner::new();
        let text = format!("\x1b[1mclaude\x1b[0m \x1b[36m--resume\x1b[0m {UUID}\n");
        assert_eq!(
            s.feed(&text),
            Some(("claude --resume".to_string(), UUID.to_string()))
        );
    }

    #[test]
    fn handles_osc_sequence_prefix() {
        let mut s = ResumeScanner::new();
        // OSC title-set sequence before the resume line, plus a colored
        // resume line — both must not confuse the scanner.
        let text =
            format!("\x1b]0;claude\x07some preamble\n\x1b[32mclaude --resume {UUID}\x1b[0m\n",);
        assert_eq!(
            s.feed(&text),
            Some(("claude --resume".to_string(), UUID.to_string()))
        );
    }

    #[test]
    fn dedupes_subsequent_feeds() {
        let mut s = ResumeScanner::new();
        // First feed: emit the id.
        let prefix = format!("prefix\nclaude --resume {UUID}\n");
        assert_eq!(
            s.feed(&prefix),
            Some(("claude --resume".to_string(), UUID.to_string()))
        );
        // The resume line is newline-terminated (as in real agent output),
        // so the buffer still holds it after the match. Subsequent feeds
        // must NOT re-emit the same id.
        assert_eq!(s.feed("more trailing output\n"), None);
    }

    #[test]
    fn stateless_scan_matches_stateful() {
        let text = format!("preamble\nclaude --resume {UUID}\ntrailer");
        assert_eq!(
            scan_text_for_resume_id(&text),
            Some(("claude --resume".to_string(), UUID.to_string()))
        );
    }

    #[test]
    fn no_match_returns_none() {
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed("just some shell output, nothing to see\n"), None);
        assert_eq!(scan_text_for_resume_id("no resume line here"), None);
    }

    #[test]
    fn supports_each_known_prefix() {
        for (pat, expected_prefix) in [
            ("claude --resume ", "claude --resume"),
            ("codex --resume ", "codex --resume"),
            ("opencode --resume ", "opencode --resume"),
            ("gemini --resume ", "gemini --resume"),
        ] {
            let text = format!("{pat}{UUID}\n");
            assert_eq!(
                scan_text_for_resume_id(&text),
                Some((expected_prefix.to_string(), UUID.to_string())),
                "prefix {pat:?} should match"
            );
        }
    }

    #[test]
    fn newest_match_wins_when_multiple_present() {
        let text = format!("claude --resume older-id-aaaa\nsome mid text\ncodex --resume {UUID}\n");
        // codex appears last, so it should win regardless of preference order.
        assert_eq!(
            scan_text_for_resume_id(&text),
            Some(("codex --resume".to_string(), UUID.to_string()))
        );
    }

    #[test]
    fn chunk_boundary_match() {
        // The resume line split across two feeds must still match, because
        // the scanner accumulates a rolling buffer.
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed("Resume this session with:\nclau"), None);
        assert_eq!(
            s.feed(&format!("de --resume {UUID}\n")),
            Some(("claude --resume".to_string(), UUID.to_string()))
        );
    }
}
