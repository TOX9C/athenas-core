//! Robust scanner for agent resume IDs in PTY output streams.
//!
//! The frontend and backend use different scanner lifecycles, but share the
//! same dependency-free parser so supported prefixes and ANSI handling cannot
//! drift. Supported harnesses print a continuation line such as
//! `claude --resume <id>` or `freebuff --continue <id>` on clean exit. The line
//! arrives as raw PTY bytes split into arbitrary
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

use athena_resume_scanner::{
    extract_resume_id, scan_text_for_resume_id as scan_snapshot, AnsiStripper, MAX_SCAN_BUFFER,
};

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
    ansi: AnsiStripper,
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
            ansi: AnsiStripper::new(),
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
        let stripped = self.ansi.feed(text);
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

    /// Reset the scanner state and discard any buffered partial match.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.last_matched = None;
        self.ansi.clear();
    }
}

/// Stateless scan of a single text blob. Returns the last (= newest)
/// `(prefix_without_trailing_space, id)` match, or `None`. Used by the
/// backend app-exit path over an accumulated output snapshot.
pub fn scan_text_for_resume_id(text: &str) -> Option<(String, String)> {
    scan_snapshot(text)
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
            ("freebuff --continue ", "freebuff --continue"),
            ("omp --resume ", "omp --resume"),
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
    fn preserves_harness_specific_ids() {
        let mut scanner = ResumeScanner::new();
        assert_eq!(
            scanner.feed("freebuff --continue 2026-08-15T11-30-56.357Z\n"),
            Some((
                "freebuff --continue".to_string(),
                "2026-08-15T11-30-56.357Z".to_string()
            ))
        );

        scanner.clear();
        assert_eq!(
            scanner.feed("omp --resume 019ff77f-fadb-7000-b51d-b7b38c9cb0eb\n"),
            Some((
                "omp --resume".to_string(),
                "019ff77f-fadb-7000-b51d-b7b38c9cb0eb".to_string()
            ))
        );
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
