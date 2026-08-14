//! Incremental OSC 633 parser and terminal-output sanitization helpers.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const OSC_PREFIX: &str = "\x1b]633;";
const BEL: char = '\x07';
const ST: &str = "\x1b\\";

/// Move a byte index back to the nearest valid UTF-8 boundary.
///
/// Parser offsets come from `str::find` and are normally safe, but the
/// bounded-buffer recovery path derives an offset from `len()`. That offset
/// can land inside a multibyte terminal glyph (for example OMP's box-drawing
/// UI), and slicing there would panic the PTY reader task.
fn floor_char_boundary(input: &str, index: usize) -> usize {
    let mut boundary = index.min(input.len());
    while boundary > 0 && !input.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

// ---------------------------------------------------------------------------
// ShellIntegrationSequence
// ---------------------------------------------------------------------------

/// A parsed shell integration sequence emitted by the terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShellIntegrationSequence {
    Prompt { data: Option<String> },
    Command { data: String },
    CommandStart,
    CommandExecuted,
    CommandFinished { exit_code: i32 },
    Cwd { data: String },
    Property { key: String, value: String },
}

// ---------------------------------------------------------------------------
// ParsedSequence
// ---------------------------------------------------------------------------

/// A parsed sequence together with the raw byte length it occupied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSequence {
    pub sequence: ShellIntegrationSequence,
    pub raw_length: usize,
}

// ---------------------------------------------------------------------------
// Osc633Parser
// ---------------------------------------------------------------------------

/// Incremental parser for OSC 633 escape sequences.
pub struct Osc633Parser {
    buffer: String,
}

impl Osc633Parser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Feed raw terminal data and return all complete parsed sequences.
    pub fn feed(&mut self, data: &str) -> Vec<ParsedSequence> {
        self.buffer.push_str(data);
        let mut results: Vec<ParsedSequence> = Vec::new();

        while !self.buffer.is_empty() {
            let osc_start = match self.buffer.find(OSC_PREFIX) {
                Some(pos) => pos,
                None => {
                    // Prevent unbounded buffer growth without splitting an incomplete OSC
                    // sequence in half. Search backwards from the keep point for the
                    // last complete OSC boundary (BEL or ST terminator) so that the
                    // parser state stays valid.
                    if self.buffer.len() > 10_000 {
                        let mut keep = floor_char_boundary(
                            &self.buffer,
                            self.buffer.len().saturating_sub(4096),
                        );
                        // Walk backward to find the last BEL or ST before keep
                        let tail = &self.buffer[..keep];
                        let last_bel = tail.rfind(BEL);
                        let last_st = tail.rfind(ST);
                        if let Some(pos) = last_bel.max(last_st) {
                            // Advance to just past the terminator
                            keep = pos
                                + if last_bel >= last_st {
                                    BEL.len_utf8()
                                } else {
                                    ST.len()
                                };
                        }
                        // Clamp and re-align to prevent slicing past the end of
                        // the buffer or through a multibyte terminal glyph.
                        keep = floor_char_boundary(&self.buffer, keep.min(self.buffer.len()));
                        self.buffer = self.buffer[keep..].to_string();
                    }
                    break;
                }
            };

            // Discard anything before the OSC prefix
            if osc_start > 0 {
                self.buffer = self.buffer[osc_start..].to_string();
            }

            let payload_start = OSC_PREFIX.len();

            let bel_idx = self.buffer[payload_start..]
                .find(BEL)
                .map(|i| payload_start + i);
            let st_idx = self.buffer[payload_start..]
                .find(ST)
                .map(|i| payload_start + i);

            let (terminator_idx, terminator_len) = match (bel_idx, st_idx) {
                (Some(bi), Some(si)) if bi < si => (bi, 1),
                (Some(_), Some(si)) => (si, 2),
                (Some(bi), None) => (bi, 1),
                (None, Some(si)) => (si, 2),
                (None, None) => {
                    // No terminator found yet
                    if self.buffer.len() > 100_000 {
                        // Try to recover by jumping to the next ESC
                        if let Some(next_esc) = self.buffer[1..].find('\x1b') {
                            self.buffer = self.buffer[next_esc + 1..].to_string();
                            continue;
                        }
                        self.buffer.clear();
                    }
                    break;
                }
            };

            let payload = self.buffer[payload_start..terminator_idx].to_string();
            let raw_length = terminator_idx + terminator_len;
            self.buffer = self.buffer[raw_length..].to_string();

            if let Some(seq) = parse_payload(&payload) {
                results.push(ParsedSequence {
                    sequence: seq,
                    raw_length,
                });
            }
        }

        results
    }

    /// Reset the parser state.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

