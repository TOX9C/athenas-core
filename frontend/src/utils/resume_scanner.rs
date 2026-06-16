//! Robust scanner for agent resume IDs in PTY output streams.
//!
//! Claude Code (and similar tools) print a line like `claude --resume <id>`
//! on exit. The line arrives as raw PTY bytes split into arbitrary chunks,
//! so a naïve "scan the last N bytes of each individual chunk" approach
//! misses matches that span chunk boundaries or sit earlier than the
//! scanned window.
//!
//! `ResumeScanner` keeps a small rolling text buffer per pane. Each new
//! chunk is ANSI-stripped (so color/bold codes interleaved between the
//! words `claude`, `--resume`, and the id do not break the match),
//! appended, the *entire* buffer is searched, then the buffer is trimmed
//! to a bounded size. A match is only reported once per distinct id, so a
//! long-running session does not re-write the store on every subsequent
//! PTY chunk.
const MAX_SCAN_BUFFER: usize = 1024;
const MAX_ID_LEN: usize = 256;

/// A stateful scanner for a single pane. Create one, feed it text via
/// `feed`, and obtain newly detected resume ids. Each distinct id is
/// returned at most once (deduplicated across feeds).
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
    fn default() -> Self { Self::new() }
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
    /// reported this mount), returns `(prefix, id)` where `prefix` is the
    /// matched CLI prefix (e.g. "claude --resume") and `id` is the resume
    /// session identifier. `None` means no new match.
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
                // Only surface ids we haven't already reported this mount.
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

    /// Manually clear the buffer (e.g. after a pane reset).
    pub fn clear(&mut self) {
        self.buf.clear();
        self.last_matched = None;
    }
}

/// Remove ANSI escape sequences (CSI SGR/color, OSC, and simple two-byte
/// escapes) from `input`.
///
/// This is byte-level but UTF-8-safe: escape sequences are pure ASCII, and
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

