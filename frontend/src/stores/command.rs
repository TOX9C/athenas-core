use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Command category for grouping in the palette.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CommandCategory {
    #[default]
    Workspace,
    Panel,
    Athena,
    Terminal,
    File,
    Settings,
    Navigation,
}

/// A registrable command in the command palette.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub category: CommandCategory,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub shortcut: Option<String>,
    /// The handler is stored as an index into a callback table because
    /// closures are not `PartialEq`. In the Dioxus integration layer the
    /// actual handler invocation is done via a separate registry.
    pub handler_key: String,
    /// Optional visibility predicate key (same approach as handler).
    pub when_key: Option<String>,
}

/// Maximum number of recent command ids retained.
const MAX_RECENT: usize = 8;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global command palette state.
#[derive(Clone, PartialEq, Default)]
pub struct CommandState {
    pub is_open: bool,
    pub query: String,
    pub commands: Vec<Command>,
    pub recent_ids: Vec<String>,
}

impl CommandState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            query: String::new(),
            commands: Vec::new(),
            recent_ids: Vec::new(),
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
    }

    pub fn toggle(&mut self) {
        let was_open = self.is_open;
        self.is_open = !was_open;
        if self.is_open {
            self.query.clear();
        }
    }

    pub fn set_query(&mut self, q: impl Into<String>) {
        self.query = q.into();
    }

    /// Register a single command. No-op if a command with the same id exists.
    pub fn register_command(&mut self, cmd: Command) {
        if !self.commands.iter().any(|c| c.id == cmd.id) {
            self.commands.push(cmd);
        }
    }

    /// Unregister a command by id.
    pub fn unregister_command(&mut self, id: &str) {
        self.commands.retain(|c| c.id != id);
        self.recent_ids.retain(|rid| rid != id);
    }

    /// Bulk-register commands, skipping duplicates.
    pub fn register_commands(&mut self, cmds: Vec<Command>) {
        let existing_ids: std::collections::HashSet<String> =
            self.commands.iter().map(|c| c.id.clone()).collect();
        for cmd in cmds {
            if !existing_ids.contains(&cmd.id) {
                self.commands.push(cmd);
            }
        }
    }

    /// Record a command execution in the recent list.
    pub fn record_execution(&mut self, id: &str) {
        self.recent_ids.retain(|rid| rid != id);
        self.recent_ids.insert(0, id.to_string());
        self.recent_ids.truncate(MAX_RECENT);
    }
}

// ---------------------------------------------------------------------------
// Filtering (ported from selectFilteredCommands)
// ---------------------------------------------------------------------------

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
            // Fuzzy prefix matching on label.
            let mut qi = 0;
            for ch in label_lower.chars() {
                if qi < lower.len() && ch == lower.chars().nth(qi).unwrap() {
                    qi += 1;
                }
            }
            if qi == lower.len() {
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

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the command palette signal from the Dioxus context.
pub fn use_command_store() -> Signal<CommandState> {
    use_context::<Signal<CommandState>>()
}

/// Initialize the command store as a context provider.
pub fn provide_command_store() {
    use_context_provider(|| Signal::new(CommandState::new()));
}
