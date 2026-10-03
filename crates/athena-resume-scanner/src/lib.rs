//! Shared, dependency-free parsing for agent resume hints in PTY output.
//!
//! The frontend and backend own different scanner lifecycles, but they must
//! agree on the byte-level parsing rules. Keeping those rules here prevents
//! ANSI handling and supported CLI prefixes from drifting between targets.
//!
//! Supported hints include both `--resume` and harness-specific continuation
//! forms such as Freebuff's `freebuff --continue <timestamp>`.

use std::collections::HashSet;

/// Maximum rolling-buffer size used by stateful scanner adapters.
pub const MAX_SCAN_BUFFER: usize = 1024;

const MAX_ID_LEN: usize = 256;
// Must stay pre-lowercased: `extract_resume_id` matches these against
// lowercased text without re-lowercasing them per scan.
const PREFIXES: &[&str] = &[
    "claude --resume ",
    "codex --resume ",
    "opencode --resume ",
    "gemini --resume ",
    "freebuff --continue ",
    "omp --resume ",
];

const fn max_prefix_len() -> usize {
    let mut max = 0;
    let mut i = 0;
    while i < PREFIXES.len() {
        if PREFIXES[i].len() > max {
            max = PREFIXES[i].len();
        }
        i += 1;
    }
    max
}

/// Length of the longest entry in [`PREFIXES`]; used to size the straddle
/// overlap when a stateful scanner scans only newly appended text.
const MAX_PREFIX_LEN: usize = max_prefix_len();

/// Streaming ANSI remover that preserves partial escape-sequence state across
/// input chunks. It emits ordinary text immediately and suppresses complete
/// CSI/OSC/simple escape sequences.
pub struct AnsiStripper {
    state: AnsiState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnsiState {
    Text,
    Escape,
    Csi,
    Osc,
    OscEscape,
}

impl Default for AnsiStripper {
    fn default() -> Self {
        Self {
            state: AnsiState::Text,
        }
    }
}

impl AnsiStripper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one valid UTF-8 chunk and return its ANSI-free text.
    pub fn feed(&mut self, input: &str) -> String {
        let bytes = input.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;

        while i < bytes.len() {
            let byte = bytes[i];
            match self.state {
                AnsiState::Text => {
                    if byte == 0x1b {
                        self.state = AnsiState::Escape;
                    } else {
                        out.push(byte);
                    }
                    i += 1;
                }
                AnsiState::Escape => {
                    match byte {
                        b'[' => self.state = AnsiState::Csi,
                        b']' => self.state = AnsiState::Osc,
                        _ => {
                            // Lone ESC followed by a non-sequence byte: fall
                            // back to text mode WITHOUT consuming the byte, so
                            // it is reprocessed below as ordinary output
                            // instead of being silently eaten.
                            self.state = AnsiState::Text;
                            continue;
                        }
                    }
                    i += 1;
                }
                AnsiState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.state = AnsiState::Text;
                    }
                    i += 1;
                }
                AnsiState::Osc => {
                    if byte == 0x07 {
                        self.state = AnsiState::Text;
                    } else if byte == 0x1b {
                        self.state = AnsiState::OscEscape;
                    }
                    i += 1;
                }
                AnsiState::OscEscape => {
                    self.state = if byte == b'\\' {
                        AnsiState::Text
                    } else {
                        AnsiState::Osc
                    };
                    i += 1;
                }
            }
        }

        String::from_utf8(out).unwrap_or_default()
    }

    /// Reset an incomplete escape sequence, for example when a PTY pane is
    /// explicitly reset or discarded.
    pub fn clear(&mut self) {
        self.state = AnsiState::Text;
    }
}

/// Remove complete ANSI escape sequences from one already-complete snapshot.
/// For streaming input, prefer [`AnsiStripper`] so split sequences are handled.
pub fn strip_ansi(input: &str) -> String {
    let mut stripper = AnsiStripper::new();
    stripper.feed(input)
}

