//! Integration tests: resume-hint extraction from terminal output.
//!
//! Contracts under defense:
//! - `extract_resume_id` returns the NEWEST (rightmost) hint among all
//!   supported prefixes and excludes the trailing space from the prefix;
//! - freebuff timestamps must match `YYYY-MM-DD-HH-MM-SS.<frac>Z` — plain
//!   words after `freebuff --continue ` do not count;
//! - ids are bounded to alphanumeric/-/_/. tokens, max 256 chars;
//! - `scan_text_for_resume_id` strips ANSI first (CSI + OSC);
//! - the stateful `ResumeScanner` reassembles hints split across feeds,
//!   tolerates escape sequences split mid-sequence across chunk boundaries,
//!   deduplicates repeated ids (redraw/replay safe), and the 1024-byte
//!   rolling window still finds a hint at the tail;
//! - `clear` resets dedup so the same id can be reported again.

use athena_resume_scanner::{extract_resume_id, scan_text_for_resume_id, ResumeScanner};

const CLAUDE_ID: &str = "2d63f514-75ac-4cca-96f4-0d78fa2941b3";
const FREEBUFF_TS: &str = "2026-08-02T14-30-05.123Z";

#[test]
fn newest_hint_wins_across_prefixes() {
    let text =
        format!("claude --resume {CLAUDE_ID}\nomp --resume older-id-123\ncodex --resume newest-9");
    let (prefix, id) = extract_resume_id(&text).expect("hint found");
    assert_eq!(prefix, "codex --resume");
    assert_eq!(id, "newest-9");
}

#[test]
fn prefix_has_no_trailing_space_and_id_stops_at_whitespace() {
    let (prefix, id) = extract_resume_id(&format!("claude --resume {CLAUDE_ID}\n")).unwrap();
    assert_eq!(prefix, "claude --resume");
    assert!(!prefix.ends_with(' '));
    assert_eq!(id, CLAUDE_ID);
    assert!(
        extract_resume_id("claude --resume \n").is_none(),
        "no id → no hint"
    );
}

#[test]
fn freebuff_requires_timestamp_shape() {
    // Valid timestamp accepted.
    let (prefix, id) = extract_resume_id(&format!("freebuff --continue {FREEBUFF_TS}")).unwrap();
    assert_eq!(prefix, "freebuff --continue");
    assert_eq!(id, FREEBUFF_TS);
    // Shape-only validation: month/day/hour ranges are NOT calendar-checked,
    // so 2026-13-99 passes. What must fail: wrong field counts, missing
    // fraction, missing Z, non-digit fields.
    assert!(extract_resume_id("freebuff --continue 2026-08-02T14-30.1Z").is_none());
    assert!(extract_resume_id("freebuff --continue 2026-08-02T14-30-05.1").is_none());
    assert!(extract_resume_id("freebuff --continue 2026-AB-02T14-30-05.1Z").is_none());
    assert!(extract_resume_id("freebuff --continue 2026-08-02T14-30-05Z").is_none());
    // Same garbage is fine for a prefix without a strict format.
    assert_eq!(
        extract_resume_id("claude --resume oops").unwrap(),
        ("claude --resume".into(), "oops".into())
    );
}

#[test]
fn id_token_is_bounded_to_shell_safe_chars() {
    // Trailing punctuation outside the token set is not absorbed.
    let (prefix, id) = extract_resume_id("claude --resume abc.def-ghi_jkl: next").unwrap();
    assert_eq!(prefix, "claude --resume");
    assert_eq!(id, "abc.def-ghi_jkl");
    // Exactly 256 chars is accepted; the scanner TRUNCATES longer token runs
    // to MAX_ID_LEN, so a 257-char run yields the first 256 chars.
    let exact = "a".repeat(256);
    assert_eq!(
        extract_resume_id(&format!("claude --resume {exact}"))
            .unwrap()
            .1
            .len(),
        256
    );
    let long = "b".repeat(257);
    let (_, truncated) = extract_resume_id(&format!("claude --resume {long}")).unwrap();
    assert_eq!(truncated.len(), 256, "token run truncated to MAX_ID_LEN");
    assert_eq!(truncated, "b".repeat(256));
}

#[test]
fn scan_strips_ansi_before_matching() {
    let colored =
        format!("\x1b[1;36mclaude\x1b[0m \x1b[32m--resume\x1b[0m \x1b[4m{CLAUDE_ID}\x1b[0m done");
    let (prefix, id) = scan_text_for_resume_id(&colored).expect("hint through ANSI");
    assert_eq!(prefix, "claude --resume");
    assert_eq!(id, CLAUDE_ID);

    let osc = "\x1b]0;title\x07omp --resume abc-1".to_string();
    assert_eq!(
        scan_text_for_resume_id(&osc).unwrap(),
        ("omp --resume".into(), "abc-1".into())
    );
}

#[test]
fn scanner_reeassembles_hint_split_across_feeds() {
    let mut scanner = ResumeScanner::new();
    assert!(scanner.feed("starting up...\nclaude ").is_none());
    assert!(
        scanner.feed("--resu").is_none(),
        "split keyword not yet a hint"
    );
    let (prefix, id) = scanner
        .feed(&format!("me {CLAUDE_ID}\n"))
        .expect("hint once complete");
    assert_eq!(prefix, "claude --resume");
    assert_eq!(id, CLAUDE_ID);
}

#[test]
fn scanner_tolerates_escape_split_across_chunk_boundary() {
    let mut scanner = ResumeScanner::new();
    // ESC arrives at the end of one chunk, `[31m` at the start of the next.
    assert!(scanner.feed("claude --resume \x1b").is_none());
    let (prefix, id) = scanner.feed(&format!("[32m{CLAUDE_ID}")).expect("hint");
    assert_eq!(prefix, "claude --resume");
    assert_eq!(id, CLAUDE_ID);
}

#[test]
fn scanner_reports_each_id_once_until_cleared() {
    let line = format!("claude --resume {CLAUDE_ID}\n");
    let mut scanner = ResumeScanner::new();
    assert!(scanner.feed(&line).is_some(), "first sighting reported");
    assert!(scanner.feed(&line).is_none(), "duplicate suppressed");
    assert!(
        scanner.feed(&line).is_none(),
        "redraw replay still suppressed"
    );

    scanner.clear();
    assert!(scanner.feed(&line).is_some(), "clear re-arms the same id");
}

#[test]
fn rolling_window_keeps_tail_hint_findable() {
    let mut scanner = ResumeScanner::new();
    // 4 KiB of filler exceeds MAX_SCAN_BUFFER (1024); the hint at the tail
    // must still be found after older bytes are trimmed.
    let filler = "x".repeat(4096);
    assert!(scanner.feed(&filler).is_none());
    let (prefix, id) = scanner
        .feed("\nomp --resume tail-hint-1\n")
        .expect("hint at tail survives trimming");
    assert_eq!(prefix, "omp --resume");
    assert_eq!(id, "tail-hint-1");
}

#[test]
fn redraw_alternating_ids_are_each_reported_once() {
    let a = "claude --resume id-aaa-1\n".to_string();
    let b = "claude --resume id-bbb-2\n".to_string();
    let mut scanner = ResumeScanner::new();
    assert!(scanner.feed(&a).is_some());
    assert!(scanner.feed(&b).is_some());
    // Replay of both older lines: both ids already seen → nothing new.
    assert!(scanner.feed(&a).is_none());
    assert!(scanner.feed(&b).is_none());
}
