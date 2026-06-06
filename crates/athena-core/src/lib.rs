pub mod agent_comms;
pub mod kanban;
pub mod mcp;
pub mod notification;
pub mod orchestrator;
pub mod output_buffer;
pub mod output_capture;
pub mod plan_manager;
pub mod search;
pub mod shell_hooks;
pub mod shell_integration;
pub mod swarm;
pub mod tool_executor;
pub mod types;

pub use orchestrator::AthenaOrchestrator;
pub use search::{search_code, search_code_sync, search_files};
pub use types::*;

#[cfg(test)]
mod tests;