impl Default for Osc633Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function: parse a single chunk of data and return all sequences.
pub fn parse_osc633(data: &str) -> Vec<ParsedSequence> {
    let mut parser = Osc633Parser::new();
    parser.feed(data)
}

// ---------------------------------------------------------------------------
// parsePayload
// ---------------------------------------------------------------------------

fn parse_payload(payload: &str) -> Option<ShellIntegrationSequence> {
    let (command, rest) = match payload.find(';') {
        Some(idx) => (&payload[..idx], &payload[idx + 1..]),
        None => (payload, ""),
    };

    match command {
        "A" => Some(ShellIntegrationSequence::Prompt { data: None }),
        "B" => Some(ShellIntegrationSequence::Command {
            data: rest.to_string(),
        }),
        "C" => Some(ShellIntegrationSequence::CommandStart),
        "D" => {
            let code = if rest.is_empty() {
                0
            } else {
                rest.parse::<i32>().unwrap_or(0)
            };
            Some(ShellIntegrationSequence::CommandFinished { exit_code: code })
        }
        "E" => Some(ShellIntegrationSequence::CommandExecuted),
        "P" => Some(ShellIntegrationSequence::Cwd {
            data: rest.to_string(),
        }),
        "Is" => Some(ShellIntegrationSequence::Property {
            key: "icon".to_string(),
            value: rest.to_string(),
        }),
        "Set" | "S" => {
            let (key, value) = match rest.find('=') {
                Some(eq) => (rest[..eq].to_string(), rest[eq + 1..].to_string()),
                None => (rest.to_string(), String::new()),
            };
            Some(ShellIntegrationSequence::Property { key, value })
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// stripOsc633
// ---------------------------------------------------------------------------

/// Strip all OSC 633 sequences from a string, returning only the visible text.
pub fn strip_osc633(data: &str) -> String {
    let mut result = String::new();
    let mut pos = 0;

    while pos < data.len() {
        let osc_start = match data[pos..].find(OSC_PREFIX) {
            Some(i) => pos + i,
            None => {
                result.push_str(&data[pos..]);
                break;
            }
        };

        result.push_str(&data[pos..osc_start]);
        let payload_start = osc_start + OSC_PREFIX.len();

        let bel_idx = data[payload_start..]
            .find(BEL)
            .map(|i| payload_start + i + BEL.len_utf8());
        let st_idx = data[payload_start..]
            .find(ST)
            .map(|i| payload_start + i + ST.len());

        match (bel_idx, st_idx) {
            (Some(bi), Some(si)) if bi < si => {
                pos = bi;
            }
            (Some(_bi), Some(si)) => {
                pos = si;
            }
            (Some(bi), None) => {
                pos = bi;
            }
            (None, Some(si)) => {
                pos = si;
            }
            (None, None) => {
                result.push_str(&data[osc_start..]);
                break;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{Osc633Parser, ShellIntegrationSequence};

    #[test]
    fn bounded_recovery_does_not_split_utf8_terminal_glyphs() {
        let mut parser = Osc633Parser::new();
        // OMP emits box-drawing glyphs in its full-screen UI. This forces the
        // parser's no-OSC bounded-buffer recovery path to compute an offset
        // inside a three-byte UTF-8 character.
        parser.feed(&"─".repeat(5_000));
        let sequences = parser.feed("\x1b]633;A\x07");
        assert!(matches!(
            sequences.first().map(|parsed| &parsed.sequence),
            Some(ShellIntegrationSequence::Prompt { .. })
        ));
    }
}
