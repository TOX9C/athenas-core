pub mod agent_activity;
pub mod agent_comms;
pub mod agent_detection;
pub mod kanban;
pub mod llm_models;
pub mod mcp;
pub mod notification;
pub mod orchestrator;
pub mod output_buffer;
pub mod plan_manager;
pub mod resume_scanner;
pub mod search;
pub mod shell_hooks;
pub mod shell_integration;
pub mod swarm;
pub mod tool_executor;
mod tool_schema;
pub mod types;

pub use orchestrator::AthenaOrchestrator;
pub use search::{search_code, search_files};
pub use types::*;

#[cfg(test)]
mod tests;
