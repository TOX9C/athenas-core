//! Shared, dependency-free parsing for agent resume hints in PTY output.
//!
//! The frontend and backend own different scanner lifecycles, but they must
//! agree on the byte-level parsing rules. Keeping those rules here prevents
//! ANSI handling and supported CLI prefixes from drifting between targets.
//!
//! Supported hints include both `--resume` and harness-specific continuation
//! forms such as Freebuff's `freebuff --continue <timestamp>`.

/// Maximum rolling-buffer size used by stateful scanner adapters.
pub const MAX_SCAN_BUFFER: usize = 1024;

const MAX_ID_LEN: usize = 256;
const PREFIXES: &[&str] = &[
    "claude --resume ",
    "codex --resume ",
    "opencode --resume ",
    "gemini --resume ",
    "freebuff --continue ",
    "omp --resume ",
];

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
                        _ => self.state = AnsiState::Text,
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

/// Extract the newest known agent resume/continuation hint from already-clean
/// `text`. IDs remain bounded to shell-safe token characters; Freebuff's
/// timestamp format additionally requires `.`.
pub fn extract_resume_id(text: &str) -> Option<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let mut last_match: Option<(usize, String, String)> = None;

    for pattern in PREFIXES {
        let pattern_lower = pattern.to_ascii_lowercase();
        let mut search_from = 0;
        while let Some(relative_idx) = lower[search_from..].find(&pattern_lower) {
            let match_start = search_from + relative_idx;
            let id_start = match_start + pattern.len();
            if id_start > text.len() {
                break;
            }

            let id: String = text[id_start..]
                .chars()
                .take(MAX_ID_LEN)
                .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
                .collect();
            if !id.is_empty() {
                let candidate = (match_start, pattern[..pattern.len() - 1].to_string(), id);
                if last_match
                    .as_ref()
                    .is_none_or(|(previous_start, _, _)| match_start >= *previous_start)
                {
                    last_match = Some(candidate);
                }
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
            ("freebuff --continue", "freebuff --continue"),
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
            extract_resume_id("freebuff --continue safe.id; rm -rf /\n"),
            Some(("freebuff --continue".to_string(), "safe.id".to_string()))
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
}
