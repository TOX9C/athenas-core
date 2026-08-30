//! Integration tests: output buffer retention caps and pane lifecycle.
//!
//! Contracts under defense:
//! - line numbering stays monotonic across appends, interior empty lines are
//!   preserved, and the spurious trailing empty string from a `\n`-terminated
//!   chunk never becomes a phantom line;
//! - ANSI escape sequences are stripped before storage;
//! - the 5000-line cap keeps the NEWEST lines (oldest dropped) and the
//!   2 MB byte cap trims leading lines in one pass;
//! - get_output filters (since_line/since_time) then paginates (offset/limit)
//!   on the filtered set; get_output_tail reads only the newest N lines;
//! - dead-pane lifecycle: mark_pane_dead preserves history, cleanup_dead_panes
//!   removes exactly the dead panes, remove_pane counts;
//! - exit snapshots round-trip through disk (save_to_disk/load_from_disk).

use athena_core::output_buffer::{GetOutputOptions, OutputBuffer};

fn line_text(buf: &OutputBuffer, pane: &str) -> Vec<String> {
    buf.get_output(pane, None)
        .iter()
        .map(|l| l.text.clone())
        .collect()
}

#[test]
fn line_numbers_are_monotonic_and_trailing_newline_adds_no_phantom() {
    let buf = OutputBuffer::new();
    buf.append_output("p1", "alpha\nbeta\n", None);
    buf.append_output("p1", "gamma\n", None);

    let lines = buf.get_output("p1", None);
    let nums: Vec<u32> = lines.iter().map(|l| l.line_num).collect();
    assert_eq!(
        nums,
        vec![1, 2, 3],
        "line_num must be monotonic across appends"
    );
    assert_eq!(line_text(&buf, "p1"), vec!["alpha", "beta", "gamma"]);
}

#[test]
fn interior_empty_lines_survive_but_terminal_empty_does_not() {
    let buf = OutputBuffer::new();
    // "a\n\nb\n" → lines a, <empty>, b; trailing \n adds nothing extra.
    buf.append_output("p1", "a\n\nb\n", None);
    assert_eq!(line_text(&buf, "p1"), vec!["a", "", "b"]);
    // A chunk that is ONLY a newline still records one empty line.
    buf.append_output("p1", "\n", None);
    assert_eq!(line_text(&buf, "p1"), vec!["a", "", "b", ""]);
}

#[test]
fn ansi_escapes_are_stripped_before_storage() {
    let buf = OutputBuffer::new();
    buf.append_output(
        "p1",
        "\x1b[31mred\x1b[0m \x1b]0;title\x07plain \x1b(Bcharset\n",
        None,
    );
    assert_eq!(line_text(&buf, "p1"), vec!["red plain charset"]);
}

#[test]
fn line_cap_keeps_newest_5000_lines_and_drops_oldest() {
    let buf = OutputBuffer::new();
    // 6000 single-char lines: cap is 5000 → first retained line_num is 1001.
    let payload: String = (0..6000).map(|i| format!("L{i}\n")).collect();
    buf.append_output("p1", &payload, None);

    let info = buf.get_pane_buffer_info("p1").expect("pane exists");
    assert_eq!(info.line_count, 5000, "line cap is enforced");
    assert_eq!(
        info.total_lines, 6000,
        "total_lines counts every line ever appended"
    );

    let first = buf
        .get_output("p1", None)
        .into_iter()
        .next()
        .expect("non-empty");
    assert_eq!(
        first.line_num, 1001,
        "oldest 1000 lines dropped, newest kept"
    );

    let tail = buf.get_output_tail("p1", 1);
    assert_eq!(tail[0].line_num, 6000);
}

#[test]
fn byte_cap_trims_leading_lines_when_budget_exceeded() {
    let buf = OutputBuffer::new();
    // 400 lines x 10_000 bytes ≈ 4 MB > 2 MB cap → roughly half trimmed.
    let big_line = "x".repeat(10_000 - 1); // +1 for newline in raw payload
    let payload: String = (0..400).map(|_| format!("{big_line}\n")).collect();
    buf.append_output("p1", &payload, None);

    let info = buf.get_pane_buffer_info("p1").expect("pane exists");
    assert!(
        info.total_bytes <= 2_000_000,
        "byte cap enforced, got {}",
        info.total_bytes
    );
    assert!(info.line_count < 400, "leading lines must be trimmed");
    // Newest line survives the trim.
    let tail = buf.get_output_tail("p1", 1);
    assert_eq!(tail[0].line_num, 400);
}

#[test]
fn get_output_filters_then_paginates() {
    let buf = OutputBuffer::new();
    let payload: String = (1..=10).map(|i| format!("L{i}\n")).collect();
    buf.append_output("p1", &payload, None);

    // since_line is exclusive.
    let since = buf.get_output(
        "p1",
        Some(&GetOutputOptions {
            since_line: Some(8),
            ..Default::default()
        }),
    );
    assert_eq!(
        since.iter().map(|l| l.line_num).collect::<Vec<_>>(),
        vec![9, 10]
    );

    // offset applies AFTER since filtering, limit truncates after offset.
    let page = buf.get_output(
        "p1",
        Some(&GetOutputOptions {
            since_line: Some(5),
            offset: Some(1),
            limit: Some(2),
            ..Default::default()
        }),
    );
    assert_eq!(
        page.iter().map(|l| l.line_num).collect::<Vec<_>>(),
        vec![7, 8]
    );

    // offset past the end empties the result instead of panicking.
    let empty = buf.get_output(
        "p1",
        Some(&GetOutputOptions {
            offset: Some(99),
            ..Default::default()
        }),
    );
    assert!(empty.is_empty());
}

#[test]
fn dead_pane_lifecycle_preserves_history_then_cleans_up() {
    let buf = OutputBuffer::new();
    buf.init_pane_buffer("live", "claude-code").unwrap();
    buf.init_pane_buffer("dead", "claude-code").unwrap();
    buf.append_output("dead", "final words\n", None);

    assert!(buf.mark_pane_dead("dead"));
    assert!(
        !buf.mark_pane_dead("ghost"),
        "marking unknown pane returns false"
    );

    // History is readable after death.
    assert_eq!(line_text(&buf, "dead"), vec!["final words"]);
    let info = buf
        .get_pane_buffer_info("dead")
        .expect("dead pane still listed");
    assert!(info.dead);

    // cleanup removes exactly the dead pane.
    assert_eq!(buf.cleanup_dead_panes(), 1);
    assert_eq!(buf.cleanup_dead_panes(), 0, "second cleanup is a no-op");
    assert!(buf.get_pane_buffer_info("dead").is_none());
    assert!(
        buf.get_pane_buffer_info("live").is_some(),
        "live pane untouched"
    );
}

#[test]
fn exit_snapshot_round_trips_through_disk() {
    let tmp = std::env::temp_dir().join(format!(
        "athena-ob-test-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let buf = OutputBuffer::new();
    buf.append_output("p1", "one\ntwo\nthree\n", None);
    buf.capture_exit_snapshot("p1", 2);

    let snap = buf.get_exit_snapshot("p1");
    assert_eq!(
        snap.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
        vec!["two", "three"]
    );

    buf.save_to_disk(&tmp).expect("save");
    let buf2 = OutputBuffer::new();
    assert!(
        buf2.get_exit_snapshot("p1").is_empty(),
        "fresh buffer has no snapshots"
    );
    buf2.load_from_disk(&tmp).expect("load");
    let loaded = buf2.get_exit_snapshot("p1");
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[1].text, "three");
    std::fs::remove_file(&tmp).ok();
}
