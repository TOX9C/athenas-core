//! Pure command filtering and grouping for the command palette.
//!
//! Owns [`CommandGroup`], the category display order, and [`filter_commands`]
//! (ported from `selectFilteredCommands`). The component store re-exports
//! these so both the store and the palette share one implementation.

use super::{Command, CommandCategory};

/// A group of commands under a label, as displayed in the palette.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandGroup {
    pub label: String,
    pub commands: Vec<Command>,
}

/// Category display order.
const CATEGORY_ORDER: &[CommandCategory] = &[
    CommandCategory::Workspace,
    CommandCategory::Panel,
    CommandCategory::Athena,
    CommandCategory::Terminal,
    CommandCategory::File,
    CommandCategory::Navigation,
    CommandCategory::Settings,
];

fn category_label(cat: &CommandCategory) -> &str {
    match cat {
        CommandCategory::Workspace => "Workspace",
        CommandCategory::Panel => "Panels",
        CommandCategory::Athena => "Athena",
        CommandCategory::Terminal => "Terminal",
        CommandCategory::File => "File",
        CommandCategory::Navigation => "Navigation",
        CommandCategory::Settings => "Settings",
    }
}

/// Filter and group commands for display in the palette.
/// `is_visible` should return `true` for commands whose `when_key` is
/// satisfied (or whose `when_key` is `None`).
pub fn filter_commands(
    commands: &[Command],
    recent_ids: &[String],
    query: &str,
    is_visible: impl Fn(&Command) -> bool,
) -> Vec<CommandGroup> {
    let available: Vec<&Command> = commands
        .iter()
        .filter(|c| c.when_key.is_none() || is_visible(c))
        .collect();

    if query.trim().is_empty() {
        // Show recent, then by category.
        let recent: Vec<Command> = recent_ids
            .iter()
            .filter_map(|rid| available.iter().find(|c| c.id == *rid))
            .map(|c| (*c).clone())
            .collect();

        let recent_set: std::collections::HashSet<&str> =
            recent_ids.iter().map(|s| s.as_str()).collect();
        let non_recent: Vec<&Command> = available
            .iter()
            .filter(|c| !recent_set.contains(c.id.as_str()))
            .copied()
            .collect();

        let mut groups = Vec::new();
        if !recent.is_empty() {
            groups.push(CommandGroup {
                label: "Recent".to_string(),
                commands: recent,
            });
        }

        for cat in CATEGORY_ORDER {
            let cmds: Vec<Command> = non_recent
                .iter()
                .filter(|c| c.category == *cat)
                .map(|c| (*c).clone())
                .collect();
            if !cmds.is_empty() {
                groups.push(CommandGroup {
                    label: category_label(cat).to_string(),
                    commands: cmds,
                });
            }
        }

        return groups;
    }

    // Fuzzy scoring.
    let lower = query.to_lowercase();
    let terms: Vec<&str> = lower.split_whitespace().collect();

    let mut scored: Vec<(i32, Command)> = Vec::new();

    for cmd in &available {
        let label_lower = cmd.label.to_lowercase();
        let desc_lower = cmd.description.as_deref().unwrap_or("").to_lowercase();
        let kw_string = cmd.keywords.join(" ").to_lowercase();
        let mut score: i32 = 0;

        if label_lower.starts_with(&lower) {
            score = 10;
        } else if label_lower.contains(&lower) {
            score = 7;
        }

        if score == 0 && terms.len() > 1 {
            let all_match = terms.iter().all(|t| {
                label_lower.contains(t) || desc_lower.contains(t) || kw_string.contains(t)
            });
            if all_match {
                score = 5;
            }
        }

        if score == 0 {
            // Subsequence match: every char of the query appears in the label
            // in order. Iterate the query by chars (not by byte index — the
            // previous code used `lower.chars().nth(qi).unwrap()` where `qi`
            // was a byte index, which panics for any multi-byte query).
            let mut query_chars = lower.chars();
            let mut next_q = query_chars.next();
            for ch in label_lower.chars() {
                if let Some(q) = next_q {
                    if ch == q {
                        next_q = query_chars.next();
                    }
                }
            }
            if next_q.is_none() {
                // Whole query consumed as a subsequence.
                score = 3;
            }
        }

        if score == 0 && (desc_lower.contains(&lower) || kw_string.contains(&lower)) {
            score = 2;
        }

        let recent_boost: i32 = if recent_ids.iter().any(|r| r == &cmd.id) {
            1
        } else {
            0
        };

        if score + recent_boost > 0 {
            scored.push((score + recent_boost, (*cmd).clone()));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.label.cmp(&b.1.label)));

    let results: Vec<Command> = scored.into_iter().map(|(_, c)| c).collect();

    if results.is_empty() {
        Vec::new()
    } else {
        vec![CommandGroup {
            label: "Results".to_string(),
            commands: results,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(id: &str, label: &str) -> Command {
        Command {
            id: id.to_string(),
            label: label.to_string(),
            category: CommandCategory::Workspace,
            description: None,
            keywords: vec![],
            shortcut: None,
            handler_key: id.to_string(),
            when_key: None,
        }
    }

    /// H8 regression: a multi-byte (non-ASCII) query MUST NOT panic the
    /// subsequence matcher. Previously the matcher did
    /// `lower.chars().nth(qi).unwrap()` where `qi` was a byte index; for any
    /// multi-byte query `qi` exceeds the char count and `nth()` returns
    /// `None`, panicking and aborting the WASM renderer.
    #[test]
    fn fuzzy_match_handles_non_ascii_query_without_panicking() {
        let commands = vec![
            cmd("open", "Open File"),
            cmd("save", "Save Workspace"),
            cmd("term", "New Terminal"),
        ];
        // Accented, CJK, and emoji — all multi-byte in UTF-8. The old code
        // panicked on any of these.
        for q in ["fïlé", "终端", "😀", "café"] {
            // Must not panic. (Result may be empty — that's fine; the point
            // is that non-ASCII input doesn't abort the renderer.)
            let _ = filter_commands(&commands, &[], q, |_| false);
        }
    }

    /// Sanity: ASCII subsequence matching still works after the rewrite.
    #[test]
    fn fuzzy_match_ascii_subsequence_still_works() {
        let commands = vec![cmd("term", "New Terminal"), cmd("save", "Save")];
        // "trm" is a subsequence of "new terminal".
        let groups = filter_commands(&commands, &[], "trm", |_| false);
        assert!(groups
            .iter()
            .any(|g| g.commands.iter().any(|c| c.id == "term")));
    }

    /// When the query is empty, recents come first, then category groups.
    #[test]
    fn empty_query_groups_recents_then_categories() {
        let commands = vec![
            cmd("term", "New Terminal"),
            cmd("open", "Open File"),
            cmd("save", "Save"),
        ];
        let groups = filter_commands(&commands, &["open".to_string()], "", |_| false);
        assert_eq!(groups[0].label, "Recent");
        assert_eq!(groups[0].commands[0].id, "open");
        // Remaining commands appear in category order (Workspace first here).
        assert!(groups.iter().any(|g| g.label == "Workspace"));
    }

    /// `when_key` gates visibility: commands with a when_key are hidden
    /// unless `is_visible` returns true for them.
    #[test]
    fn when_key_requires_visibility_predicate() {
        let mut hidden = cmd("hidden", "Hidden Command");
        hidden.when_key = Some("feature-x".to_string());
        let mut shown = cmd("shown", "Shown Command");
        shown.when_key = Some("feature-x".to_string());
        let commands = vec![hidden.clone(), shown.clone(), cmd("plain", "Plain")];

        // Predicate rejects everything → only the plain command appears.
        let groups = filter_commands(&commands, &[], "", |_| false);
        let flat: Vec<&str> = groups
            .iter()
            .flat_map(|g| g.commands.iter())
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(flat, vec!["plain"]);

        // Predicate accepts feature-x → both gated commands appear too.
        let groups = filter_commands(&commands, &[], "", |c| {
            c.when_key.as_deref() == Some("feature-x")
        });
        let flat: Vec<&str> = groups
            .iter()
            .flat_map(|g| g.commands.iter())
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(flat, vec!["hidden", "shown", "plain"]);
    }
}
