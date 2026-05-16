//! ANSI escape code utilities — ported from src/utils/ansi.ts
//!
//! Provides prompt detection and a streaming command parser for terminal output.

use once_cell::sync::Lazy;
use regex::Regex;

// ---------------------------------------------------------------------------
// ANSI stripping
// ---------------------------------------------------------------------------

/// Regex matching common ANSI escape sequences.
static ANSI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap());

/// Strip ANSI escape codes from a string.
pub fn strip_ansi(input: &str) -> String {
    ANSI_RE.replace_all(input, "").to_string()
}

// ---------------------------------------------------------------------------
// Prompt detection
// ---------------------------------------------------------------------------

/// Patterns that indicate a shell prompt line.
static PROMPT_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r#"[$%#]\s*$"#,
        r#"[❯›»➜]\s*$"#,
        r#"\S+@\S+:[^$]*[$%#]\s*$"#,
        r#"\([^)]+\)\s*[$%#]\s*$"#,
        r#"\S+@\S+\s+[$%#>]\s*$"#,
    ]
    .iter()
    .map(|p| Regex::new(p).unwrap())
    .collect()
});

/// Check if a line (after stripping ANSI codes) looks like a shell prompt.
pub fn is_prompt_line(line: &str) -> bool {
    let stripped = strip_ansi(line);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return false;
    }
    PROMPT_PATTERNS.iter().any(|re| re.is_match(trimmed))
}

// ---------------------------------------------------------------------------
// CommandBlockEvent
// ---------------------------------------------------------------------------

/// Events emitted by the `CommandParser`.
#[derive(Debug, Clone)]
pub enum CommandBlockEvent {
    CommandStart { command: String },
    CommandEnd { command: String, output: String },
    Output { output: String },
}

// ---------------------------------------------------------------------------
// CommandParser
// ---------------------------------------------------------------------------

/// Streaming parser that detects command blocks in terminal output.
pub struct CommandParser {
    buffer: String,
    current_command: Option<String>,
    current_output: Vec<String>,
    prompt_seen: bool,
    callback: Box<dyn FnMut(&CommandBlockEvent)>,
}

impl CommandParser {
    pub fn new(callback: Box<dyn FnMut(&CommandBlockEvent)>) -> Self {
        Self {
            buffer: String::new(),
            current_command: None,
            current_output: Vec::new(),
            prompt_seen: false,
            callback,
        }
    }

    /// Feed raw terminal data into the parser.
    pub fn feed(&mut self, data: &str) {
        self.buffer.push_str(data);

        while let Some(newline_pos) = self.buffer.find('\n') {
            let line: String = self.buffer[..=newline_pos].to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();
            self.process_line(&line);
        }

        // Check if the partial buffer line became a prompt
        if is_prompt_line(&self.buffer) {
            self.finish_current_command();
        }
    }

    fn process_line(&mut self, line: &str) {
        if is_prompt_line(line) {
            self.finish_current_command();
            self.prompt_seen = true;
        } else if self.prompt_seen {
            let trimmed = strip_ansi(line).trim().to_string();
            if self.current_command.is_none() && !trimmed.is_empty() {
                self.current_command = Some(trimmed.clone());
                (self.callback)(&CommandBlockEvent::CommandStart { command: trimmed });
            } else if self.current_command.is_some() {
                self.current_output.push(trimmed);
                (self.callback)(&CommandBlockEvent::Output {
                    output: strip_ansi(line).trim().to_string(),
                });
            }
        }
    }

    fn finish_current_command(&mut self) {
        if let Some(cmd) = self.current_command.take() {
            let output = self.current_output.drain(..).collect::<Vec<_>>().join("\n");
            (self.callback)(&CommandBlockEvent::CommandEnd {
                command: cmd,
                output,
            });
        }
        self.current_output.clear();
        self.current_command = None;
    }

    /// Reset the parser state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.current_command = None;
        self.current_output.clear();
        self.prompt_seen = false;
    }
}
