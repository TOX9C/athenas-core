use dioxus::prelude::*;
use std::collections::VecDeque;

const MAX_BLOCKS: usize = 100;
const MAX_OUTPUT_PER_BLOCK: usize = 50_000;

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalBlock {
    pub id: u64,
    pub command: String,
    pub output: String,
    pub exit_status: Option<i32>,
    pub start_time_ms: u64,
    pub end_time_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerminalBlocksStore {
    pub blocks: VecDeque<TerminalBlock>,
    pub current_command: Option<String>,
    pub current_output: String,
    pub generation: u64,
    pub next_id: u64,
}

impl TerminalBlocksStore {
    pub fn on_prompt_start(&mut self) {
        self.current_output.clear();
        self.current_command = None;
    }

    pub fn on_command_start(&mut self, command: String) {
        while self.blocks.len() >= MAX_BLOCKS {
            self.blocks.pop_front();
        }
        let id = self.next_id;
        self.next_id += 1;
        let block = TerminalBlock {
            id,
            command: command.clone(),
            output: String::new(),
            exit_status: None,
            start_time_ms: js_sys::Date::now() as u64,
            end_time_ms: None,
        };
        self.blocks.push_back(block);
        self.current_command = Some(command);
        self.current_output.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn on_command_finished(&mut self, status: i32) {
        if let Some(block) = self.blocks.back_mut() {
            if block.exit_status.is_none() {
                block.exit_status = Some(status);
                block.end_time_ms = Some(js_sys::Date::now() as u64);
                self.generation = self.generation.wrapping_add(1);
            }
        }
        self.current_output.clear();
        self.current_command = None;
    }

    pub fn append_output(&mut self, text: &str) {
        self.current_output.push_str(text);
        if self.current_output.len() > MAX_OUTPUT_PER_BLOCK {
            let truncate_at = self.current_output.len().saturating_sub(MAX_OUTPUT_PER_BLOCK);
            self.current_output = self.current_output[truncate_at..].to_string();
        }
        if let Some(block) = self.blocks.back_mut() {
            if block.exit_status.is_none() {
                block.output.push_str(text);
                if block.output.len() > MAX_OUTPUT_PER_BLOCK {
                    let truncate_at = block.output.len().saturating_sub(MAX_OUTPUT_PER_BLOCK);
                    block.output = block.output[truncate_at..].to_string();
                }
            }
        }
    }
}

/// Create the signal-based store.
pub fn use_terminal_blocks_store() -> Signal<TerminalBlocksStore> {
    use_context::<Signal<TerminalBlocksStore>>()
}

/// Initialize the store as a context provider.
pub fn provide_terminal_blocks_store() {
    use_context_provider(|| Signal::new(TerminalBlocksStore::default()));
}