/// Scan for every known agent resume prefix and return the last (= newest)
/// match as `(prefix_without_trailing_space, id)`.
fn extract_resume_id(text: &str) -> Option<(String, String)> {
    let lower = text.to_lowercase();

    // Known CLI resume prefixes, in order of preference.
    const PREFIXES: &[&str] = &[
        "claude --resume ",
        "codex --resume ",
        "opencode --resume ",
        "gemini --resume ",
    ];

    let mut last_match: Option<(String, String)> = None;

    for pat in PREFIXES {
        let pat_lower = pat.to_lowercase();
        let mut search_from = 0;
        while let Some(idx) = lower[search_from..].find(&pat_lower) {
            let match_start = search_from + idx;
            let id_start = match_start + pat.len();
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "2d63f514-75ac-4cca-96f4-0d78fa2941b3";

    #[test]
    fn plain_line() {
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed(&format!("Resume this session with:\nclaude --resume {UUID}\n")),
                   Some(("claude --resume".to_string(), UUID.to_string())));
    }

    #[test]
    fn line_with_ansi_escape_codes() {
        // Claude colors the exit line: the bold/color codes interleave the
        // words `claude`, `--resume`, and the id. Without stripping these,
        // the literal substring "claude --resume " is never present and the
        // scan would silently miss the resume id.
        let mut s = ResumeScanner::new();
        let text = format!("\x1b[1mclaude\x1b[0m \x1b[36m--resume\x1b[0m {UUID}\n");
        assert_eq!(s.feed(&text),
                   Some(("claude --resume".to_string(), UUID.to_string())));
    }

    #[test]
    fn ansi_around_the_whole_line() {
        // OSC title sequence + colored wrapper — still matches.
        let mut s = ResumeScanner::new();
        let text = format!(
            "\x1b]0;claude\x07some preamble\n\x1b[32mclaude --resume {UUID}\x1b[0m\n",
        );
        assert_eq!(s.feed(&text),
                   Some(("claude --resume".to_string(), UUID.to_string())));
    }

    #[test]
    fn large_chunk_with_line_in_middle() {
        let mut s = ResumeScanner::new();
        let prefix = "a".repeat(200);
        let suffix = "b".repeat(200);
        // The resume line is newline-terminated (as in real agent output),
        // so the id extractor stops at the `\n` rather than greedily
        // consuming the trailing filler bytes.
        let text = format!("{prefix}claude --resume {UUID}\n{suffix}");
        let exp = ("claude --resume".to_string(), UUID.to_string());
        assert_eq!(s.feed(&text), Some(exp));
    }

    #[test]
    fn split_across_two_chunks() {
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed("claude --re"), None);
        assert_eq!(s.feed(&format!("sume {UUID}\n")),
                   Some(("claude --resume".to_string(), UUID.to_string())));
    }

    #[test]
    fn box_drawing_after_line() {
        let mut s = ResumeScanner::new();
        let box_chars = "─".repeat(120);
        let text = format!("claude --resume {UUID}\n{box_chars}\n  /help for help\n");
        assert_eq!(s.feed(&text), Some(("claude --resume".to_string(), UUID.to_string())));
    }

    #[test]
    fn updates_to_newer_id() {
        let mut s = ResumeScanner::new();
        let id1 = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let id2 = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

        // First session
        assert_eq!(s.feed(&format!("claude --resume {id1}\n")),
                   Some(("claude --resume".to_string(), id1.to_string())));

        // Second session started later — should overwrite
        assert_eq!(s.feed(&format!("claude --resume {id2}\n")),
                   Some(("claude --resume".to_string(), id2.to_string())));
    }

    #[test]
    fn does_not_re_report_same_id() {
        // Once an id is reported, subsequent feeds that still contain the
        // same line must NOT re-report it (the caller would otherwise
        // re-write the store on every PTY chunk for the rest of the
        // session).
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed(&format!("claude --resume {UUID}\n")),
                   Some(("claude --resume".to_string(), UUID.to_string())));
        // Same line still in buffer + more output streaming:
        assert_eq!(s.feed("more output, shell prompt returns\n"), None);
        assert_eq!(s.feed("even more\n"), None);
    }

    #[test]
    fn no_match_on_plain_text() {
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed("Hello world, nothing here\n"), None);
        assert_eq!(s.feed("Some file named claude-resume.txt\n"), None);
    }

    #[test]
    fn codex_variant() {
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed("codex --resume abc123\n"),
                   Some(("codex --resume".to_string(), "abc123".to_string())));
    }

    #[test]
    fn rolling_buffer_drops_old_data() {
        let mut s = ResumeScanner::new();
        // Fill buffer beyond MAX_SCAN_BUFFER
        let big = "x".repeat(MAX_SCAN_BUFFER + 100);
        s.feed(&big);
        // Old data is dropped, but this just tests it doesn't crash.
        assert_eq!(s.feed(&format!("claude --resume {UUID}")),
                   Some(("claude --resume".to_string(), UUID.to_string())));
    }

    /// Realistic end-to-end simulation of a Claude Code exit, fed as the
    /// kind of arbitrary byte chunks a PTY actually delivers: ANSI-colored
    /// TUI teardown, the boxed resume hint split across chunk boundaries, a
    /// trailing cost summary, and the shell prompt returning. This is the
    /// closest we can get to a live-CLI test without spawning `claude`.
    #[test]
    fn realistic_claude_exit_stream_chunked() {
        let mut s = ResumeScanner::new();

        // Simulated raw PTY output Claude emits on exit. The resume line is
        // ANSI-colored (bold command, cyan flag) and sits inside a box, with
        // session stats around it — exactly the shape that broke the old
        // no-ANSI-stripping scanner.
        let full = format!(
            "\x1b[?25l\x1b[2J\x1b[H\
             \x1b[1m┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓\x1b[0m\n\
             \x1b[1m┃  Session complete             ┃\x1b[0m\n\
             \x1b[1m┃  Resume this session with:    ┃\x1b[0m\n\
             \x1b[1m┃  \x1b[1mclaude\x1b[0m \x1b[36m--resume\x1b[0m {UUID} \x1b[1m┃\x1b[0m\n\
             \x1b[1m┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛\x1b[0m\n\
             \n\
             \x1b[2mTotal cost: $0.04                              \x1b[0m\n\
             \x1b[2mDuration: 12s                                  \x1b[0m\n\
             \x1b[?25h\
             \x1b[?2004h%",
        );

        // Feed it in irregular chunks (PTY reads are not line-aligned).
        // Split on char boundaries: the real pty_listen_raw path delivers
        // `String::from_utf8_lossy` output (never mid-codepoint), so the
        // scanner only ever sees well-formed UTF-8 chunks.
        let chars: Vec<&str> = full.split_inclusive(|c: char| true).collect();
        let mut captured = None;
        let mut pos = 0;
        for &size in &[7usize, 1, 64, 3, 200, 13, 40, 500] {
            let end = (pos + size).min(chars.len());
            if end == pos {
                continue;
            }
            let chunk: String = chars[pos..end].concat();
            if let Some((prefix, id)) = s.feed(&chunk) {
                assert!(captured.is_none(), "captured more than once");
                captured = Some((prefix, id));
            }
            pos = end;
            if pos >= chars.len() {
                break;
            }
        }
        // Feed any remainder.
        if pos < chars.len() {
            let chunk: String = chars[pos..].concat();
            if let Some(c) = s.feed(&chunk) {
                assert!(captured.is_none(), "captured more than once");
                captured = Some(c);
            }
        }

        // After the stream, feeding the returning shell prompt must NOT
        // re-report (dedup), even though the line is still in the buffer.
        assert_eq!(s.feed("user@host athenas-core % "), None);

        let (prefix, id) = captured.expect("scanner failed to capture the resume id");
        assert_eq!(prefix, "claude --resume");
        assert_eq!(id, UUID);
    }
}