fn has_digit_shape(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_freebuff_timestamp(id: &str) -> bool {
    let Some((date, time)) = id.split_once('T') else {
        return false;
    };
    let date_parts: Vec<&str> = date.split('-').collect();
    let time_parts: Vec<&str> = time.split('-').collect();
    if date_parts.len() != 3 || time_parts.len() != 3 {
        return false;
    }
    if !has_digit_shape(date_parts[0], 4)
        || !has_digit_shape(date_parts[1], 2)
        || !has_digit_shape(date_parts[2], 2)
        || !has_digit_shape(time_parts[0], 2)
        || !has_digit_shape(time_parts[1], 2)
    {
        return false;
    }
    let Some((seconds, fraction_and_zone)) = time_parts[2].split_once('.') else {
        return false;
    };
    let Some(fraction) = fraction_and_zone.strip_suffix('Z') else {
        return false;
    };
    has_digit_shape(seconds, 2)
        && !fraction.is_empty()
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

fn is_valid_resume_id(prefix: &str, id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return false;
    }
    match prefix {
        "freebuff --continue" => is_freebuff_timestamp(id),
        _ => true,
    }
}

/// Extract the newest known agent resume/continuation hint from already-clean
/// `text`. IDs remain bounded to shell-safe token characters. Harness-specific
/// IDs use stricter formats so adjacent shell text cannot be absorbed into an
/// otherwise valid timestamp.
pub fn extract_resume_id(text: &str) -> Option<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let mut last_match: Option<(usize, String, String)> = None;

    for pattern in PREFIXES {
        let mut search_from = 0;
        while let Some(relative_idx) = lower[search_from..].find(pattern) {
            let match_start = search_from + relative_idx;
            let id_start = match_start + pattern.len();
            if id_start > text.len() {
                break;
            }

            let id: String = text[id_start..]
                .chars()
                .take(MAX_ID_LEN)
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                .collect::<String>()
                .trim_end_matches('.')
                .to_string();
            let prefix = pattern[..pattern.len() - 1].to_string();
            if is_valid_resume_id(&prefix, &id)
                && last_match
                    .as_ref()
                    .is_none_or(|(previous_start, _, _)| match_start >= *previous_start)
            {
                last_match = Some((match_start, prefix, id));
            }

            search_from = id_start;
            if search_from >= lower.len() {
                break;
            }
        }
    }

    last_match.map(|(_, prefix, id)| (prefix, id))
}

/// Strip ANSI sequences and extract the newest resume hint from one snapshot.
pub fn scan_text_for_resume_id(text: &str) -> Option<(String, String)> {
    extract_resume_id(&strip_ansi(text))
}

/// A stateful scanner for a single pane. Create one, feed it text via
/// [`ResumeScanner::feed`], and obtain newly detected resume ids. Each
/// distinct id is returned at most once (deduplicated across feeds).
///
/// This is the single shared owner of the rolling-buffer/dedup logic used by
/// both the backend (app-exit capture) and the frontend (live xterm stream).
pub struct ResumeScanner {
    /// Accumulated ANSI-stripped tail of the last feed(s); never grows
    /// beyond [`MAX_SCAN_BUFFER`].
    buf: String,
    /// Every `(prefix, id)` pair reported during this PTY lifecycle. A single
    /// `last_matched` value is insufficient because replayed/redrawn output
    /// can alternate between older IDs and cause the same ID to be emitted
    /// repeatedly. Keying on the pair keeps a shared id under a different CLI
    /// prefix from being wrongly suppressed.
    seen_ids: HashSet<(String, String)>,
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
            seen_ids: HashSet::new(),
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
        let tail_len = stripped.len();
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

        // Only the newly appended tail can produce a *new* match. A prefix or
        // a previously truncated id may straddle the append boundary, so
        // include the maximum straddle span (`MAX_PREFIX_LEN + MAX_ID_LEN`)
        // of older text. Anything matched entirely before that window was
        // already reported (or deduplicated) by an earlier feed.
        let mut scan_from = self
            .buf
            .len()
            .saturating_sub(tail_len + MAX_PREFIX_LEN + MAX_ID_LEN);
        while scan_from < self.buf.len() && !self.buf.is_char_boundary(scan_from) {
            scan_from += 1;
        }
        let matched = extract_resume_id(&self.buf[scan_from..]);
        match matched {
            Some((prefix, id)) => {
                // Only surface ids we haven't already reported this scanner.
                // This is intentionally a set rather than a last-value check:
                // terminal redraw/replay can alternate between several old
                // resume lines.
                if self.seen_ids.insert((prefix.clone(), id.clone())) {
                    Some((prefix, id))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    /// Reset the scanner state and discard any buffered partial match.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.seen_ids.clear();
        self.ansi.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "2d63f514-75ac-4cca-96f4-0d78fa2941b3";

    #[test]
    fn strips_csi_and_osc_sequences() {
        assert_eq!(
            strip_ansi("\x1b[1mclaude\x1b[0m \x1b]0;title\x07text"),
            "claude text"
        );
    }

    #[test]
    fn preserves_unicode_while_stripping() {
        assert_eq!(strip_ansi("\x1b[32m┏━┓ café\x1b[0m"), "┏━┓ café");
    }

    #[test]
    fn stateful_stripper_handles_split_csi_and_osc() {
        let mut stripper = AnsiStripper::new();
        assert_eq!(stripper.feed("before\x1b[3"), "before");
        assert_eq!(stripper.feed("1mclaude\x1b[0m after"), "claude after");

        let mut stripper = AnsiStripper::new();
        assert_eq!(stripper.feed("\x1b]0;ti"), "");
        assert_eq!(stripper.feed("tle\x07claude --resume "), "claude --resume ");
    }

    #[test]
    fn extracts_each_supported_prefix() {
        for (prefix, expected) in [
            ("claude --resume", "claude --resume"),
            ("codex --resume", "codex --resume"),
            ("opencode --resume", "opencode --resume"),
            ("gemini --resume", "gemini --resume"),
            ("omp --resume", "omp --resume"),
        ] {
            assert_eq!(
                extract_resume_id(&format!("{prefix} {ID}\n")),
                Some((expected.to_string(), ID.to_string()))
            );
        }
    }

    #[test]
    fn preserves_freebuff_timestamp_and_omp_uuid_ids() {
        assert_eq!(
            extract_resume_id("freebuff --continue 2026-08-15T11-30-56.357Z\n"),
            Some((
                "freebuff --continue".to_string(),
                "2026-08-15T11-30-56.357Z".to_string()
            ))
        );
        assert_eq!(
            extract_resume_id("omp --resume 019ff77f-fadb-7000-b51d-b7b38c9cb0eb\n"),
            Some((
                "omp --resume".to_string(),
                "019ff77f-fadb-7000-b51d-b7b38c9cb0eb".to_string()
            ))
        );
    }

    #[test]
    fn rejects_shell_syntax_after_resume_id() {
        assert_eq!(
            extract_resume_id("claude --resume safe.id; rm -rf /\n"),
            Some(("claude --resume".to_string(), "safe.id".to_string()))
        );
        assert_eq!(
            extract_resume_id("freebuff --continue 2026-08-17T21-35-03.500Z; rm -rf /\n"),
            Some((
                "freebuff --continue".to_string(),
                "2026-08-17T21-35-03.500Z".to_string()
            ))
        );
    }

    #[test]
    fn newest_match_wins_by_text_position() {
        assert_eq!(
            extract_resume_id(&format!("codex --resume old-id\nclaude --resume {ID}")),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn scan_snapshot_strips_ansi_before_extracting() {
        assert_eq!(
            scan_text_for_resume_id(&format!("\x1b[1mclaude --resume\x1b[0m {ID}")),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(
            extract_resume_id(&format!("CLAUDE --RESUME {ID}")),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn rejects_plain_text_and_invalid_ids() {
        assert_eq!(extract_resume_id("no resume hint"), None);
        assert_eq!(extract_resume_id("claude --resume !!!"), None);
    }

    // -- Stateful ResumeScanner tests (consolidated from the former duplicate
    // -- copies in athena-core and the frontend) -----------------------------

    #[test]
    fn stateful_plain_line() {
        let mut s = ResumeScanner::new();
        assert_eq!(
            s.feed(&format!(
                "Resume this session with:\nclaude --resume {ID}\n"
            )),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_line_with_ansi_escape_codes() {
        // Claude colors the exit line: the bold/color codes interleave the
        // words `claude`, `--resume`, and the id. Without stripping, the
        // literal substring "claude --resume " is never present.
        let mut s = ResumeScanner::new();
        let text = format!("\x1b[1mclaude\x1b[0m \x1b[36m--resume\x1b[0m {ID}\n");
        assert_eq!(
            s.feed(&text),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_handles_osc_sequence_prefix() {
        let mut s = ResumeScanner::new();
        let text =
            format!("\x1b]0;claude\x07some preamble\n\x1b[32mclaude --resume {ID}\x1b[0m\n",);
        assert_eq!(
            s.feed(&text),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_dedupes_subsequent_feeds() {
        let mut s = ResumeScanner::new();
        let prefix = format!("prefix\nclaude --resume {ID}\n");
        assert_eq!(
            s.feed(&prefix),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
        assert_eq!(s.feed("more trailing output\n"), None);
        assert_eq!(s.feed("even more\n"), None);
    }

    #[test]
    fn stateful_chunk_boundary_match() {
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed("Resume this session with:\nclau"), None);
        assert_eq!(
            s.feed(&format!("de --resume {ID}\n")),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_newest_match_wins() {
        let mut s = ResumeScanner::new();
        let text = format!("claude --resume older-id-aaaa\nsome mid text\ncodex --resume {ID}\n");
        assert_eq!(
            s.feed(&text),
            Some(("codex --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_preserves_harness_specific_ids() {
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
    fn stateful_updates_to_newer_id() {
        let mut s = ResumeScanner::new();
        let id1 = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let id2 = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        assert_eq!(
            s.feed(&format!("claude --resume {id1}\n")),
            Some(("claude --resume".to_string(), id1.to_string()))
        );
        assert_eq!(
            s.feed(&format!("claude --resume {id2}\n")),
            Some(("claude --resume".to_string(), id2.to_string()))
        );
    }

    #[test]
    fn stateful_dedupes_nonconsecutive_repeated_ids() {
        let mut s = ResumeScanner::new();
        let id1 = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let id2 = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        assert!(s.feed(&format!("claude --resume {id1}\n")).is_some());
        assert!(s.feed(&format!("claude --resume {id2}\n")).is_some());
        assert_eq!(s.feed(&format!("claude --resume {id1}\n")), None);
    }

    #[test]
    fn stateful_rejects_freebuff_timestamp_concatenated_with_next_command() {
        assert_eq!(
            extract_resume_id("freebuff --continue 2026-08-17T21-35-03.500Zfreebuff"),
            None
        );
    }

    #[test]
    fn stateful_large_chunk_with_line_in_middle() {
        let mut s = ResumeScanner::new();
        let prefix = "a".repeat(200);
        let suffix = "b".repeat(200);
        let text = format!("{prefix}claude --resume {ID}\n{suffix}");
        assert_eq!(
            s.feed(&text),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_box_drawing_after_line() {
        let mut s = ResumeScanner::new();
        let box_chars = "─".repeat(120);
        let text = format!("claude --resume {ID}\n{box_chars}\n  /help for help\n");
        assert_eq!(
            s.feed(&text),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_codex_variant() {
        let mut s = ResumeScanner::new();
        assert_eq!(
            s.feed("codex --resume abc123\n"),
            Some(("codex --resume".to_string(), "abc123".to_string()))
        );
    }

    #[test]
    fn stateful_no_match_on_plain_text() {
        let mut s = ResumeScanner::new();
        assert_eq!(s.feed("Hello world, nothing here\n"), None);
        assert_eq!(s.feed("Some file named claude-resume.txt\n"), None);
    }

    #[test]
    fn stateful_rolling_buffer_drops_old_data() {
        let mut s = ResumeScanner::new();
        let big = "x".repeat(MAX_SCAN_BUFFER + 100);
        s.feed(&big);
        assert_eq!(
            s.feed(&format!("claude --resume {ID}")),
            Some(("claude --resume".to_string(), ID.to_string()))
        );
    }

    #[test]
    fn stateful_realistic_claude_exit_stream_chunked() {
        let mut s = ResumeScanner::new();

        let full = format!(
            "\x1b[?25l\x1b[2J\x1b[H\
             \x1b[1m┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓\x1b[0m\n\
             \x1b[1m┃  Session complete             ┃\x1b[0m\n\
             \x1b[1m┃  Resume this session with:    ┃\x1b[0m\n\
             \x1b[1m┃  \x1b[1mclaude\x1b[0m \x1b[36m--resume\x1b[0m {ID} \x1b[1m┃\x1b[0m\n\
             \x1b[1m┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛\x1b[0m\n\
             \n\
             \x1b[2mTotal cost: $0.04                              \x1b[0m\n\
             \x1b[2mDuration: 12s                                  \x1b[0m\n\
             \x1b[?25h\
             \x1b[?2004h%",
        );

        let chars: Vec<&str> = full.split_inclusive(|_: char| true).collect();
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
        if pos < chars.len() {
            let chunk: String = chars[pos..].concat();
            if let Some(c) = s.feed(&chunk) {
                assert!(captured.is_none(), "captured more than once");
                captured = Some(c);
            }
        }

        assert_eq!(s.feed("user@host athenas-core % "), None);

        let (prefix, id) = captured.expect("scanner failed to capture the resume id");
        assert_eq!(prefix, "claude --resume");
        assert_eq!(id, ID);
    }

    #[test]
    fn lone_escape_does_not_eat_following_character() {
        // A stray ESC followed by plain text must not swallow the next byte:
        // "\x1bc" strips the ESC but keeps the 'c'.
        assert_eq!(strip_ansi("x\x1bclaude --resume y"), "xclaude --resume y");
        // Split across feed boundaries too.
        let mut stripper = AnsiStripper::new();
        assert_eq!(stripper.feed("ab\x1b"), "ab");
        assert_eq!(stripper.feed("c"), "c");
    }

    #[test]
    fn strips_trailing_period_and_unicode_from_id() {
        // A sentence-ending period must not be part of the id.
        assert_eq!(
            extract_resume_id("run claude --resume abc123.\nnext"),
            Some(("claude --resume".to_string(), "abc123".to_string()))
        );
        // Non-ASCII text right after the id must not be absorbed.
        assert_eq!(
            extract_resume_id("claude --resume abc123日本語\n"),
            Some(("claude --resume".to_string(), "abc123".to_string()))
        );
    }

    #[test]
    fn stateful_dedup_key_includes_prefix() {
        let mut s = ResumeScanner::new();
        let id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        assert_eq!(
            s.feed(&format!("claude --resume {id}\n")),
            Some(("claude --resume".to_string(), id.to_string()))
        );
        // The same id under a different CLI prefix is a different match.
        assert_eq!(
            s.feed(&format!("codex --resume {id}\n")),
            Some(("codex --resume".to_string(), id.to_string()))
        );
        // But the same (prefix, id) pair still dedupes.
        assert_eq!(s.feed(&format!("claude --resume {id}\n")), None);
    }
}
